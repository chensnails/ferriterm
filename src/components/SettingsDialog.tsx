import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../api";
import { useAppStore } from "../store";
import { Modal } from "./Modal";

export function SettingsDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const theme = useAppStore((s) => s.theme);
  const language = useAppStore((s) => s.language);
  const languagePacks = useAppStore((s) => s.languagePacks);
  const pluginScan = useAppStore((s) => s.pluginScan);
  const loadLanguagePacks = useAppStore((s) => s.loadLanguagePacks);
  const bumpCommandLibrary = useAppStore((s) => s.bumpCommandLibrary);
  const keepaliveSecs = useAppStore((s) => s.keepaliveSecs);
  const setTheme = useAppStore((s) => s.setTheme);
  const setLanguage = useAppStore((s) => s.setLanguage);
  const setKeepalive = useAppStore((s) => s.setKeepalive);
  const loadHosts = useAppStore((s) => s.loadHosts);
  const loadTags = useAppStore((s) => s.loadTags);
  const [passphrase, setPassphrase] = useState("");
  const [pluginMessage, setPluginMessage] = useState("");
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);

  const exportBackup = async () => {
    if (!passphrase) return;
    setBusy(true);
    try {
      const path = await api.exportBackup(passphrase);
      setMessage(t("sync.exported", { path }));
    } catch (err) {
      setMessage(t("sync.failed", { error: String(err) }));
    } finally {
      setBusy(false);
    }
  };

  const importFile = async (file: File) => {
    if (!passphrase) return;
    setBusy(true);
    try {
      const stats = await api.importBackup(await file.text(), passphrase);
      await Promise.all([loadHosts(), loadTags()]);
      setMessage(
        t("sync.imported", {
          added: stats.hostsAdded,
          skipped: stats.hostsSkipped,
        }),
      );
    } catch (err) {
      setMessage(t("sync.failed", { error: String(err) }));
    } finally {
      setBusy(false);
      if (fileRef.current) fileRef.current.value = "";
    }
  };

  const refreshPlugins = async () => {
    try {
      await loadLanguagePacks();
      setPluginMessage(t("plugins.refreshed"));
    } catch (err) {
      setPluginMessage(String(err));
    }
  };

  const openPluginDir = async () => {
    try {
      await api.pluginOpenDir();
    } catch (err) {
      setPluginMessage(String(err));
    }
  };

  const importPack = async (packId: string) => {
    try {
      const count = await api.pluginImportCommands(packId);
      bumpCommandLibrary();
      setPluginMessage(t("plugins.imported", { count }));
    } catch (err) {
      setPluginMessage(String(err));
    }
  };

  return (
    <Modal
      title={t("settings.title")}
      onClose={onClose}
      footer={
        <button type="button" onClick={onClose}>
          {t("settings.close")}
        </button>
      }
    >
      <div className="settings-form">
        <fieldset className="settings-group">
          <legend>{t("settings.theme")}</legend>
          <label className="checkbox">
            <input
              type="radio"
              name="theme"
              checked={theme === "dark"}
              onChange={() => setTheme("dark")}
            />
            <span>{t("settings.themeDark")}</span>
          </label>
          <label className="checkbox">
            <input
              type="radio"
              name="theme"
              checked={theme === "light"}
              onChange={() => setTheme("light")}
            />
            <span>{t("settings.themeLight")}</span>
          </label>
        </fieldset>

        <label className="field">
          <span>{t("settings.language")}</span>
          <select
            value={language}
            onChange={(e) => setLanguage(e.target.value)}
          >
            <option value="en">English</option>
            <option value="zh-CN">简体中文</option>
            {languagePacks
              .filter((pack) => pack.locale !== "en" && pack.locale !== "zh-CN")
              .map((pack) => (
                <option key={pack.id} value={pack.locale}>
                  {pack.name}
                </option>
              ))}
          </select>
          <span className="hint">{t("settings.languagePackHint")}</span>
        </label>

        <label className="field">
          <span>{t("settings.keepalive")}</span>
          <input
            type="number"
            min={0}
            max={600}
            value={keepaliveSecs}
            onChange={(e) =>
              setKeepalive(Number.parseInt(e.target.value, 10) || 0)
            }
          />
          <span className="hint">{t("settings.keepaliveHint")}</span>
        </label>

        <fieldset className="settings-group">
          <legend>{t("sync.title")}</legend>
          <p className="hint">{t("sync.hint")}</p>
          <label className="field">
            <span>{t("sync.passphrase")}</span>
            <input
              type="password"
              value={passphrase}
              onChange={(e) => setPassphrase(e.target.value)}
              placeholder={t("sync.passphrasePlaceholder")}
            />
          </label>
          <div className="sync-actions">
            <button
              type="button"
              onClick={exportBackup}
              disabled={busy || !passphrase}
            >
              {t("sync.export")}
            </button>
            <button
              type="button"
              onClick={() => fileRef.current?.click()}
              disabled={busy || !passphrase}
            >
              {t("sync.import")}
            </button>
            <input
              ref={fileRef}
              type="file"
              accept=".ftbak"
              hidden
              onChange={(e) => {
                const file = e.target.files?.[0];
                if (file) void importFile(file);
              }}
            />
          </div>
          {message && <p className="hint sync-message">{message}</p>}
        </fieldset>

        <fieldset className="settings-group">
          <legend>{t("plugins.title")}</legend>
          <p className="hint">{t("plugins.hint")}</p>
          <div className="plugin-actions">
            <button type="button" onClick={() => void refreshPlugins()}>
              {t("plugins.refresh")}
            </button>
            <button type="button" onClick={() => void openPluginDir()}>
              {t("plugins.openFolder")}
            </button>
          </div>
          {pluginMessage && (
            <p className="hint sync-message">{pluginMessage}</p>
          )}
          {pluginScan && pluginScan.plugins.length > 0 && (
            <ul className="plugin-list">
              {pluginScan.plugins.map((plugin) => (
                <li key={plugin.id} className="plugin-row">
                  <span className="plugin-name">{plugin.name}</span>
                  <span className="plugin-kind">
                    {t(`plugins.kinds.${plugin.kind}`, {
                      defaultValue: plugin.kind,
                    })}
                    {plugin.version ? ` · ${plugin.version}` : ""}
                  </span>
                </li>
              ))}
            </ul>
          )}
          {pluginScan && pluginScan.commandPacks.length > 0 && (
            <ul className="plugin-list">
              {pluginScan.commandPacks.map((pack) => (
                <li key={pack.id} className="plugin-row">
                  <span className="plugin-name">{pack.name}</span>
                  <button
                    type="button"
                    className="link-btn"
                    onClick={() => void importPack(pack.id)}
                  >
                    {t("plugins.importCommands")}
                  </button>
                </li>
              ))}
            </ul>
          )}
          {pluginScan && pluginScan.errors.length > 0 && (
            <details className="plugin-errors">
              <summary>
                {t("plugins.errors", { count: pluginScan.errors.length })}
              </summary>
              <ul>
                {pluginScan.errors.map((error) => (
                  <li key={error}>{error}</li>
                ))}
              </ul>
            </details>
          )}
        </fieldset>

        <fieldset className="settings-group">
          <legend>{t("settings.credentials")}</legend>
          <p className="hint">{t("settings.credentialsHint")}</p>
        </fieldset>
      </div>
    </Modal>
  );
}
