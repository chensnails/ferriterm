import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, isTauri, type RemoteConfig } from "../api";
import { useAppStore, type TabNumbering } from "../store";
import { BUILTIN_THEMES, HIGHLIGHT_COLORS } from "../theme";
import { parseSchemeFile } from "../scheme";
import { DEV_VERSION } from "../version";
import { Modal } from "./Modal";
import { ShortcutEditor } from "./ShortcutEditor";
import { VersionHistory } from "./VersionHistory";
import { Chip } from "./ui";
import {
  CloudIcon,
  EyeIcon,
  FilesIcon,
  GearIcon,
  InfoIcon,
  KeyboardIcon,
  PuzzleIcon,
  SlidersIcon,
  TerminalIcon,
} from "./icons";

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

const CATEGORY_ICONS: Record<SettingsCategory, typeof SlidersIcon> = {
  general: SlidersIcon,
  terminal: TerminalIcon,
  keyboard: KeyboardIcon,
  transfer: FilesIcon,
  plugins: PuzzleIcon,
  sync: CloudIcon,
  about: InfoIcon,
};

function PasswordInput({
  value,
  onChange,
  placeholder,
  shown,
  onToggle,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  shown: boolean;
  onToggle: () => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="password-field">
      <input
        type={shown ? "text" : "password"}
        value={value}
        placeholder={placeholder}
        onChange={(e) => onChange(e.target.value)}
      />
      <button
        type="button"
        className="icon-btn password-toggle"
        title={shown ? t("sync.hidePassword") : t("sync.showPassword")}
        aria-label={shown ? t("sync.hidePassword") : t("sync.showPassword")}
        aria-pressed={shown}
        onClick={onToggle}
      >
        <EyeIcon size={14} />
      </button>
    </div>
  );
}

export function SettingsDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const [category, setCategory] = useState<SettingsCategory>("general");
  const theme = useAppStore((s) => s.theme);
  const language = useAppStore((s) => s.language);
  const languagePacks = useAppStore((s) => s.languagePacks);
  const pluginScan = useAppStore((s) => s.pluginScan);
  const themes = pluginScan?.themes ?? [];
  const importedThemes = useAppStore((s) => s.importedThemes);
  const importTheme = useAppStore((s) => s.importTheme);
  const removeImportedTheme = useAppStore((s) => s.removeImportedTheme);
  const loadLanguagePacks = useAppStore((s) => s.loadLanguagePacks);
  const bumpCommandLibrary = useAppStore((s) => s.bumpCommandLibrary);
  const keepaliveSecs = useAppStore((s) => s.keepaliveSecs);
  const terminalFontFamily = useAppStore((s) => s.terminalFontFamily);
  const setTerminalFontFamily = useAppStore((s) => s.setTerminalFontFamily);
  const terminalFontSize = useAppStore((s) => s.terminalFontSize);
  const setTerminalFontSize = useAppStore((s) => s.setTerminalFontSize);
  const terminalScrollback = useAppStore((s) => s.terminalScrollback);
  const setTerminalScrollback = useAppStore((s) => s.setTerminalScrollback);
  const terminalCursorBlink = useAppStore((s) => s.terminalCursorBlink);
  const setTerminalCursorBlink = useAppStore((s) => s.setTerminalCursorBlink);
  const osc52 = useAppStore((s) => s.osc52);
  const setOsc52 = useAppStore((s) => s.setOsc52);
  const keywordHighlights = useAppStore((s) => s.keywordHighlights);
  const setKeywordHighlights = useAppStore((s) => s.setKeywordHighlights);
  const setTheme = useAppStore((s) => s.setTheme);
  const setLanguage = useAppStore((s) => s.setLanguage);
  const setKeepalive = useAppStore((s) => s.setKeepalive);
  const tabNumbering = useAppStore((s) => s.tabNumbering);
  const setTabNumbering = useAppStore((s) => s.setTabNumbering);
  const confirmClose = useAppStore((s) => s.confirmClose);
  const setConfirmClose = useAppStore((s) => s.setConfirmClose);
  const sftpShowHidden = useAppStore((s) => s.sftpShowHidden);
  const setSftpShowHidden = useAppStore((s) => s.setSftpShowHidden);
  const transferConcurrency = useAppStore((s) => s.transferConcurrency);
  const setTransferConcurrency = useAppStore(
    (s) => s.setTransferConcurrency,
  );
  const loadHosts = useAppStore((s) => s.loadHosts);
  const loadTags = useAppStore((s) => s.loadTags);
  const [passphrase, setPassphrase] = useState("");
  const [pluginMessage, setPluginMessage] = useState("");
  const [themeMessage, setThemeMessage] = useState("");
  const [keywordDraft, setKeywordDraft] = useState("");
  const [colorDraft, setColorDraft] = useState(HIGHLIGHT_COLORS[0]);
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
  const [showPassword, setShowPassword] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);
  const themeFileRef = useRef<HTMLInputElement>(null);

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

  const addHighlight = () => {
    const text = keywordDraft.trim();
    if (!text) return;
    setKeywordHighlights([...keywordHighlights, { text, color: colorDraft }]);
    setKeywordDraft("");
  };

  const importSchemeFile = async (file: File) => {
    try {
      const scheme = parseSchemeFile(file.name, await file.text());
      importTheme(scheme);
      setThemeMessage(t("settings.themeImported", { name: scheme.name }));
    } catch (err) {
      const detail = err instanceof Error ? err.message : String(err);
      setThemeMessage(t("settings.themeImportFailed", { error: detail }));
    } finally {
      if (themeFileRef.current) themeFileRef.current.value = "";
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
    >
      <div className="settings-center">
        <nav className="settings-nav" aria-label={t("settings.title")}>
          {SETTINGS_CATEGORIES.map((item) => {
            const Icon = CATEGORY_ICONS[item];
            return (
              <button
                key={item}
                type="button"
                className={category === item ? "active" : ""}
                aria-current={category === item ? "page" : undefined}
                onClick={() => setCategory(item)}
              >
                <Icon size={14} />
                <span>{t(`settings.cat.${item}`)}</span>
              </button>
            );
          })}
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
              {importedThemes.length > 0 && (
                <optgroup label={t("settings.themeCustom")}>
                  {importedThemes.map((item) => (
                    <option key={item.id} value={item.id}>
                      {item.name}
                    </option>
                  ))}
                </optgroup>
              )}
            </select>
          </label>
          <div className="plugin-actions">
            <button
              type="button"
              onClick={() => themeFileRef.current?.click()}
            >
              {t("settings.themeImport")}
            </button>
            <input
              ref={themeFileRef}
              type="file"
              accept=".itermcolors,.toml,.tml"
              hidden
              onChange={(e) => {
                const file = e.target.files?.[0];
                if (file) void importSchemeFile(file);
              }}
            />
          </div>
          <span className="hint">{t("settings.themeImportHint")}</span>
          {themeMessage && (
            <p className="hint sync-message">{themeMessage}</p>
          )}
          {importedThemes.length > 0 && (
            <ul className="plugin-list">
              {importedThemes.map((item) => (
                <li key={item.id} className="plugin-row">
                  <span className="plugin-name">{item.name}</span>
                  <span className="plugin-kind">
                    {t(`settings.theme_${item.base}`)}
                  </span>
                  {theme === item.id ? (
                    <span className="hint">{t("settings.themeActive")}</span>
                  ) : (
                    <button
                      type="button"
                      className="link-btn"
                      onClick={() => setTheme(item.id)}
                    >
                      {t("settings.themeApply")}
                    </button>
                  )}
                  <button
                    type="button"
                    className="link-btn"
                    onClick={() => removeImportedTheme(item.id)}
                  >
                    {t("settings.themeRemove")}
                  </button>
                </li>
              ))}
            </ul>
          )}
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
          <label className="field">
            <span>{t("settings.confirmClose")}</span>
            <select
              value={confirmClose ? "1" : "0"}
              onChange={(e) => setConfirmClose(e.target.value === "1")}
            >
              <option value="1">{t("common.yes")}</option>
              <option value="0">{t("common.no")}</option>
            </select>
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
        <fieldset className="settings-group">
          <legend>{t("settings.terminalAppearance")}</legend>
          <p className="hint">{t("settings.appearanceHint")}</p>
          <div className="settings-inline">
            <label className="field">
              <span>{t("settings.fontFamily")}</span>
              <input
                value={terminalFontFamily}
                spellCheck={false}
                onChange={(e) => setTerminalFontFamily(e.target.value)}
              />
            </label>
            <label className="field">
              <span>{t("settings.fontSize")}</span>
              <input
                type="number"
                min={8}
                max={32}
                value={terminalFontSize}
                onChange={(e) =>
                  setTerminalFontSize(
                    Number.parseInt(e.target.value, 10) || terminalFontSize,
                  )
                }
              />
            </label>
          </div>
          <span className="hint">{t("settings.fontFamilyHint")}</span>
          <label className="field">
            <span>{t("settings.scrollback")}</span>
            <input
              type="number"
              min={200}
              step={100}
              value={terminalScrollback}
              onChange={(e) =>
                setTerminalScrollback(
                  Number.parseInt(e.target.value, 10) || terminalScrollback,
                )
              }
            />
            <span className="hint">{t("settings.scrollbackHint")}</span>
          </label>
          <label className="field">
            <span>{t("settings.cursorBlink")}</span>
            <select
              value={terminalCursorBlink ? "1" : "0"}
              onChange={(e) => setTerminalCursorBlink(e.target.value === "1")}
            >
              <option value="1">{t("common.yes")}</option>
              <option value="0">{t("common.no")}</option>
            </select>
          </label>
        </fieldset>

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
          <legend>{t("settings.osc52Title")}</legend>
          <label className="field">
            <span>{t("settings.osc52")}</span>
            <select
              value={osc52 ? "1" : "0"}
              onChange={(e) => setOsc52(e.target.value === "1")}
            >
              <option value="1">{t("common.yes")}</option>
              <option value="0">{t("common.no")}</option>
            </select>
          </label>
          <span className="hint">{t("settings.osc52Hint")}</span>
        </fieldset>

        <fieldset className="settings-group">
          <legend>{t("settings.highlightsTitle")}</legend>
          <p className="hint">{t("settings.highlightsHint")}</p>
          <div className="highlight-form">
            <label className="field grow">
              <input
                value={keywordDraft}
                maxLength={64}
                placeholder={t("settings.highlightPlaceholder")}
                onChange={(e) => setKeywordDraft(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    addHighlight();
                  }
                }}
              />
            </label>
            <label className="field">
              <select
                value={colorDraft}
                onChange={(e) => setColorDraft(e.target.value)}
              >
                {HIGHLIGHT_COLORS.map((color) => (
                  <option key={color} value={color}>
                    {t(`settings.highlightColor_${color.replace("--tag-", "")}`)}
                  </option>
                ))}
              </select>
            </label>
            <button
              type="button"
              onClick={addHighlight}
              disabled={!keywordDraft.trim()}
            >
              {t("settings.highlightAdd")}
            </button>
          </div>
          {keywordHighlights.length === 0 ? (
            <p className="hint">{t("settings.highlightEmpty")}</p>
          ) : (
            <ul className="plugin-list">
              {keywordHighlights.map((rule) => (
                <li key={rule.text} className="plugin-row">
                  <span className="plugin-name highlight-name">
                    <span
                      className="highlight-dot"
                      style={{ background: `var(${rule.color})` }}
                    />
                    {rule.text}
                  </span>
                  <button
                    type="button"
                    className="link-btn"
                    onClick={() =>
                      setKeywordHighlights(
                        keywordHighlights.filter(
                          (item) => item.text !== rule.text,
                        ),
                      )
                    }
                  >
                    {t("settings.highlightRemove")}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </fieldset>
            </>
          )}
          {category === "keyboard" && <ShortcutEditor />}
          {category === "transfer" && (
            <>
        <fieldset className="settings-group">
          <legend>{t("settings.cat.transfer")}</legend>
          <label className="field">
            <span>{t("settings.sftpShowHidden")}</span>
            <select
              value={sftpShowHidden ? "1" : "0"}
              onChange={(e) => setSftpShowHidden(e.target.value === "1")}
            >
              <option value="1">{t("common.yes")}</option>
              <option value="0">{t("common.no")}</option>
            </select>
          </label>
          <span className="hint">{t("settings.sftpShowHiddenHint")}</span>
          <label className="field">
            <span>{t("settings.transferConcurrency")}</span>
            <select
              value={String(transferConcurrency)}
              onChange={(e) =>
                setTransferConcurrency(Number(e.target.value))
              }
            >
              {Array.from({ length: 8 }, (_, index) => index + 1).map(
                (value) => (
                  <option key={value} value={String(value)}>
                    {value}
                  </option>
                ),
              )}
            </select>
          </label>
          <span className="hint">{t("settings.transferConcurrencyHint")}</span>
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
            <PasswordInput
              value={passphrase}
              onChange={setPassphrase}
              placeholder={t("sync.passphrasePlaceholder")}
              shown={showPassword}
              onToggle={() => setShowPassword(!showPassword)}
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
            <div className="field-inline">
              <select
                value={remoteKind}
                onChange={(e) => setRemoteKind(e.target.value as "gist" | "webdav")}
              >
                <option value="gist">{t("sync.providerGist")}</option>
                <option value="webdav">{t("sync.providerWebdav")}</option>
              </select>
              {remoteKind === "webdav" && (
                <input
                  value={davUrl}
                  onChange={(e) => setDavUrl(e.target.value)}
                  placeholder="https://dav.example.com/backups"
                />
              )}
            </div>
          </label>
          {remoteKind === "gist" ? (
            <>
              <label className="field">
                <span>{t("sync.gistToken")}</span>
                <PasswordInput
                  value={gistToken}
                  onChange={setGistToken}
                  shown={showPassword}
                  onToggle={() => setShowPassword(!showPassword)}
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
                <span>{t("sync.webdavUser")}</span>
                <input
                  value={davUser}
                  onChange={(e) => setDavUser(e.target.value)}
                />
              </label>
              <label className="field">
                <span>{t("sync.webdavPassword")}</span>
                <PasswordInput
                  value={davPass}
                  onChange={setDavPass}
                  shown={showPassword}
                  onToggle={() => setShowPassword(!showPassword)}
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
        <div className="settings-tabs">
          <button type="button" className="settings-tab" disabled>
            {t("plugins.market")}
          </button>
          <button
            type="button"
            className="settings-tab active"
            aria-current="page"
          >
            {t("plugins.installed")}
          </button>
          <button
            type="button"
            className="icon-btn settings-tabs-gear"
            title={t("plugins.openFolder")}
            aria-label={t("plugins.openFolder")}
            onClick={() => void openPluginDir()}
          >
            <GearIcon size={14} />
          </button>
        </div>
        <fieldset className="settings-group">
          <p className="hint">{t("plugins.hint")}</p>
          <div className="plugin-actions">
            <button type="button" onClick={() => void refreshPlugins()}>
              {t("plugins.refresh")}
            </button>
          </div>
          {pluginMessage && (
            <p className="hint sync-message">{pluginMessage}</p>
          )}
          {pluginScan && pluginScan.plugins.length > 0 && (
            <ul className="plugin-list">
              {pluginScan.plugins.map((plugin) => (
                <li key={plugin.id} className="plugin-row">
                  <span className="plugin-icon">
                    <PuzzleIcon size={16} />
                  </span>
                  <span className="plugin-main">
                    <span className="plugin-name">
                      {plugin.name}
                      {plugin.version && (
                        <span className="plugin-version"> {plugin.version}</span>
                      )}
                    </span>
                    <span className="hint plugin-desc">
                      {t(`plugins.kinds.${plugin.kind}`, {
                        defaultValue: plugin.kind,
                      })}
                    </span>
                  </span>
                  {plugin.kind === "wasm" && (
                    <button
                      type="button"
                      className="plugin-action"
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
                  <span className="plugin-icon">
                    <PuzzleIcon size={16} />
                  </span>
                  <span className="plugin-main">
                    <span className="plugin-name">{pack.name}</span>
                    <span className="hint plugin-desc">
                      {t("plugins.kinds.command-pack")}
                    </span>
                  </span>
                  <button
                    type="button"
                    className="plugin-action"
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
          <p className="about-version">
            <span className="hint">{t("about.devVersion")}</span>
            <Chip tone="info">{DEV_VERSION}</Chip>
          </p>
          <VersionHistory />
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
