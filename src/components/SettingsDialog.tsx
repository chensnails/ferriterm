import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, isTauri, type RemoteConfig } from "../api";
import { useAppStore, type TabNumbering } from "../store";
import { BUILTIN_THEMES } from "../theme";
import { Modal } from "./Modal";
import { ShortcutEditor } from "./ShortcutEditor";

const SETTINGS_CATEGORIES = [
  "general",
  "terminal",
  "keyboard",
  "transfer",
  "plugins",
  "sync",
  "about",
] as const;

type SettingsCategory = (typeof SETTINGS_CATEGORIES)[number];

export function SettingsDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const [category, setCategory] = useState<SettingsCategory>("general");
  const theme = useAppStore((s) => s.theme);
  const language = useAppStore((s) => s.language);
  const languagePacks = useAppStore((s) => s.languagePacks);
  const pluginScan = useAppStore((s) => s.pluginScan);
  const themes = pluginScan?.themes ?? [];
  const loadLanguagePacks = useAppStore((s) => s.loadLanguagePacks);
  const bumpCommandLibrary = useAppStore((s) => s.bumpCommandLibrary);
  const keepaliveSecs = useAppStore((s) => s.keepaliveSecs);
  const setTheme = useAppStore((s) => s.setTheme);
  const setLanguage = useAppStore((s) => s.setLanguage);
  const setKeepalive = useAppStore((s) => s.setKeepalive);
  const tabNumbering = useAppStore((s) => s.tabNumbering);
  const setTabNumbering = useAppStore((s) => s.setTabNumbering);
  const confirmClose = useAppStore((s) => s.confirmClose);
  const setConfirmClose = useAppStore((s) => s.setConfirmClose);
  const sftpShowHidden = useAppStore((s) => s.sftpShowHidden);
  const setSftpShowHidden = useAppStore((s) => s.setSftpShowHidden);
  const resetShortcuts = useAppStore((s) => s.resetShortcuts);
  const loadHosts = useAppStore((s) => s.loadHosts);
  const loadTags = useAppStore((s) => s.loadTags);
  const [passphrase, setPassphrase] = useState("");
  const [pluginMessage, setPluginMessage] = useState("");
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);
  const [remoteKind, setRemoteKind] = useState<"gist" | "webdav">("gist");
  const [gistToken, setGistToken] = useState("");
  const [gistId, setGistId] = useState("");
  const [gistFile, setGistFile] = useState("ferriterm-backup.ftbak");
  const [davUrl, setDavUrl] = useState("");
  const [davUser, setDavUser] = useState("");
  const [davPass, setDavPass] = useState("");
  const [davFile, setDavFile] = useState("ferriterm-backup.ftbak");
  const [remoteMessage, setRemoteMessage] = useState("");
  const [remoteBusy, setRemoteBusy] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!isTauri) return;
    void (async () => {
      try {
        const settings = await api.getSettings();
        setRemoteKind(settings["sync:provider"] === "webdav" ? "webdav" : "gist");
        setGistId(settings["sync:gist-id"] ?? "");
        setGistFile(settings["sync:gist-file"] || "ferriterm-backup.ftbak");
        setDavUrl(settings["sync:webdav-url"] ?? "");
        setDavUser(settings["sync:webdav-user"] ?? "");
        setDavFile(settings["sync:webdav-file"] || "ferriterm-backup.ftbak");
        const [token, password] = await Promise.all([
          api.getCredential("sync:gist-token"),
          api.getCredential("sync:webdav-password"),
        ]);
        setGistToken(token ?? "");
        setDavPass(password ?? "");
      } catch (err) {
        setRemoteMessage(String(err));
      }
    })();
  }, []);

  const remoteConfig = (): RemoteConfig =>
    remoteKind === "gist"
      ? { kind: "gist", token: gistToken, gistId, filename: gistFile }
      : {
          kind: "webdav",
          url: davUrl,
          username: davUser,
          password: davPass,
          filename: davFile,
        };

  const persistRemote = async () => {
    await Promise.all([
      api.setSetting("sync:provider", remoteKind),
      api.setSetting("sync:gist-id", gistId),
      api.setSetting("sync:gist-file", gistFile),
      api.setSetting("sync:webdav-url", davUrl),
      api.setSetting("sync:webdav-user", davUser),
      api.setSetting("sync:webdav-file", davFile),
      api.saveCredential("sync:gist-token", gistToken),
      api.saveCredential("sync:webdav-password", davPass),
    ]);
  };

  const runRemote = async (action: "test" | "upload" | "download") => {
    if (!isTauri) {
      setRemoteMessage(t("sync.desktopOnly"));
      return;
    }
    if (action !== "test" && !passphrase) {
      setRemoteMessage(t("sync.passphraseRequired"));
      return;
    }
    setRemoteBusy(true);
    setRemoteMessage("");
    try {
      await persistRemote();
      if (action === "test") {
        const detail = await api.syncRemoteTest(remoteConfig());
        setRemoteMessage(t("sync.testOk", { detail }));
      } else if (action === "upload") {
        const result = await api.syncRemoteUpload(remoteConfig(), passphrase);
        if (remoteKind === "gist" && result.id && result.id !== gistId) {
          setGistId(result.id);
          await api.setSetting("sync:gist-id", result.id);
        }
        setRemoteMessage(
          t("sync.uploaded", { location: result.location, bytes: result.bytes }),
        );
      } else {
        const stats = await api.syncRemoteDownload(remoteConfig(), passphrase);
        await Promise.all([loadHosts(), loadTags()]);
        setRemoteMessage(
          t("sync.downloaded", {
            added: stats.hostsAdded,
            skipped: stats.hostsSkipped,
          }),
        );
      }
    } catch (err) {
      setRemoteMessage(t("sync.failed", { error: String(err) }));
    } finally {
      setRemoteBusy(false);
    }
  };

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

  const runPlugin = async (id: string) => {
    try {
      const result = await api.pluginRunWasm(id, "");
      const text = result.output || result.logs.join("\n");
      setPluginMessage(
        text ? t("plugins.ran", { output: text }) : t("plugins.noOutput"),
      );
    } catch (err) {
      setPluginMessage(String(err));
    }
  };

  return (
    <Modal
      title={t("settings.title")}
      onClose={onClose}
      className="settings"
      footer={
        <button type="button" onClick={onClose}>
          {t("settings.close")}
        </button>
      }
    >
      <div className="settings-center">
        <nav className="settings-nav" aria-label={t("settings.title")}>
          {SETTINGS_CATEGORIES.map((item) => (
            <button
              key={item}
              type="button"
              className={category === item ? "active" : ""}
              aria-current={category === item ? "page" : undefined}
              onClick={() => setCategory(item)}
            >
              {t(`settings.cat.${item}`)}
            </button>
          ))}
        </nav>
        <div className="settings-pane">
          {category === "general" && (
            <>
        <fieldset className="settings-group">
          <legend>{t("settings.theme")}</legend>
          <label className="field">
            <span>{t("settings.themeTitle")}</span>
            <select
              value={theme}
              onChange={(e) => setTheme(e.target.value)}
            >
              <optgroup label={t("settings.themeBuiltin")}>
                {BUILTIN_THEMES.map((item) => (
                  <option key={item.id} value={item.id}>
                    {t(`settings.theme_${item.id}`)}
                  </option>
                ))}
              </optgroup>
              {themes.length > 0 && (
                <optgroup label={t("settings.themePacks")}>
                  {themes.map((pack) => (
                    <option key={pack.id} value={pack.id}>
                      {pack.name}
                    </option>
                  ))}
                </optgroup>
              )}
            </select>
          </label>
          {themes.length === 0 ? (
            <p className="hint">{t("settings.themePackHint")}</p>
          ) : (
            <ul className="plugin-list">
              {themes.map((pack) => (
                <li key={pack.id} className="plugin-row">
                  <span className="plugin-name">{pack.name}</span>
                  <span className="plugin-kind">
                    {t(`settings.theme_${pack.base}`)}
                    {pack.version ? ` · ${pack.version}` : ""}
                  </span>
                  {theme === pack.id ? (
                    <span className="hint">{t("settings.themeActive")}</span>
                  ) : (
                    <button
                      type="button"
                      className="link-btn"
                      onClick={() => setTheme(pack.id)}
                    >
                      {t("settings.themeApply")}
                    </button>
                  )}
                </li>
              ))}
            </ul>
          )}
        </fieldset>

        <fieldset className="settings-group">
          <legend>{t("settings.languageTitle")}</legend>
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
          <div className="plugin-actions">
            <button type="button" onClick={() => void refreshPlugins()}>
              {t("settings.languageReload")}
            </button>
            <button type="button" onClick={() => void openPluginDir()}>
              {t("plugins.openFolder")}
            </button>
          </div>
          {languagePacks.length === 0 ? (
            <p className="hint">{t("settings.languagePackEmpty")}</p>
          ) : (
            <ul className="plugin-list">
              {languagePacks.map((pack) => (
                <li key={pack.id} className="plugin-row">
                  <span className="plugin-name">{pack.name}</span>
                  <span className="plugin-kind">{pack.locale}</span>
                  {language === pack.locale ? (
                    <span className="hint">{t("settings.languageActive")}</span>
                  ) : (
                    <button
                      type="button"
                      className="link-btn"
                      onClick={() => setLanguage(pack.locale)}
                    >
                      {t("settings.languageActivate")}
                    </button>
                  )}
                </li>
              ))}
            </ul>
          )}
        </fieldset>

        <fieldset className="settings-group">
          <legend>{t("settings.tabsTitle")}</legend>
          <label className="field">
            <span>{t("settings.tabNumbering")}</span>
            <select
              value={tabNumbering}
              onChange={(e) => setTabNumbering(e.target.value as TabNumbering)}
            >
              <option value="asNeed">{t("settings.tabNumberingAsNeed")}</option>
              <option value="always">{t("settings.tabNumberingAlways")}</option>
              <option value="none">{t("settings.tabNumberingNone")}</option>
            </select>
            <span className="hint">{t("settings.tabNumberingHint")}</span>
          </label>
          <label className="checkbox">
            <input
              type="checkbox"
              checked={confirmClose}
              onChange={(e) => setConfirmClose(e.target.checked)}
            />
            <span>{t("settings.confirmClose")}</span>
          </label>
          <span className="hint">{t("settings.confirmCloseHint")}</span>
        </fieldset>

        <fieldset className="settings-group">
          <legend>{t("settings.credentials")}</legend>
          <p className="hint">{t("settings.credentialsHint")}</p>
        </fieldset>
            </>
          )}
          {category === "terminal" && (
            <>
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
            </>
          )}
          {category === "keyboard" && (
            <>
        <div className="plugin-actions">
          <button type="button" onClick={resetShortcuts}>
            {t("shortcuts.resetAll")}
          </button>
        </div>
        <ShortcutEditor />
            </>
          )}
          {category === "transfer" && (
            <>
        <fieldset className="settings-group">
          <legend>{t("settings.cat.transfer")}</legend>
          <label className="checkbox">
            <input
              type="checkbox"
              checked={sftpShowHidden}
              onChange={(e) => setSftpShowHidden(e.target.checked)}
            />
            <span>{t("settings.sftpShowHidden")}</span>
          </label>
          <span className="hint">{t("settings.sftpShowHiddenHint")}</span>
        </fieldset>
            </>
          )}
          {category === "sync" && (
            <>
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
          <legend>{t("sync.remoteTitle")}</legend>
          <p className="hint">{t("sync.remoteHint")}</p>
          <label className="field">
            <span>{t("sync.provider")}</span>
            <select
              value={remoteKind}
              onChange={(e) => setRemoteKind(e.target.value as "gist" | "webdav")}
            >
              <option value="gist">{t("sync.providerGist")}</option>
              <option value="webdav">{t("sync.providerWebdav")}</option>
            </select>
          </label>
          {remoteKind === "gist" ? (
            <>
              <label className="field">
                <span>{t("sync.gistToken")}</span>
                <input
                  type="password"
                  value={gistToken}
                  onChange={(e) => setGistToken(e.target.value)}
                />
              </label>
              <label className="field">
                <span>{t("sync.gistId")}</span>
                <input
                  value={gistId}
                  onChange={(e) => setGistId(e.target.value)}
                  placeholder={t("sync.gistIdPlaceholder")}
                />
                <span className="hint">{t("sync.gistIdHint")}</span>
              </label>
              <label className="field">
                <span>{t("sync.fileName")}</span>
                <input
                  value={gistFile}
                  onChange={(e) => setGistFile(e.target.value)}
                />
              </label>
            </>
          ) : (
            <>
              <label className="field">
                <span>{t("sync.webdavUrl")}</span>
                <input
                  value={davUrl}
                  onChange={(e) => setDavUrl(e.target.value)}
                  placeholder="https://dav.example.com/backups"
                />
              </label>
              <label className="field">
                <span>{t("sync.webdavUser")}</span>
                <input
                  value={davUser}
                  onChange={(e) => setDavUser(e.target.value)}
                />
              </label>
              <label className="field">
                <span>{t("sync.webdavPassword")}</span>
                <input
                  type="password"
                  value={davPass}
                  onChange={(e) => setDavPass(e.target.value)}
                />
              </label>
              <label className="field">
                <span>{t("sync.fileName")}</span>
                <input
                  value={davFile}
                  onChange={(e) => setDavFile(e.target.value)}
                />
              </label>
            </>
          )}
          <div className="sync-actions">
            <button
              type="button"
              onClick={() => void runRemote("test")}
              disabled={remoteBusy}
            >
              {t("sync.test")}
            </button>
            <button
              type="button"
              onClick={() => void runRemote("upload")}
              disabled={remoteBusy || !passphrase}
            >
              {t("sync.upload")}
            </button>
            <button
              type="button"
              onClick={() => void runRemote("download")}
              disabled={remoteBusy || !passphrase}
            >
              {t("sync.download")}
            </button>
          </div>
          {remoteMessage && (
            <p className="hint sync-message">{remoteMessage}</p>
          )}
        </fieldset>
            </>
          )}
          {category === "plugins" && (
            <>
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
                  {plugin.kind === "wasm" && (
                    <button
                      type="button"
                      className="link-btn"
                      onClick={() => void runPlugin(plugin.id)}
                    >
                      {t("plugins.run")}
                    </button>
                  )}
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
            </>
          )}
          {category === "about" && (
            <>
        <fieldset className="settings-group">
          <legend>{t("settings.cat.about")}</legend>
          <p className="about-name">{t("app.name")}</p>
          <p className="hint">{t("about.description")}</p>
          <p className="hint">{t("about.cleanRoom")}</p>
          <p className="hint">
            <a
              href="https://github.com/chensnails/ferriterm"
              target="_blank"
              rel="noreferrer"
            >
              {t("about.repository")}
            </a>
          </p>
        </fieldset>
            </>
          )}
        </div>
      </div>
    </Modal>
  );
}
