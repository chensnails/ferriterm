import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  aiChatStream,
  aiGenerateCommand,
  aiListProviders,
  api,
  isTauri,
  parseAiSettings,
  type AiChatMessage,
  type AiCommandReply,
  type AiContext,
  type AiProvider,
  type AiSettings,
  DEFAULT_AI_SETTINGS,
} from "../api";
import { terminalHandle } from "../terminal-registry";
import {
  startAgentLoop,
  type AgentLoopHandle,
  type AgentLoopState,
  type AgentStepVerdict,
} from "../agent-loop";
import { runGuarded } from "../policy";
import { useAppStore } from "../store";
import { EmptyState, ErrorState, Notice, Switch } from "./ui";
import { Modal } from "./Modal";
import { SparkleIcon } from "./icons";

/** Glob over `*` / `?`, matched against the tab title. */
function matchesHost(pattern: string, title: string): boolean {
  const source = pattern
    .trim()
    .replace(/[.+^${}()|[\]\\]/g, "\\$&")
    .replace(/\*/g, ".*")
    .replace(/\?/g, ".");
  if (!source) return false;
  try {
    return new RegExp(`^${source}$`, "i").test(title);
  } catch {
    return false;
  }
}

function denied(patterns: string, title: string): boolean {
  return patterns
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean)
    .some((pattern) => matchesHost(pattern, title));
}

/**
 * Split a streamed chat reply into its reasoning (inside <think:6124c78e>…</think:6124c78e>) and
 * the final answer (everything after the closing tag). Until the closing tag
 * arrives we treat the text as provisional reasoning so the panel can show it
 * live; if no opening tag appears at all the whole reply is the answer.
 */
function splitThinking(raw: string): { thinking: string; answer: string } {
  const end = raw.indexOf("</think:6124c78e>");
  if (end === -1) {
    if (/^\s*<think:6124c78e>/i.test(raw)) {
      return { thinking: raw.replace(/^\s*<think:6124c78e>/i, "").trim(), answer: "" };
    }
    return { thinking: "", answer: raw };
  }
  const head = raw.slice(0, end);
  const thinking = head.replace(/^\s*<think:6124c78e>/i, "").trim();
  const answer = raw.slice(end + "</think:6124c78e>".length).trim();
  return { thinking, answer };
}

/**
 * Built-in assistant panel. It never executes anything on its own: it answers,
 * or proposes a single command that the user has to insert into the terminal.
 */
export function AiPanel() {
  const { t } = useTranslation();
  const activeId = useAppStore((s) => s.activeId);
  const sessions = useAppStore((s) => s.sessions);
  const commandHistory = useAppStore((s) => s.commandHistory);
  const setSettingsOpen = useAppStore((s) => s.setSettingsOpen);
  const aiConfigVersion = useAppStore((s) => s.aiConfigVersion);

  const [providers, setProviders] = useState<AiProvider[]>([]);
  const [providerId, setProviderId] = useState<string | null>(null);
  const [settings, setSettings] = useState<AiSettings>(DEFAULT_AI_SETTINGS);
  const [messages, setMessages] = useState<AiChatMessage[]>([]);
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [draft, setDraft] = useState<AiCommandReply | null>(null);
  const [notes, setNotes] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);
  const listRef = useRef<HTMLDivElement>(null);

  // Continuous mode: an autonomous agent loop that drives the active session.
  const [continuousOn, setContinuousOn] = useState(false);
  const [goal, setGoal] = useState("");
  const [agentState, setAgentState] = useState<AgentLoopState | null>(null);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [showThinking, setShowThinking] = useState(true);
  const agentHandleRef = useRef<AgentLoopHandle | null>(null);

  useEffect(() => {
    if (!isTauri) {
      setLoading(false);
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        const [list, stored] = await Promise.all([
          aiListProviders(),
          api.getSettings(),
        ]);
        if (cancelled) return;
        setProviders(list);
        setSettings(parseAiSettings(stored.aiSettings));
        const saved = stored.aiProviderId;
        setProviderId((current) =>
          current && list.some((item) => item.id === current)
            ? current
            : saved && list.some((item) => item.id === saved)
              ? saved
              : (list[0]?.id ?? null),
        );
      } catch (err) {
        if (!cancelled) setError(String(err));
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [aiConfigVersion]);

  useEffect(() => {
    const node = listRef.current;
    if (node) node.scrollTop = node.scrollHeight;
  }, [messages]);

  const provider = providers.find((item) => item.id === providerId) ?? null;
  const session = sessions.find((item) => item.id === activeId) ?? null;
  const title = session?.title ?? "";

  const persistSettings = (next: AiSettings) => {
    setSettings(next);
    if (isTauri) {
      api.setSetting("aiSettings", JSON.stringify(next)).catch(() => {});
    }
  };

  const pickProvider = (id: string) => {
    setProviderId(id);
    if (isTauri) api.setSetting("aiProviderId", id).catch(() => {});
  };

  const buildContext = (): AiContext => {
    const base: AiContext = {
      selection: null,
      lastCommand: null,
      screen: [],
      scope: settings.scope,
      lines: settings.screenLines,
      sendOutput: settings.sendOutput,
      redact: settings.redact,
    };
    if (!settings.enabled || settings.scope === "none") return base;
    if (title && denied(settings.deniedHosts, title)) return base;
    const handle = activeId ? terminalHandle(activeId) : null;
    if (settings.sendOutput && handle) {
      base.screen = handle.screenLines(settings.screenLines);
      base.selection = handle.selectionText() || null;
    }
    const last = [...commandHistory]
      .reverse()
      .find((entry) => entry.sessionId === activeId);
    base.lastCommand = last?.text ?? null;
    return base;
  };

  const send = async () => {
    const text = input.trim();
    if (!text || busy || !providerId) return;
    const prefix = settings.nlPrefix;
    const isCommand = prefix.length > 0 && text.startsWith(prefix);
    setError(null);

    if (isCommand) {
      const request = text.slice(prefix.length).trim();
      if (!request) return;
      setBusy(true);
      try {
        const reply = await aiGenerateCommand(providerId, request, {
          host: title || null,
          shell: null,
          context: buildContext(),
        });
        setDraft(reply);
        setNotes(reply.notes);
        setInput("");
      } catch (err) {
        setError(String(err));
      } finally {
        setBusy(false);
      }
      return;
    }

    const history: AiChatMessage[] = [
      ...messages,
      { role: "user", content: text },
    ];
    setMessages([...history, { role: "assistant", content: "", thinking: "" }]);
    setInput("");
    setBusy(true);
    let raw = "";
    try {
      const reply = await aiChatStream(
        providerId,
        history,
        buildContext(),
        undefined,
        (delta) => {
          raw += delta;
          const { thinking, answer } = splitThinking(raw);
          setMessages((previous) => {
            const copy = previous.slice();
            const last = copy[copy.length - 1];
            copy[copy.length - 1] = {
              role: "assistant",
              content: answer,
              thinking,
            };
            return copy;
          });
        },
      );
      // The vendor may have refused a parameter; say so instead of hiding it.
      setNotes(reply.notes);
    } catch (err) {
      setError(String(err));
      setMessages((previous) => {
        const copy = previous.slice();
        const last = copy[copy.length - 1];
        if (last?.content === "" && !last?.thinking) copy.pop();
        return copy;
      });
    } finally {
      setBusy(false);
    }
  };

  // Typing the line into the terminal without the newline leaves it at the
  // prompt: the person still presses Enter. That keeps "insert" a preview and
  // leaves "run" as the only path that actually executes, which is the path
  // the policy gate guards.
  const insertDraft = () => {
    if (!draft) return;
    const target = activeId ? terminalHandle(activeId) : null;
    if (target) {
      target.insert(draft.command);
      setDraft(null);
    } else {
      setError(t("ai.noSession"));
    }
  };

  const runDraft = async () => {
    if (!draft) return;
    const target = activeId ? terminalHandle(activeId) : null;
    if (!target) {
      setError(t("ai.noSession"));
      return;
    }
    const command = draft.command;
    setError(null);
    const outcome = await runGuarded(command, {
      host: title,
      elevated: false,
      run: () => {
        target.insert(`${command}\n`);
        setDraft(null);
      },
    });
    if (outcome.status === "denied") {
      setError(
        outcome.decision.hard
          ? t("policy.deniedFatal")
          : t("policy.denied", {
              reasons: outcome.decision.reasons
                .map((reason) => t(`policy.reason.${reason}`))
                .join("、"),
            }),
      );
    } else if (outcome.status === "failed") {
      setError(outcome.message);
    }
  };

  const labelForStatus = (status: string): string => {
    switch (status) {
      case "done":
        return t("ai.continuous.status.done");
      case "denied":
        return t("ai.continuous.status.denied");
      case "aborted":
        return t("ai.continuous.status.aborted");
      case "timeout":
        return t("ai.continuous.status.timeout");
      case "errors":
        return t("ai.continuous.status.errors");
      case "blocked":
        return t("ai.continuous.status.blocked");
      case "no-session":
        return t("ai.continuous.status.noSession");
      case "maxSteps":
        return t("ai.continuous.status.maxSteps");
      case "finished":
        return t("ai.continuous.status.finished");
      default:
        return status;
    }
  };

  const labelForVerdict = (verdict: AgentStepVerdict): string => {
    switch (verdict) {
      case "ran":
        return t("ai.continuous.verdict.ran");
      case "ask-cancelled":
        return t("ai.continuous.verdict.askCancelled");
      case "denied":
        return t("ai.continuous.verdict.denied");
      case "failed":
        return t("ai.continuous.verdict.failed");
      case "done":
        return t("ai.continuous.verdict.done");
      case "aborted":
        return t("ai.continuous.verdict.aborted");
      default:
        return verdict;
    }
  };

  /** Backend error tokens become localized text instead of raw English. */
  const labelForNote = (note: string): string => {
    switch (note) {
      case "ai.err.emptyReply":
        return t("ai.err.emptyReply");
      case "ai.err.emptyCommand":
        return t("ai.err.emptyCommand");
      default:
        return note;
    }
  };

  const toggleContinuous = (next: boolean) => {
    setContinuousOn(next);
    // Turning the mode off while a run is live stops the loop rather than
    // leaving it running invisibly in the background.
    if (!next && agentHandleRef.current) {
      agentHandleRef.current.stop();
    }
  };

  const openConfirm = () => {
    if (!activeId) {
      setError(t("ai.noSession"));
      return;
    }
    if (!providerId) return;
    setConfirmOpen(true);
  };

  const startRun = () => {
    setConfirmOpen(false);
    const id = activeId;
    if (!id || !providerId) return;
    const handle = startAgentLoop({
      sessionId: id,
      goal: goal.trim(),
      providerId,
      host: title,
      onUpdate: (next) => setAgentState({ ...next, steps: [...next.steps] }),
    });
    agentHandleRef.current = handle;
  };

  const stopRun = () => {
    agentHandleRef.current?.stop();
  };

  const dismissRun = () => {
    agentHandleRef.current = null;
    setAgentState(null);
  };

  if (!isTauri) {
    return (
      <div className="ai-panel">
        <EmptyState title={t("ai.desktopOnly")} />
      </div>
    );
  }

  if (loading) {
    return (
      <div className="ai-panel">
        <EmptyState title={t("common.loading")} />
      </div>
    );
  }

  if (providers.length === 0) {
    return (
      <div className="ai-panel">
        <EmptyState
          icon={<SparkleIcon size={22} />}
          title={t("ai.noProviderTitle")}
          description={t("ai.noProviderDesc")}
          action={
            <div className="plugin-actions">
              <button
                type="button"
                className="primary"
                onClick={() => setSettingsOpen(true)}
              >
                {t("ai.openSettings")}
              </button>
            </div>
          }
        />
      </div>
    );
  }

  return (
    <div className="ai-panel">
      <div className="ai-toolbar">
        <select
          className="ai-select"
          aria-label={t("ai.providerLabel")}
          value={providerId ?? ""}
          onChange={(event) => pickProvider(event.target.value)}
        >
          {providers.map((item) => (
            <option key={item.id} value={item.id}>
              {item.name}
              {item.hasKey ? "" : ` · ${t("ai.noKey")}`}
            </option>
          ))}
        </select>
        <span className="ai-model" title={provider?.model}>
          {provider?.model}
        </span>
        <button
          type="button"
          className="link-btn"
          onClick={() => setSettingsOpen(true)}
        >
          {t("ai.openSettings")}
        </button>
      </div>

      {provider && !provider.hasKey && (
        <Notice tone="warn">{t("ai.providerNoKey")}</Notice>
      )}

      {notes.length > 0 && (
        <Notice tone="info">
          <span>
            {t("ai.degraded", {
              notes: notes.map((note) => t(note)).join(" \u00b7 "),
            })}
          </span>
        </Notice>
      )}

      {settings.enabled &&
        settings.scope !== "none" &&
        !settings.firstSendConfirmed && (
          <Notice tone="info">
            <span>{t("ai.firstSendWarning")}</span>
            <button
              type="button"
              className="link-btn"
              onClick={() => persistSettings({ ...settings, firstSendConfirmed: true })}
            >
              {t("ai.firstSendAck")}
            </button>
          </Notice>
        )}

      <div className="ai-continuous">
        <div className="ai-continuous-head">
          <Switch
            checked={continuousOn}
            onChange={toggleContinuous}
            label={t("ai.continuous.enable")}
          />
          <span className="ai-continuous-label">{t("ai.continuous.enable")}</span>
        </div>
        <p className="ai-continuous-hint">{t("ai.continuous.enableHint")}</p>

        {continuousOn && !agentState && (
          <div className="ai-continuous-setup">
            <label className="ai-continuous-goal-label" htmlFor="ai-continuous-goal">
              {t("ai.continuous.goalLabel")}
            </label>
            <textarea
              id="ai-continuous-goal"
              className="ai-input ai-continuous-goal"
              rows={2}
              value={goal}
              placeholder={t("ai.continuous.goalPlaceholder")}
              onChange={(event) => setGoal(event.target.value)}
            />
            <div className="ai-composer-bar">
              <span className="ai-hint">{t("ai.continuous.enableHint")}</span>
              <div className="plugin-actions">
                <button
                  type="button"
                  className="primary"
                  disabled={!goal.trim() || !providerId || !activeId}
                  onClick={() => openConfirm()}
                >
                  {t("ai.continuous.start")}
                </button>
              </div>
            </div>
          </div>
        )}

        {agentState && (
          <div className="ai-continuous-run">
            <div className="ai-continuous-status">
              <span className="ai-continuous-status-text">
                {agentState.running
                  ? `${t("ai.continuous.running")} · ${t("ai.continuous.stepCount", {
                      n: agentState.steps.length,
                    })}`
                  : labelForStatus(agentState.status)}
              </span>
              {agentState.running ? (
                <button type="button" className="link-btn danger" onClick={stopRun}>
                  {t("ai.continuous.stop")}
                </button>
              ) : (
                <button type="button" className="link-btn" onClick={dismissRun}>
                  {t("ai.continuous.dismiss")}
                </button>
              )}
            </div>

            <label className="ai-toggle">
              <input
                type="checkbox"
                checked={showThinking}
                onChange={(event) => setShowThinking(event.target.checked)}
              />
              {t("ai.continuous.showThinking")}
            </label>

            {agentState.thinking && (
              <p className="ai-continuous-thinking">{t("ai.continuous.thinkingNow")}</p>
            )}

            <div className="ai-continuous-steps">
              <span className="ai-continuous-steps-title">
                {t("ai.continuous.stepLog")}
              </span>
              {agentState.steps.length === 0 ? (
                <p className="ai-continuous-empty">{t("ai.continuous.stepEmpty")}</p>
              ) : (
                <ul className="ai-step-list">
                  {agentState.steps.map((step) => (
                    <li key={step.index} className={`ai-step ai-step-${step.verdict}`}>
                      <span className="ai-step-verdict">
                        {labelForVerdict(step.verdict)}
                      </span>
                      <code className="ai-step-command">{step.command || "—"}</code>
                      {step.note && (
                        <span className="ai-step-note">{labelForNote(step.note)}</span>
                      )}
                      {showThinking && step.thinking && (
                        <p className="ai-step-thinking">{step.thinking}</p>
                      )}
                    </li>
                  ))}
                </ul>
              )}
            </div>
          </div>
        )}
      </div>

      <div className="ai-messages" ref={listRef}>
        {messages.length === 0 && !draft && (
          <p className="ai-intro">{t("ai.intro", { prefix: settings.nlPrefix })}</p>
        )}
        {messages.map((message, index) => (
          <div key={index} className={`ai-message ${message.role}`}>
            <span className="ai-role">
              {message.role === "user" ? t("ai.you") : t("ai.assistant")}
            </span>
            {message.thinking && (
              <details className="ai-thinking-box" open>
                <summary>{t("ai.thinkingTitle")}</summary>
                <div className="ai-thinking">{message.thinking}</div>
              </details>
            )}
            <div className="ai-bubble">
              {message.content || (busy ? t("ai.thinking") : "")}
            </div>
          </div>
        ))}

        {draft && (
          <div className="ai-draft">
            <span className="ai-role">{t("ai.commandTitle")}</span>
            <pre className="ai-draft-command">{draft.command}</pre>
            {draft.explanation && (
              <details className="ai-thinking-box" open>
                <summary>{t("ai.thinkingTitle")}</summary>
                <div className="ai-thinking">{draft.explanation}</div>
              </details>
            )}
            <div className="ai-draft-actions plugin-actions">
              <button type="button" onClick={insertDraft}>
                {t("ai.insert")}
              </button>
              <button type="button" className="primary" onClick={() => void runDraft()}>
                {t("ai.run")}
              </button>
              <button type="button" onClick={() => setDraft(null)}>
                {t("ai.discard")}
              </button>
            </div>
          </div>
        )}
      </div>

      {error && (
        <ErrorState
          title={t("ai.failed")}
          detail={error}
          action={
            <button type="button" className="link-btn" onClick={() => setError(null)}>
              {t("common.close")}
            </button>
          }
        />
      )}

      <div className="ai-composer">
        <textarea
          className="ai-input"
          rows={2}
          value={input}
          placeholder={t("ai.placeholder", { prefix: settings.nlPrefix })}
          onChange={(event) => setInput(event.target.value)}
          onKeyDown={(event) => {
            if (
              event.key === "Enter" &&
              !event.shiftKey &&
              !event.nativeEvent.isComposing
            ) {
              event.preventDefault();
              void send();
            }
          }}
        />
        <div className="ai-composer-bar">
          <span className="ai-hint">{t("ai.sendHint")}</span>
          <div className="plugin-actions">
            <button
              type="button"
              className="primary"
              disabled={busy || !input.trim() || !providerId}
              onClick={() => void send()}
            >
              {busy ? t("ai.sending") : t("ai.send")}
            </button>
          </div>
        </div>
      </div>

      {confirmOpen && (
        <Modal
          title={t("ai.continuous.confirmTitle")}
          onClose={() => setConfirmOpen(false)}
          footer={
            <div className="plugin-actions">
              <button type="button" onClick={() => setConfirmOpen(false)}>
                {t("ai.cancel")}
              </button>
              <button type="button" className="primary" onClick={() => startRun()}>
                {t("ai.continuous.confirmStart")}
              </button>
            </div>
          }
        >
          <p>{t("ai.continuous.confirmBody")}</p>
          <p className="ai-continuous-risk">{t("ai.continuous.confirmRisks")}</p>
        </Modal>
      )}
    </div>
  );
}
