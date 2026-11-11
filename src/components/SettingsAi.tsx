import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  aiDeleteProvider,
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
  type AiSettings,
  type AiProviderInput,
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

function draftFrom(provider: AiProvider): Draft {
  return { ...provider, key: "" };
}

function draftFromPreset(preset: AiPreset): Draft {
  return {
    id: "",
    name: preset.name,
    kind: preset.kind,
    baseUrl: preset.baseUrl,
    model: preset.model,
    timeoutSecs: 60,
    temperature: 0.2,
    maxTokens: 1024,
    fastModel: "",
    preset: preset.id,
    key: "",
  };
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

  const applyPreset = (id: string) => {
    const preset = presets.find((item) => item.id === id);
    if (!preset || !draft) return;
    setDraft({
      ...draftFromPreset(preset),
      id: draft.id,
      key: draft.key,
    });
  };

  const save = async () => {
    if (!draft) return;
    setBusy(true);
    setMessage(null);
    try {
      const saved = await aiSaveProvider({
        id: draft.id,
        name: draft.name.trim(),
        kind: draft.kind,
        baseUrl: draft.baseUrl.trim(),
        model: draft.model.trim(),
        timeoutSecs: draft.timeoutSecs,
        temperature: draft.temperature,
        maxTokens: draft.maxTokens,
        fastModel: draft.fastModel.trim(),
        preset: draft.preset,
      });
      if (draft.key.trim()) {
        await api.saveSecret(`ai:${saved.id}`, draft.key.trim());
      }
      await reload();
      setDraft(null);
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
      setMessage(t("ai.testOk", { model: reply.model, ms: reply.latencyMs }));
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
        onReset={() => setDraft(draftFromPreset(presets[0] ?? {
          id: "custom",
          name: t("ai.customProvider"),
          kind: "openai",
          baseUrl: "https://",
          model: "",
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
            hint={`${provider.kind} · ${provider.model}`}
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
                onClick={() => setDraft(draftFrom(provider))}
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
                  <option value="openai">OpenAI 兼容</option>
                  <option value="anthropic">Anthropic</option>
                </select>
              </label>
              <label className="ai-form-field">
                <span>{t("ai.model")}</span>
                <input
                  className="ai-field"
                  value={draft.model}
                  onChange={(event) =>
                    setDraft({ ...draft, model: event.target.value })
                  }
                />
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
              <button type="button" onClick={() => setDraft(null)}>
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
