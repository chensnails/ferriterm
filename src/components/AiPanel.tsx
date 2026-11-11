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
import { runGuarded } from "../policy";
import { useAppStore } from "../store";
import { EmptyState, ErrorState, Notice } from "./ui";
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
    setMessages([...history, { role: "assistant", content: "" }]);
    setInput("");
    setBusy(true);
    try {
      const reply = await aiChatStream(
        providerId,
        history,
        buildContext(),
        undefined,
        (delta) => {
          setMessages((previous) => {
            const copy = previous.slice();
            const last = copy[copy.length - 1];
            copy[copy.length - 1] = {
              role: "assistant",
              content: last.content + delta,
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
        if (copy[copy.length - 1]?.content === "") copy.pop();
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

      <div className="ai-messages" ref={listRef}>
        {messages.length === 0 && !draft && (
          <p className="ai-intro">{t("ai.intro", { prefix: settings.nlPrefix })}</p>
        )}
        {messages.map((message, index) => (
          <div key={index} className={`ai-message ${message.role}`}>
            <span className="ai-role">
              {message.role === "user" ? t("ai.you") : t("ai.assistant")}
            </span>
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
              <p className="ai-draft-note">{draft.explanation}</p>
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
    </div>
  );
}
