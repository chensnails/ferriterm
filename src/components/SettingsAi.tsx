import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import {
  aiDeleteProvider,
  aiListModels,
  aiListProviders,
  aiPresets,
  aiSaveProvider,
  aiTestProvider,
  api,
  isTauri,
  parseAiSettings,
  type AiKind,
  type AiPreset,
  type AiProvider,
  type AiProviderInput,
  type AiSettings,
  type AiMaxTokensField,
  type AiReasoning,
  DEFAULT_AI_SETTINGS,
} from "../api";
import { useAppStore } from "../store";
import { Chip, SettingRow, SettingGroup } from "./ui";
import {
  SegmentedSetting,
  SelectSetting,
  ToggleSetting,
} from "./SettingsControls";

interface Draft extends AiProviderInput {
  key: string;
}

const EMPTY_PROVIDER: AiProviderInput = {
  id: "",
  name: "",
  kind: "openai",
  baseUrl: "https://",
  model: "",
  timeoutSecs: 60,
  temperature: 0.2,
  maxTokens: 1024,
  fastModel: "",
  preset: "custom",
  maxTokensField: "auto",
  sendSampling: true,
  topP: null,
  stop: [],
  foldSystem: false,
  streamUsage: false,
  reasoning: "off",
  apiVersion: "",
  extraHeaders: [],
  proxy: "",
};

function draftFrom(provider: AiProvider): Draft {
  return { ...provider, key: "" };
}

function draftFromPreset(preset: AiPreset): Draft {
  return {
    ...EMPTY_PROVIDER,
    ...preset.provider,
    id: "",
    preset: preset.id,
    key: "",
  };
}

/** Drop the form-only key field before handing the draft to the backend. */
function toInput(draft: Draft): AiProviderInput {
  const { key: _key, ...rest } = draft;
  return rest;
}

function headersToText(draft: Draft): string {
  return draft.extraHeaders.map((pair) => `${pair.name}: ${pair.value}`).join("\n");
}

function textToHeaders(text: string) {
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0)
    .map((line) => {
      const split = line.indexOf(":");
      if (split < 0) return { name: line, value: "" };
      return {
        name: line.slice(0, split).trim(),
        value: line.slice(split + 1).trim(),
      };
    })
    .filter((pair) => pair.name.length > 0);
}

/** Model picker drawn by the app instead of the WebView. A native
 *  `<datalist>` popup ignores the theme, the type scale and the radius system,
 *  so the field owns a listbox that anchors to the input and follows scroll. */
function ModelCombo({
  value,
  options,
  onChange,
}: {
  value: string;
  options: string[];
  onChange: (value: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const [box, setBox] = useState<{
    left: number;
    top: number;
    width: number;
    flip: boolean;
  } | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const popRef = useRef<HTMLDivElement>(null);

  // Typing filters the catalogue; a value that already names one entry counts
  // as "chosen", so reopening the list offers the whole catalogue again.
  const query = value.trim().toLowerCase();
  const chosen = options.some((option) => option.toLowerCase() === query);
  const list = (
    query.length > 0 && !chosen
      ? options.filter((option) => option.toLowerCase().includes(query))
      : options
  ).slice(0, 300);

  const place = useCallback(() => {
    const input = inputRef.current;
    if (!input) return;
    const rect = input.getBoundingClientRect();
    const room = 260;
    const below = window.innerHeight - rect.bottom;
    const flip = below < room && rect.top > below;
    setBox({
      left: rect.left,
      top: flip ? rect.top - 6 : rect.bottom + 6,
      width: rect.width,
      flip,
    });
  }, []);

  useEffect(() => {
    if (!open) return;
    place();
    const onScroll = () => place();
    const onDown = (event: MouseEvent) => {
      const target = event.target as Node;
      if (inputRef.current?.contains(target)) return;
      if (popRef.current?.contains(target)) return;
      setOpen(false);
    };
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("resize", onScroll);
    window.addEventListener("mousedown", onDown);
    return () => {
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("resize", onScroll);
      window.removeEventListener("mousedown", onDown);
    };
  }, [open, place]);

  useEffect(() => {
    setActive(0);
  }, [value, open]);

  const choose = (model: string) => {
    onChange(model);
    setOpen(false);
    inputRef.current?.focus();
  };

  const visible = open && list.length > 0 && box !== null;

  return (
    <div className="ai-combo">
      <input
        ref={inputRef}
        className="ai-field"
        role="combobox"
        aria-expanded={visible}
        aria-controls={visible ? "ai-model-listbox" : undefined}
        aria-autocomplete="list"
        autoComplete="off"
        value={value}
        onChange={(event) => {
          onChange(event.target.value);
          setOpen(true);
        }}
        onFocus={() => {
          if (options.length > 0) setOpen(true);
        }}
        onKeyDown={(event) => {
          if (event.key === "ArrowDown" || event.key === "ArrowUp") {
            if (options.length === 0) return;
            event.preventDefault();
            if (!open) {
              setOpen(true);
              return;
            }
            const delta = event.key === "ArrowDown" ? 1 : -1;
            setActive((current) => (current + delta + list.length) % list.length);
          } else if (event.key === "Enter" && visible) {
            event.preventDefault();
            choose(list[Math.min(active, list.length - 1)]);
          } else if (event.key === "Escape" && open) {
            // Keep the settings dialog open: the modal listens on window.
            event.preventDefault();
            event.stopPropagation();
            setOpen(false);
          } else if (event.key === "Tab") {
            setOpen(false);
          }
        }}
      />
      {visible &&
        createPortal(
          <div
            ref={popRef}
            id="ai-model-listbox"
            role="listbox"
            aria-label={value}
            className={`ai-combo-popup${box.flip ? " flip" : ""}`}
            style={{ left: box.left, top: box.top, width: box.width }}
            onMouseDown={(event) => event.preventDefault()}
          >
            {list.map((model, index) => (
              <button
                key={model}
                type="button"
                role="option"
                aria-selected={model === value}
                className={[
                  "ai-combo-option",
                  index === active ? "active" : "",
                  model === value ? "selected" : "",
                ]
                  .filter(Boolean)
                  .join(" ")}
                onMouseEnter={() => setActive(index)}
                onClick={() => choose(model)}
              >
                {model}
              </button>
            ))}
          </div>,
          // The settings pane scrolls and clips its own overflow, so the list
          // has to live outside it to stay whole.
          document.body,
        )}
    </div>
  );
}

/** Settings → AI: model providers, redaction and context policy. */
export function SettingsAi() {
  const { t } = useTranslation();
  const askConfirm = useAppStore((s) => s.askConfirm);
  const bumpAiConfig = useAppStore((s) => s.bumpAiConfig);

  const [providers, setProviders] = useState<AiProvider[]>([]);
  const [presets, setPresets] = useState<AiPreset[]>([]);
  const [settings, setSettings] = useState<AiSettings>(DEFAULT_AI_SETTINGS);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [busy, setBusy] = useState(false);
  const [busyModels, setBusyModels] = useState(false);
  const [models, setModels] = useState<string[]>([]);
  const [modelNote, setModelNote] = useState<string | null>(null);
  const [headersText, setHeadersText] = useState("");
  const [message, setMessage] = useState<string | null>(null);

  const reload = async () => {
    const [list, stored] = await Promise.all([
      aiListProviders(),
      api.getSettings(),
    ]);
    setProviders(list);
    setSettings(parseAiSettings(stored.aiSettings));
    bumpAiConfig();
  };

  useEffect(() => {
    if (!isTauri) return;
    void (async () => {
      setPresets(await aiPresets());
      await reload();
    })().catch((err) => setMessage(String(err)));
  }, []);

  const persist = (next: AiSettings) => {
    setSettings(next);
    if (isTauri) {
      api.setSetting("aiSettings", JSON.stringify(next)).catch(() => {});
    }
  };

  const openDraft = (next: Draft) => {
    setDraft(next);
    setModels([]);
    setModelNote(null);
    setHeadersText(headersToText(next));
  };

  const applyPreset = (id: string) => {
    const preset = presets.find((item) => item.id === id);
    if (!preset || !draft) return;
    openDraft({ ...draftFromPreset(preset), id: draft.id, key: draft.key });
  };

  const kindLabel = (kind: AiKind) => t(`ai.kind_${kind}`);

  const loadModels = async () => {
    if (!draft) return;
    setBusyModels(true);
    setModelNote(null);
    try {
      const list = await aiListModels(toInput(draft), draft.key.trim() || undefined);
      setModels(list);
      setModelNote(t("ai.modelsLoaded", { count: list.length }));
    } catch (err) {
      setModels([]);
      setModelNote(t("ai.modelsFailed", { error: String(err) }));
    } finally {
      setBusyModels(false);
    }
  };

  const save = async () => {
    if (!draft) return;
    setBusy(true);
    setMessage(null);
    try {
      const saved = await aiSaveProvider(toInput(draft));
      if (draft.key.trim()) {
        await api.saveSecret(`ai:${saved.id}`, draft.key.trim());
      }
      await reload();
      setDraft(null);
      setHeadersText("");
    } catch (err) {
      setMessage(String(err));
    } finally {
      setBusy(false);
    }
  };

  const remove = async (provider: AiProvider) => {
    const ok = await askConfirm({
      title: t("ai.deleteTitle", { name: provider.name }),
      body: t("ai.deleteBody"),
      danger: true,
      confirmLabel: t("ai.delete"),
    });
    if (!ok) return;
    try {
      await aiDeleteProvider(provider.id);
      await reload();
    } catch (err) {
      setMessage(String(err));
    }
  };

  const test = async (provider: AiProvider) => {
    setMessage(t("ai.testing", { name: provider.name }));
    try {
      const reply = await aiTestProvider(provider.id);
      const notes = reply.notes.length
        ? ` · ${reply.notes.map((note) => t(note)).join("、")}`
        : "";
      setMessage(t("ai.testOk", { model: reply.model, ms: reply.latencyMs }) + notes);
    } catch (err) {
      setMessage(t("ai.testFailed", { error: String(err) }));
    }
  };

  if (!isTauri) {
    return <p className="hint">{t("ai.desktopOnly")}</p>;
  }

  return (
    <>
      <SettingGroup title={t("ai.settingsGroup")} hint={t("ai.settingsHint")}>
        <ToggleSetting
          title={t("ai.enabled")}
          hint={t("ai.enabledHint")}
          checked={settings.enabled}
          onChange={(enabled) => persist({ ...settings, enabled })}
        />
        <ToggleSetting
          title={t("ai.redact")}
          hint={t("ai.redactHint")}
          checked={settings.redact}
          onChange={(redact) => persist({ ...settings, redact })}
        />
        <SegmentedSetting
          title={t("ai.scope")}
          hint={t("ai.scopeHint")}
          value={settings.scope}
          onChange={(value) =>
            persist({ ...settings, scope: value as AiSettings["scope"] })
          }
          options={[
            { value: "none", label: t("ai.scopeNone") },
            { value: "command", label: t("ai.scopeCommand") },
            { value: "selection", label: t("ai.scopeSelection") },
            { value: "screen", label: t("ai.scopeScreen") },
          ]}
        />
        <SelectSetting
          title={t("ai.screenLines")}
          hint={t("ai.screenLinesHint")}
          value={String(settings.screenLines)}
          onChange={(value) =>
            persist({ ...settings, screenLines: Number(value) })
          }
          options={[10, 20, 40, 80].map((value) => ({
            value: String(value),
            label: String(value),
          }))}
        />
        <ToggleSetting
          title={t("ai.sendOutput")}
          hint={t("ai.sendOutputHint")}
          checked={settings.sendOutput}
          onChange={(sendOutput) => persist({ ...settings, sendOutput })}
        />
        <SettingRow title={t("ai.deniedHosts")} hint={t("ai.deniedHostsHint")}>
          <input
            className="ai-field"
            value={settings.deniedHosts}
            placeholder="prod-*, db-*"
            onChange={(event) =>
              persist({ ...settings, deniedHosts: event.target.value })
            }
          />
        </SettingRow>
        <SettingRow title={t("ai.nlPrefix")} hint={t("ai.nlPrefixHint")}>
          <input
            className="ai-field ai-field-short"
            value={settings.nlPrefix}
            maxLength={3}
            onChange={(event) =>
              persist({ ...settings, nlPrefix: event.target.value })
            }
          />
        </SettingRow>
      </SettingGroup>

      <SettingGroup
        title={t("ai.providers")}
        hint={t("ai.providersHint")}
        onReset={() => openDraft(draftFromPreset(presets[0] ?? {
          id: "custom",
          name: t("ai.customProvider"),
          provider: EMPTY_PROVIDER,
        }))}
        resetLabel={t("ai.addProvider")}
      >
        {providers.length === 0 && !draft && (
          <p className="hint">{t("ai.noProviders")}</p>
        )}
        {providers.map((provider) => (
          <SettingRow
            key={provider.id}
            title={provider.name}
            hint={`${kindLabel(provider.kind)} · ${provider.model}`}
          >
            <div className="ai-provider-actions">
              <Chip tone={provider.hasKey ? "info" : "warn"}>
                {provider.hasKey ? t("ai.keySaved") : t("ai.noKey")}
              </Chip>
              <button
                type="button"
                className="link-btn"
                onClick={() => void test(provider)}
              >
                {t("ai.test")}
              </button>
              <button
                type="button"
                className="link-btn"
                onClick={() => openDraft(draftFrom(provider))}
              >
                {t("ai.edit")}
              </button>
              <button
                type="button"
                className="link-btn danger"
                onClick={() => void remove(provider)}
              >
                {t("ai.delete")}
              </button>
            </div>
          </SettingRow>
        ))}

        {draft && (
          <div className="ai-form">
            <div className="ai-form-grid">
              <label className="ai-form-field">
                <span>{t("ai.preset")}</span>
                <select
                  className="ai-field"
                  value={draft.preset}
                  onChange={(event) => applyPreset(event.target.value)}
                >
                  {presets.map((preset) => (
                    <option key={preset.id} value={preset.id}>
                      {preset.name}
                    </option>
                  ))}
                  {!presets.some((preset) => preset.id === draft.preset) && (
                    <option value={draft.preset}>{draft.preset}</option>
                  )}
                </select>
              </label>
              <label className="ai-form-field">
                <span>{t("ai.name")}</span>
                <input
                  className="ai-field"
                  value={draft.name}
                  onChange={(event) =>
                    setDraft({ ...draft, name: event.target.value })
                  }
                />
              </label>
              <label className="ai-form-field">
                <span>{t("ai.kind")}</span>
                <select
                  className="ai-field"
                  value={draft.kind}
                  onChange={(event) =>
                    setDraft({ ...draft, kind: event.target.value as AiKind })
                  }
                >
                  <option value="openai">{t("ai.kind_openai")}</option>
                  <option value="anthropic">{t("ai.kind_anthropic")}</option>
                  <option value="azure">{t("ai.kind_azure")}</option>
                </select>
              </label>
              <label className="ai-form-field">
                <span>{t("ai.model")}</span>
                <div className="ai-model-row">
                  <ModelCombo
                    value={draft.model}
                    options={models}
                    onChange={(model) => setDraft({ ...draft, model })}
                  />
                  <button
                    type="button"
                    className="link-btn"
                    disabled={busyModels}
                    onClick={() => void loadModels()}
                  >
                    {busyModels ? t("ai.loadingModels") : t("ai.loadModels")}
                  </button>
                </div>
              </label>
              <label className="ai-form-field ai-form-wide">
                <span>{t("ai.baseUrl")}</span>
                <input
                  className="ai-field"
                  value={draft.baseUrl}
                  onChange={(event) =>
                    setDraft({ ...draft, baseUrl: event.target.value })
                  }
                />
              </label>
              <label className="ai-form-field ai-form-wide">
                <span>{t("ai.apiKey")}</span>
                <input
                  className="ai-field"
                  type="password"
                  autoComplete="off"
                  value={draft.key}
                  placeholder={
                    draft.id ? t("ai.apiKeyKeep") : t("ai.apiKeyPlaceholder")
                  }
                  onChange={(event) =>
                    setDraft({ ...draft, key: event.target.value })
                  }
                />
              </label>
            </div>
            <p className="hint">{t("ai.baseUrlHint")}</p>
            {modelNote && <p className="hint">{modelNote}</p>}

            <details className="ai-advanced">
              <summary>{t("ai.advanced")}</summary>
              <p className="hint">{t("ai.advancedHint")}</p>
              <div className="ai-form-grid">
                <label className="ai-form-field">
                  <span>{t("ai.temperature")}</span>
                  <input
                    className="ai-field"
                    type="number"
                    step="0.1"
                    min="0"
                    max="2"
                    value={draft.temperature}
                    onChange={(event) =>
                      setDraft({
                        ...draft,
                        temperature: Number(event.target.value),
                      })
                    }
                  />
                </label>
                <label className="ai-form-field">
                  <span>{t("ai.maxTokens")}</span>
                  <input
                    className="ai-field"
                    type="number"
                    min="16"
                    value={draft.maxTokens}
                    onChange={(event) =>
                      setDraft({ ...draft, maxTokens: Number(event.target.value) })
                    }
                  />
                </label>
                <label className="ai-form-field">
                  <span>{t("ai.timeout")}</span>
                  <input
                    className="ai-field"
                    type="number"
                    min="5"
                    max="600"
                    value={draft.timeoutSecs}
                    onChange={(event) =>
                      setDraft({
                        ...draft,
                        timeoutSecs: Number(event.target.value),
                      })
                    }
                  />
                </label>
                <label className="ai-form-field">
                  <span>{t("ai.topP")}</span>
                  <input
                    className="ai-field"
                    type="number"
                    step="0.05"
                    min="0"
                    max="1"
                    value={draft.topP ?? ""}
                    placeholder={t("ai.notSent")}
                    onChange={(event) =>
                      setDraft({
                        ...draft,
                        topP:
                          event.target.value === ""
                            ? null
                            : Number(event.target.value),
                      })
                    }
                  />
                </label>
                <label className="ai-form-field">
                  <span>{t("ai.maxTokensField")}</span>
                  <select
                    className="ai-field"
                    value={draft.maxTokensField}
                    onChange={(event) =>
                      setDraft({
                        ...draft,
                        maxTokensField: event.target.value as AiMaxTokensField,
                      })
                    }
                  >
                    <option value="auto">{t("ai.maxTokensField_auto")}</option>
                    <option value="maxTokens">max_tokens</option>
                    <option value="maxCompletionTokens">
                      max_completion_tokens
                    </option>
                  </select>
                </label>
                <label className="ai-form-field">
                  <span>{t("ai.reasoning")}</span>
                  <select
                    className="ai-field"
                    value={draft.reasoning}
                    onChange={(event) =>
                      setDraft({
                        ...draft,
                        reasoning: event.target.value as AiReasoning,
                      })
                    }
                  >
                    <option value="off">{t("ai.reasoningOff")}</option>
                    <option value="low">low</option>
                    <option value="medium">medium</option>
                    <option value="high">high</option>
                  </select>
                </label>
                {draft.kind === "azure" && (
                  <label className="ai-form-field">
                    <span>{t("ai.apiVersion")}</span>
                    <input
                      className="ai-field"
                      value={draft.apiVersion}
                      placeholder="2024-10-21"
                      onChange={(event) =>
                        setDraft({ ...draft, apiVersion: event.target.value })
                      }
                    />
                  </label>
                )}
                <label className="ai-form-field">
                  <span>{t("ai.fastModel")}</span>
                  <input
                    className="ai-field"
                    value={draft.fastModel}
                    placeholder={t("ai.fastModelHint")}
                    onChange={(event) =>
                      setDraft({ ...draft, fastModel: event.target.value })
                    }
                  />
                </label>
                <label className="ai-form-field ai-form-wide">
                  <span>{t("ai.stop")}</span>
                  <input
                    className="ai-field"
                    value={draft.stop.join(", ")}
                    placeholder={t("ai.stopHint")}
                    onChange={(event) =>
                      setDraft({
                        ...draft,
                        stop: event.target.value
                          .split(",")
                          .map((item) => item.trim())
                          .filter((item) => item.length > 0),
                      })
                    }
                  />
                </label>
                <label className="ai-form-field ai-form-wide">
                  <span>{t("ai.extraHeaders")}</span>
                  <textarea
                    className="ai-field ai-field-area"
                    rows={3}
                    value={headersText}
                    placeholder={t("ai.extraHeadersHint")}
                    onChange={(event) => {
                      setHeadersText(event.target.value);
                      setDraft({
                        ...draft,
                        extraHeaders: textToHeaders(event.target.value),
                      });
                    }}
                  />
                </label>
                <label className="ai-form-field ai-form-wide">
                  <span>{t("ai.proxy")}</span>
                  <input
                    className="ai-field"
                    value={draft.proxy}
                    placeholder="http://127.0.0.1:7890"
                    onChange={(event) =>
                      setDraft({ ...draft, proxy: event.target.value })
                    }
                  />
                </label>
              </div>
              <div className="ai-toggle-list">
                <label className="ai-toggle">
                  <input
                    type="checkbox"
                    checked={draft.sendSampling}
                    onChange={(event) =>
                      setDraft({
                        ...draft,
                        sendSampling: event.target.checked,
                      })
                    }
                  />
                  <span>{t("ai.sendSampling")}</span>
                </label>
                <label className="ai-toggle">
                  <input
                    type="checkbox"
                    checked={draft.foldSystem}
                    onChange={(event) =>
                      setDraft({ ...draft, foldSystem: event.target.checked })
                    }
                  />
                  <span>{t("ai.foldSystem")}</span>
                </label>
                <label className="ai-toggle">
                  <input
                    type="checkbox"
                    checked={draft.streamUsage}
                    onChange={(event) =>
                      setDraft({ ...draft, streamUsage: event.target.checked })
                    }
                  />
                  <span>{t("ai.streamUsage")}</span>
                </label>
              </div>
            </details>

            <p className="hint">{t("ai.keyVaultHint")}</p>
            <div className="ai-form-actions plugin-actions">
              <button
                type="button"
                className="primary"
                disabled={busy}
                onClick={() => void save()}
              >
                {busy ? t("ai.saving") : t("ai.save")}
              </button>
              <button
                type="button"
                onClick={() => {
                  setDraft(null);
                  setHeadersText("");
                  setModelNote(null);
                }}
              >
                {t("ai.cancel")}
              </button>
            </div>
          </div>
        )}
      </SettingGroup>

      {message && <p className="hint">{message}</p>}
    </>
  );
}
