import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, isTauri, SYNC_SCOPES, type BackupSummary, type BuiltinPlugin, type MarketEntry, type MarketIndex, type RemoteConfig, type SyncScope, type VersionEntry, type VersionSource } from "../api";
import { remoteConfig, runAutoSync, SYNC_PASSPHRASE_REF } from "../autoSync";
import { formatBytes } from "../format";
import {
  useAppStore,
  DEFAULT_TERMINAL_FONT_FAMILY,
  DEFAULT_TERMINAL_FONT_SIZE,
  DEFAULT_TERMINAL_SCROLLBACK,
  type SyncInterval,
  type SyncPolicy,
  type TabNumbering,
  type TerminalBell,
  type TerminalCursorStyle,
} from "../store";
import type { SshPreset } from "../api";
import { BUILTIN_THEMES, HIGHLIGHT_COLORS } from "../theme";
import { parseSchemeFile } from "../scheme";
import { CUSTOM_FONT_ID, TERMINAL_FONTS, fontChoiceFor } from "../fonts";
import {
  SSH_ALGORITHM_GROUPS,
  SSH_RECOMMENDED,
  type SshAlgorithmGroupKey,
} from "../ssh-algorithms";
import { DEV_VERSION } from "../version";
import { detectLanguage } from "../i18n";
import { Modal } from "./Modal";
import { CredentialVault } from "./CredentialVault";
import { SettingsAi } from "./SettingsAi";
import { SettingsPolicy } from "./SettingsPolicy";
import { ShortcutEditor } from "./ShortcutEditor";
import { VersionHistory } from "./VersionHistory";
import { UpdatePanel } from "./UpdatePanel";
import { ThemeGallery, ThemeImportButton } from "./ThemeGallery";
import {
  CheckList,
  Chip,
  SettingGroup,
  SettingRow,
  SettingsSearchContext,
} from "./ui";
import {
  ActionSetting,
  PillSetting,
  SegmentedSetting,
  SelectSetting,
  ToggleSetting,
  type SelectOption,
} from "./SettingsControls";
import {
  CheckIcon,
  CloudIcon,
  DownloadIcon,
  EyeIcon,
  FilesIcon,
  GearIcon,
  InfoIcon,
  KeyboardIcon,
  KeyIcon,
  LogoMark,
  PuzzleIcon,
  RefreshIcon,
  SearchIcon,
  SlidersIcon,
  SparkleIcon,
  ShieldIcon,
  TerminalIcon,
  ThemeIcon,
  UploadIcon,
} from "./icons";

const SETTINGS_CATEGORIES = [
  "general",
  "appearance",
  "credentials",
  "terminal",
  "ai",
  "security",
  "keyboard",
  "transfer",
  "plugins",
  "sync",
  "about",
] as const;

type SettingsCategory = (typeof SETTINGS_CATEGORIES)[number];

const CATEGORY_ICONS: Record<SettingsCategory, typeof SlidersIcon> = {
  general: SlidersIcon,
  appearance: ThemeIcon,
  credentials: KeyIcon,
  terminal: TerminalIcon,
  ai: SparkleIcon,
  security: ShieldIcon,
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
  const [query, setQuery] = useState("");
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
  const storedFontChoice = fontChoiceFor(terminalFontFamily);
  const [customFont, setCustomFont] = useState(
    storedFontChoice === CUSTOM_FONT_ID,
  );
  const terminalFontSize = useAppStore((s) => s.terminalFontSize);
  const setTerminalFontSize = useAppStore((s) => s.setTerminalFontSize);
  const terminalScrollback = useAppStore((s) => s.terminalScrollback);
  const setTerminalScrollback = useAppStore((s) => s.setTerminalScrollback);
  const terminalCursorBlink = useAppStore((s) => s.terminalCursorBlink);
  const setTerminalCursorBlink = useAppStore((s) => s.setTerminalCursorBlink);
  const terminalCursorStyle = useAppStore((s) => s.terminalCursorStyle);
  const setTerminalCursorStyle = useAppStore((s) => s.setTerminalCursorStyle);
  const terminalBell = useAppStore((s) => s.terminalBell);
  const setTerminalBell = useAppStore((s) => s.setTerminalBell);
  const terminalRightClickPaste = useAppStore((s) => s.terminalRightClickPaste);
  const setTerminalRightClickPaste = useAppStore(
    (s) => s.setTerminalRightClickPaste,
  );
  const terminalScrollOnOutput = useAppStore((s) => s.terminalScrollOnOutput);
  const setTerminalScrollOnOutput = useAppStore(
    (s) => s.setTerminalScrollOnOutput,
  );
  const terminalAutoCopy = useAppStore((s) => s.terminalAutoCopy);
  const setTerminalAutoCopy = useAppStore((s) => s.setTerminalAutoCopy);
  const sshSecurity = useAppStore((s) => s.sshSecurity);
  const setSshPreset = useAppStore((s) => s.setSshPreset);
  const setSshAlgorithmGroup = useAppStore((s) => s.setSshAlgorithmGroup);
  const osc52 = useAppStore((s) => s.osc52);
  const setOsc52 = useAppStore((s) => s.setOsc52);
  const keywordHighlights = useAppStore((s) => s.keywordHighlights);
  const setKeywordHighlights = useAppStore((s) => s.setKeywordHighlights);
  const setTheme = useAppStore((s) => s.setTheme);
  const uiFontSize = useAppStore((s) => s.uiFontSize);
  const setUiFontSize = useAppStore((s) => s.setUiFontSize);
  const reduceMotion = useAppStore((s) => s.reduceMotion);
  const setReduceMotion = useAppStore((s) => s.setReduceMotion);
  const dockSingleCard = useAppStore((s) => s.dockSingleCard);
  const setDockSingleCard = useAppStore((s) => s.setDockSingleCard);
  const setLanguage = useAppStore((s) => s.setLanguage);
  const setKeepalive = useAppStore((s) => s.setKeepalive);
  const tabNumbering = useAppStore((s) => s.tabNumbering);
  const setTabNumbering = useAppStore((s) => s.setTabNumbering);
  const restoreLayoutOnStartup = useAppStore((s) => s.restoreLayoutOnStartup);
  const setRestoreLayoutOnStartup = useAppStore(
    (s) => s.setRestoreLayoutOnStartup,
  );
  const resetLayoutSizes = useAppStore((s) => s.resetLayoutSizes);
  const statusMonitor = useAppStore((s) => s.statusMonitor);
  const setStatusMonitor = useAppStore((s) => s.setStatusMonitor);
  const clearCommandHistory = useAppStore((s) => s.clearCommandHistory);
  const clearExecLog = useAppStore((s) => s.clearExecLog);
  const resetShortcuts = useAppStore((s) => s.resetShortcuts);
  const historyCount = useAppStore((s) => s.commandHistory.length);
  const execLogCount = useAppStore((s) => s.execLog.length);
  const confirmClose = useAppStore((s) => s.confirmClose);
  const setConfirmClose = useAppStore((s) => s.setConfirmClose);
  const sftpShowHidden = useAppStore((s) => s.sftpShowHidden);
  const setSftpShowHidden = useAppStore((s) => s.setSftpShowHidden);
  const editorAutoSave = useAppStore((s) => s.editorAutoSave);
  const setEditorAutoSave = useAppStore((s) => s.setEditorAutoSave);
  const transferConcurrency = useAppStore((s) => s.transferConcurrency);
  const setTransferConcurrency = useAppStore(
    (s) => s.setTransferConcurrency,
  );
  const loadHosts = useAppStore((s) => s.loadHosts);
  const loadTags = useAppStore((s) => s.loadTags);
  const [passphrase, setPassphrase] = useState("");
  const [includeSecrets, setIncludeSecrets] = useState(false);
  // v2 bundles carry every managed data type; the credentials switch still
  // decides whether secret material is allowed to leave this machine.
  const backupScope: SyncScope[] = SYNC_SCOPES.filter(
    (scope) => scope !== "secrets" || includeSecrets,
  );
  const syncPolicy = useAppStore((s) => s.syncPolicy);
  const setSyncPolicy = useAppStore((s) => s.setSyncPolicy);
  const syncInterval = useAppStore((s) => s.syncInterval);
  const setSyncInterval = useAppStore((s) => s.setSyncInterval);
  const syncScope = useAppStore((s) => s.syncScope);
  const setSyncScope = useAppStore((s) => s.setSyncScope);
  const syncLastAt = useAppStore((s) => s.syncLastAt);
  const syncStatus = useAppStore((s) => s.syncStatus);
  const setSyncStatus = useAppStore((s) => s.setSyncStatus);
  const [remoteConflict, setRemoteConflict] = useState(false);
  const [autoPassSaved, setAutoPassSaved] = useState(false);
  const [autoBusy, setAutoBusy] = useState(false);
  const [autoMessage, setAutoMessage] = useState("");
  const [versions, setVersions] = useState<VersionEntry[]>([]);
  const [versionsBusy, setVersionsBusy] = useState(false);
  const [versionsMessage, setVersionsMessage] = useState("");
  const [restoringId, setRestoringId] = useState<string | null>(null);
  const askConfirm = useAppStore((s) => s.askConfirm);
  const [pluginMessage, setPluginMessage] = useState("");
  const [pluginTab, setPluginTab] = useState<"installed" | "market">(
    "installed",
  );
  const [market, setMarket] = useState<MarketIndex | null>(null);
  const [marketBusy, setMarketBusy] = useState(false);
  const [marketBusyId, setMarketBusyId] = useState<string | null>(null);
  const [marketMessage, setMarketMessage] = useState("");
  const setSerialDialogOpen = useAppStore((s) => s.setSerialDialogOpen);
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
  const [gistTokenSaved, setGistTokenSaved] = useState(false);
  const [davPassSaved, setDavPassSaved] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);
  const paneRef = useRef<HTMLDivElement>(null);
  /** The top veil exists only while rows are actually scrolled under the
      pinned intro; at rest it would clip the first group title. */
  const [paneScrolled, setPaneScrolled] = useState(false);

  // Switching category or filtering rewrites the pane: start it at the top so
  // the first group title is the first thing on screen.
  useEffect(() => {
    const pane = paneRef.current;
    if (pane) pane.scrollTop = 0;
    setPaneScrolled(false);
  }, [category, query]);

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
        const [token, password, autoPass] = await Promise.all([
          api.secretSaved("sync:gist-token"),
          api.secretSaved("sync:webdav-password"),
          api.secretSaved(SYNC_PASSPHRASE_REF),
        ]);
        setGistTokenSaved(token);
        setDavPassSaved(password);
        setAutoPassSaved(autoPass);
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
    ]);
    if (gistToken) {
      await api.saveSecret("sync:gist-token", gistToken);
      setGistToken("");
      setGistTokenSaved(true);
    }
    if (davPass) {
      await api.saveSecret("sync:webdav-password", davPass);
      setDavPass("");
      setDavPassSaved(true);
    }
  };

  const forgetSecret = async (
    secretRef: string,
    kind: "gist" | "webdav",
  ) => {
    try {
      await api.deleteSecret(secretRef);
      if (kind === "gist") setGistTokenSaved(false);
      else setDavPassSaved(false);
      setRemoteMessage(t("sync.secretCleared"));
    } catch (err) {
      setRemoteMessage(String(err));
    }
  };

  const runRemote = async (
    action: "test" | "upload" | "download",
    force = false,
  ) => {
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
        const outcome = await api.syncRemoteUpload(
          remoteConfig(),
          passphrase,
          backupScope,
          force,
        );
        if (outcome.state === "conflict") {
          setRemoteConflict(true);
          setRemoteMessage(t("sync.conflict"));
          return;
        }
        setRemoteConflict(false);
        if (remoteKind === "gist" && outcome.id && outcome.id !== gistId) {
          setGistId(outcome.id);
          await api.setSetting("sync:gist-id", outcome.id);
        }
        setRemoteMessage(
          t("sync.uploaded", {
            location: outcome.location,
            bytes: outcome.bytes,
          }),
        );
      } else {
        const stats = await api.syncRemoteDownload(remoteConfig(), passphrase);
        await Promise.all([loadHosts(), loadTags()]);
        setRemoteConflict(false);
        setSyncStatus({ state: "idle", message: "", at: Date.now() });
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
      const path = await api.exportBackup(passphrase, backupScope);
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

  const saveAutoPassphrase = async () => {
    if (!passphrase) {
      setAutoMessage(t("sync.passphraseNeeded"));
      return;
    }
    setAutoBusy(true);
    try {
      await persistRemote();
      await api.saveSecret(SYNC_PASSPHRASE_REF, passphrase);
      setAutoPassSaved(true);
      setAutoMessage(t("sync.passphraseSaved"));
    } catch (err) {
      setAutoMessage(String(err));
    } finally {
      setAutoBusy(false);
    }
  };

  const forgetAutoPassphrase = async () => {
    setAutoBusy(true);
    try {
      await api.deleteSecret(SYNC_PASSPHRASE_REF);
      setAutoPassSaved(false);
      setAutoMessage(t("sync.passphraseCleared"));
    } catch (err) {
      setAutoMessage(String(err));
    } finally {
      setAutoBusy(false);
    }
  };

  const syncNow = async () => {
    setAutoBusy(true);
    setAutoMessage("");
    try {
      await persistRemote();
    } catch (err) {
      setAutoMessage(String(err));
      setAutoBusy(false);
      return;
    }
    const result = await runAutoSync(syncScope);
    setAutoMessage(
      result.state === "ok"
        ? t("sync.syncNowOk", { time: new Date().toLocaleTimeString() })
        : result.state === "conflict"
          ? t("sync.conflict")
          : t("sync.syncNowFailed"),
    );
    setAutoBusy(false);
  };

  /** An unconfigured remote still has a local history (manual exports, Gist
   * fallbacks), so use a harmless config that makes the backend read locally. */
  const localConfig = (): RemoteConfig => ({
    kind: "gist",
    token: "",
    gistId: "",
    filename: "ferriterm-backup.ftbak",
  });

  const loadVersions = async () => {
    if (!isTauri) return;
    setVersionsBusy(true);
    try {
      const config = (await remoteConfig()) ?? localConfig();
      setVersions(await api.syncVersionsList(config));
    } catch (err) {
      setVersionsMessage(String(err));
    } finally {
      setVersionsBusy(false);
    }
  };

  const sourceLabel = (source: VersionSource) => {
    switch (source) {
      case "webdav":
        return t("sync.sourceWebdav");
      case "gist":
        return t("sync.sourceGist");
      case "restorePoint":
        return t("sync.sourceRestorePoint");
      default:
        return t("sync.sourceLocal");
    }
  };

  const summaryText = (summary: BackupSummary) => {
    const parts: string[] = [];
    if (summary.hosts) parts.push(t("sync.countHosts", { count: summary.hosts }));
    if (summary.tags) parts.push(t("sync.countTags", { count: summary.tags }));
    if (summary.secrets)
      parts.push(t("sync.countSecrets", { count: summary.secrets }));
    if (summary.sshKeys)
      parts.push(t("sync.countKeys", { count: summary.sshKeys }));
    if (summary.macros)
      parts.push(t("sync.countMacros", { count: summary.macros }));
    if (summary.highlights)
      parts.push(t("sync.countHighlights", { count: summary.highlights }));
    if (summary.shortcuts)
      parts.push(t("sync.countShortcuts", { count: summary.shortcuts }));
    return parts.length ? parts.join(" · ") : t("sync.emptyVersion");
  };

  const restoreVersion = async (entry: VersionEntry) => {
    const when = new Date(entry.createdAt * 1000).toLocaleString();
    const confirmed = await askConfirm({
      title: t("sync.restoreTitle"),
      body: t("sync.restoreBody", { time: when }),
      confirmLabel: t("sync.restoreConfirm"),
      danger: true,
    });
    if (!confirmed) return;
    setRestoringId(entry.id);
    setVersionsMessage("");
    try {
      const config = (await remoteConfig()) ?? localConfig();
      const result = await api.syncVersionRestore(config, passphrase, entry.id);
      await Promise.all([loadHosts(), loadTags()]);
      setVersionsMessage(
        t("sync.restored", { time: new Date(result.restorePointAt * 1000).toLocaleString() }),
      );
      await loadVersions();
    } catch (err) {
      setVersionsMessage(t("sync.restoreFailed", { error: String(err) }));
    } finally {
      setRestoringId(null);
    }
  };

  useEffect(() => {
    if (!isTauri) return;
    void loadVersions();
  }, [remoteKind]);

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

  const loadMarket = async (force = false) => {
    if (!isTauri) {
      setMarketMessage(t("plugins.marketDesktopOnly"));
      return;
    }
    if (market && !force) return;
    setMarketBusy(true);
    try {
      const index = await api.pluginMarketFetch();
      setMarket(index);
      setMarketMessage(index.plugins.length ? "" : t("plugins.marketEmpty"));
    } catch (err) {
      setMarketMessage(String(err));
    } finally {
      setMarketBusy(false);
    }
  };

  const installMarket = async (entry: MarketEntry) => {
    setMarketBusyId(entry.id);
    try {
      await api.pluginMarketInstall(entry.id, entry.manifestUrl);
      await loadLanguagePacks();
      setMarketMessage(t("plugins.marketInstalled", { name: entry.name }));
    } catch (err) {
      setMarketMessage(String(err));
    } finally {
      setMarketBusyId(null);
    }
  };

  const uninstallPlugin = async (
    entry: { id: string; name: string },
    scope: "installed" | "market" = "installed",
  ) => {
    const report = scope === "market" ? setMarketMessage : setPluginMessage;
    setMarketBusyId(entry.id);
    try {
      const removed = await api.pluginMarketUninstall(entry.id);
      await loadLanguagePacks();
      report(
        removed
          ? t("plugins.marketUninstalled", { name: entry.name })
          : t("plugins.marketNotFound", { name: entry.name }),
      );
    } catch (err) {
      report(String(err));
    } finally {
      setMarketBusyId(null);
    }
  };

  const installBuiltin = async (entry: BuiltinPlugin) => {
    setMarketBusyId(entry.id);
    try {
      await api.pluginInstallBuiltin(entry.id);
      await loadLanguagePacks();
      setPluginMessage(t("plugins.builtinInstalled", { name: entry.name }));
    } catch (err) {
      setPluginMessage(String(err));
    } finally {
      setMarketBusyId(null);
    }
  };

  // Each group reset touches only the settings shown inside that group, so
  // "reset this group" never silently changes an unrelated preference.
  const resetAppearance = () => {
    setTheme("dark");
    setUiFontSize(13);
    setReduceMotion(false);
    setDockSingleCard(false);
  };
  const resetStartup = () => {
    setRestoreLayoutOnStartup(true);
    setTabNumbering("asNeed");
    setConfirmClose(true);
  };
  const resetTerminalAppearance = () => {
    setTerminalFontFamily(DEFAULT_TERMINAL_FONT_FAMILY);
    setCustomFont(false);
    setTerminalFontSize(DEFAULT_TERMINAL_FONT_SIZE);
    setTerminalScrollback(DEFAULT_TERMINAL_SCROLLBACK);
    setTerminalCursorBlink(true);
    setTerminalCursorStyle("block");
  };
  const resetTerminalInteraction = () => {
    setTerminalBell("off");
    setTerminalRightClickPaste(false);
    setTerminalScrollOnOutput(true);
    setTerminalAutoCopy(false);
  };
  const resetConnection = () => {
    setKeepalive(30);
    setOsc52(true);
  };
  const resetTransfer = () => {
    setSftpShowHidden(false);
    setTransferConcurrency(6);
    setEditorAutoSave(false);
  };

  const clearCache = async (kind: "history" | "log") => {
    const confirmed = await useAppStore.getState().askConfirm({
      title: t(`settings.cache.${kind}Title`),
      body: t(`settings.cache.${kind}Confirm`),
      confirmLabel: t("settings.cache.clear"),
      danger: true,
    });
    if (!confirmed) return;
    if (kind === "history") clearCommandHistory();
    else clearExecLog();
    setMessage(t("settings.cache.cleared"));
  };

  return (
    <Modal
      title={t("settings.title")}
      subtitle={t("settings.autoSaveHint")}
      icon={<GearIcon size={15} />}
      onClose={onClose}
      className="settings"
    >
      <div className="settings-center">
        <nav className="settings-nav" aria-label={t("settings.title")}>
          <div className="settings-nav-search">
            <SearchIcon size={13} />
            <input
              type="search"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder={t("settings.searchPlaceholder")}
              aria-label={t("settings.searchPlaceholder")}
              spellCheck={false}
            />
          </div>
          <div className="settings-nav-list">
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
          </div>
        </nav>
        <div
          className={["settings-pane", query.trim() ? "searching" : ""]
            .filter(Boolean)
            .join(" ")}
        >
          <p className="settings-pane-intro">{t(`settings.desc.${category}`)}</p>
          {/* The category line stays put while the rows scroll underneath, so a
              long pane never loses "where am I". */}
          <div
            className="settings-pane-scroll"
            ref={paneRef}
            onScroll={(event) =>
              setPaneScrolled(event.currentTarget.scrollTop > 2)
            }
          >
          <div
            className={[
              "settings-scroll-fade",
              paneScrolled ? "visible" : "",
            ]
              .filter(Boolean)
              .join(" ")}
            aria-hidden
          />
          <SettingsSearchContext.Provider value={query}>
          {category === "general" && (
            <>
        <SettingGroup
          title={t("settings.group.language")}
          hint={t("settings.group.languageHint")}
        >
          {/* Language can grow with language packs, so it uses the dropdown
              the other open-ended lists use instead of a wrapping pill row. */}
          <SelectSetting
            title={t("settings.language")}
            hint={t("settings.languagePackHint")}
            value={language}
            onChange={setLanguage}
            options={[
              {
                value: "en",
                label: "English",
                group: t("settings.languageBuiltin"),
              },
              {
                value: "zh-CN",
                label: "简体中文",
                group: t("settings.languageBuiltin"),
              },
              ...languagePacks
                .filter(
                  (pack) =>
                    pack.locale !== "en" && pack.locale !== "zh-CN",
                )
                .map(
                  (pack): SelectOption => ({
                    value: pack.locale,
                    label: pack.name,
                    group: t("settings.languageTitle"),
                  }),
                ),
            ]}
          />
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
        </SettingGroup>

        <SettingGroup
          title={t("settings.group.startup")}
          hint={t("settings.group.startupHint")}
          onReset={resetStartup}
          resetLabel={t("settings.resetGroup")}
        >
          <ToggleSetting
            title={t("settings.restoreLayout")}
            hint={t("settings.restoreLayoutHint")}
            checked={restoreLayoutOnStartup}
            onChange={setRestoreLayoutOnStartup}
          />
          <SegmentedSetting
            title={t("settings.tabNumbering")}
            hint={t("settings.tabNumberingHint")}
            value={tabNumbering}
            onChange={(value) => setTabNumbering(value as TabNumbering)}
            options={[
              { value: "asNeed", label: t("settings.tabNumberingAsNeed") },
              { value: "always", label: t("settings.tabNumberingAlways") },
              { value: "none", label: t("settings.tabNumberingNone") },
            ]}
          />
          <ToggleSetting
            title={t("settings.confirmClose")}
            hint={t("settings.confirmCloseHint")}
            checked={confirmClose}
            onChange={setConfirmClose}
          />
        </SettingGroup>

        <SettingGroup
          title={t("settings.group.statusBar")}
          hint={t("settings.group.statusBarHint")}
        >
          <ToggleSetting
            title={t("settings.statusBarMonitor")}
            hint={t("settings.statusBarMonitorHint")}
            checked={statusMonitor}
            onChange={setStatusMonitor}
          />
        </SettingGroup>

        <SettingGroup
          title={t("settings.group.data")}
          hint={t("settings.group.dataHint")}
        >
          <ActionSetting
            title={t("settings.cache.historyTitle")}
            hint={t("settings.cache.historyHint")}
            current={t("settings.cache.entries", { count: historyCount })}
            actionLabel={t("settings.cache.clear")}
            danger
            onAction={() => void clearCache("history")}
          />
          <ActionSetting
            title={t("settings.cache.logTitle")}
            hint={t("settings.cache.logHint")}
            current={t("settings.cache.entries", { count: execLogCount })}
            actionLabel={t("settings.cache.clear")}
            danger
            onAction={() => void clearCache("log")}
          />
          <ActionSetting
            title={t("settings.cache.layoutTitle")}
            hint={t("settings.cache.layoutHint")}
            actionLabel={t("settings.cache.resetLayout")}
            onAction={resetLayoutSizes}
          />
          <ActionSetting
            title={t("settings.cache.shortcutsTitle")}
            hint={t("settings.cache.shortcutsHint")}
            actionLabel={t("settings.cache.resetShortcuts")}
            onAction={resetShortcuts}
          />
        </SettingGroup>
            </>
          )}
          {category === "appearance" && (
            <>
        <SettingGroup
          title={t("settings.group.theme")}
          hint={t("settings.group.themeHint")}
          onReset={resetAppearance}
          resetLabel={t("settings.resetGroup")}
        >
          <ThemeGallery
            theme={theme}
            packs={themes}
            imported={importedThemes}
            onSelect={setTheme}
          />
          <div className="plugin-actions">
            <ThemeImportButton
              label={t("settings.themeImport")}
              onFile={(file) => void importSchemeFile(file)}
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
        </SettingGroup>

        <SettingGroup
          title={t("settings.group.interface")}
          hint={t("settings.group.interfaceHint")}
        >
          <SettingRow
            title={t("settings.uiFontSize")}
            hint={t("settings.uiFontSizeHint")}
          >
            <input
              type="number"
              min={11}
              max={16}
              value={uiFontSize}
              onChange={(e) =>
                setUiFontSize(
                  Number.parseInt(e.target.value, 10) || uiFontSize,
                )
              }
            />
          </SettingRow>
          <ToggleSetting
            title={t("settings.reduceMotion")}
            hint={t("settings.reduceMotionHint")}
            checked={reduceMotion}
            onChange={setReduceMotion}
          />
          <ToggleSetting
            title={t("settings.dockSingleCard")}
            hint={t("settings.dockSingleCardHint")}
            checked={dockSingleCard}
            onChange={setDockSingleCard}
          />
        </SettingGroup>
            </>
          )}
          {category === "credentials" && <CredentialVault />}
          {category === "ai" && <SettingsAi />}
          {category === "security" && <SettingsPolicy />}
          {category === "terminal" && (
            <>
        <SettingGroup
          title={t("settings.terminalAppearance")}
          hint={t("settings.appearanceHint")}
          onReset={resetTerminalAppearance}
          resetLabel={t("settings.resetGroup")}
        >
          <SelectSetting
            title={t("settings.fontFamily")}
            hint={t("settings.fontFamilyHint")}
            wide
            value={customFont ? CUSTOM_FONT_ID : storedFontChoice}
            onChange={(id) => {
              if (id === CUSTOM_FONT_ID) {
                setCustomFont(true);
                return;
              }
              const font = TERMINAL_FONTS.find((item) => item.id === id);
              if (!font) return;
              setCustomFont(false);
              setTerminalFontFamily(font.stack);
            }}
            options={[
              ...TERMINAL_FONTS.map(
                (font): SelectOption => ({ value: font.id, label: font.label }),
              ),
              { value: CUSTOM_FONT_ID, label: t("settings.fontCustom") },
            ]}
          />
          {customFont && (
            <label className="field">
              <span>{t("settings.fontStack")}</span>
              <input
                value={terminalFontFamily}
                spellCheck={false}
                placeholder={TERMINAL_FONTS[0].stack}
                onChange={(e) => setTerminalFontFamily(e.target.value)}
              />
            </label>
          )}
          <SettingRow
            title={t("settings.fontSize")}
            hint={t("settings.fontSizeHint")}
          >
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
          </SettingRow>
          <SettingRow
            title={t("settings.scrollback")}
            hint={t("settings.scrollbackHint")}
          >
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
          </SettingRow>
          <ToggleSetting
            title={t("settings.cursorBlink")}
            hint={t("settings.cursorBlinkHint")}
            checked={terminalCursorBlink}
            onChange={setTerminalCursorBlink}
          />
          <SegmentedSetting
            title={t("settings.cursorStyle")}
            hint={t("settings.cursorStyleHint")}
            value={terminalCursorStyle}
            onChange={(value) =>
              setTerminalCursorStyle(value as TerminalCursorStyle)
            }
            options={[
              { value: "block", label: t("settings.cursorStyleBlock") },
              {
                value: "underline",
                label: t("settings.cursorStyleUnderline"),
              },
              { value: "bar", label: t("settings.cursorStyleBar") },
            ]}
          />
          <div
            className="font-preview"
            style={{
              fontFamily: terminalFontFamily,
              fontSize: `${terminalFontSize}px`,
            }}
            aria-label={t("settings.fontPreview")}
          >
            <span className="font-preview-prompt">
              user@ferriterm:~$ ls -la projects
            </span>
            <span>drwxr-xr-x  5 user staff  160 Nov  9 20:00 .</span>
            <span>-rw-r--r--  1 user staff 1024 Nov  9 20:00 README.md</span>
          </div>
        </SettingGroup>

        <SettingGroup
          title={t("settings.terminalInteraction")}
          hint={t("settings.terminalInteractionHint")}
          onReset={resetTerminalInteraction}
          resetLabel={t("settings.resetGroup")}
        >
          <SegmentedSetting
            title={t("settings.bell")}
            hint={t("settings.bellHint")}
            value={terminalBell}
            onChange={(value) => setTerminalBell(value as TerminalBell)}
            options={[
              { value: "off", label: t("settings.bellOff") },
              { value: "visual", label: t("settings.bellVisual") },
              { value: "sound", label: t("settings.bellSound") },
            ]}
          />
          <ToggleSetting
            title={t("settings.rightClickPaste")}
            hint={t("settings.rightClickPasteHint")}
            checked={terminalRightClickPaste}
            onChange={setTerminalRightClickPaste}
          />
          <ToggleSetting
            title={t("settings.scrollOnOutput")}
            hint={t("settings.scrollOnOutputHint")}
            checked={terminalScrollOnOutput}
            onChange={setTerminalScrollOnOutput}
          />
          <ToggleSetting
            title={t("settings.autoCopy")}
            hint={t("settings.autoCopyHint")}
            checked={terminalAutoCopy}
            onChange={setTerminalAutoCopy}
          />
        </SettingGroup>

        <SettingGroup
          title={t("settings.sshSecurityTitle")}
          hint={t("settings.sshSecurityHint")}
        >
          <PillSetting
            title={t("settings.sshPreset")}
            hint={t("settings.sshPresetHint")}
            value={sshSecurity.preset}
            onChange={(value) => setSshPreset(value as SshPreset)}
            options={[
              {
                value: "recommended",
                label: t("settings.sshPresetRecommended"),
              },
              {
                value: "compatible",
                label: t("settings.sshPresetCompatible"),
              },
              { value: "strict", label: t("settings.sshPresetStrict") },
              { value: "custom", label: t("settings.sshPresetCustom") },
            ]}
          />
          <details className="ssh-algo-advanced">
            <summary>{t("settings.sshAdvanced")}</summary>
            <p className="hint">{t("settings.sshAdvancedHint")}</p>
            {SSH_ALGORITHM_GROUPS.map((group) => {
              const key = group.key as SshAlgorithmGroupKey;
              const selected = sshSecurity[key];
              return (
                <div key={key} className="ssh-algo-group">
                  <div className="ssh-algo-group-head">
                    <span className="ssh-algo-group-title">
                      {t(group.labelKey)}
                    </span>
                    <span className="ssh-algo-count">
                      {t("settings.sshEnabledCount", {
                        enabled: selected.length,
                        total: group.items.length,
                      })}
                    </span>
                    <button
                      type="button"
                      className="link-btn ssh-algo-recommend"
                      onClick={() =>
                        setSshAlgorithmGroup(key, [...SSH_RECOMMENDED[key]])
                      }
                    >
                      {t("settings.sshUseRecommended")}
                    </button>
                  </div>
                  <CheckList
                    label={t(group.labelKey)}
                    value={[...selected]}
                    onChange={(next) => setSshAlgorithmGroup(key, next)}
                    options={group.items.map((name) => ({
                      value: name,
                      label: name,
                      hint: name,
                    }))}
                  />
                </div>
              );
            })}
          </details>
        </SettingGroup>

        <SettingGroup
          title={t("settings.group.connection")}
          hint={t("settings.group.connectionHint")}
          onReset={resetConnection}
          resetLabel={t("settings.resetGroup")}
        >
          <SettingRow
            title={t("settings.keepalive")}
            hint={t("settings.keepaliveHint")}
          >
            <input
              type="number"
              min={0}
              max={600}
              value={keepaliveSecs}
              onChange={(e) =>
                setKeepalive(Number.parseInt(e.target.value, 10) || 0)
              }
            />
          </SettingRow>
          <ToggleSetting
            title={t("settings.osc52")}
            hint={t("settings.osc52Hint")}
            checked={osc52}
            onChange={setOsc52}
          />
        </SettingGroup>

        <SettingGroup
          title={t("settings.highlightsTitle")}
          hint={t("settings.highlightsHint")}
        >
          <SelectSetting
            title={t("settings.highlightNew")}
            hint={t("settings.highlightColorHint")}
            label={t("settings.highlightColor")}
            value={colorDraft}
            onChange={setColorDraft}
            options={HIGHLIGHT_COLORS.map((color) => ({
              value: color,
              label: t(
                `settings.highlightColor_${color.replace("--tag-", "")}`,
              ),
            }))}
          />
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
        </SettingGroup>
            </>
          )}
          {category === "keyboard" && <ShortcutEditor />}
          {category === "transfer" && (
            <>
        <SettingGroup
          title={t("settings.group.transfer")}
          hint={t("settings.group.transferHint")}
          onReset={resetTransfer}
          resetLabel={t("settings.resetGroup")}
        >
          <ToggleSetting
            title={t("settings.sftpShowHidden")}
            hint={t("settings.sftpShowHiddenHint")}
            checked={sftpShowHidden}
            onChange={setSftpShowHidden}
          />
          <SelectSetting
            title={t("settings.transferConcurrency")}
            hint={t("settings.transferConcurrencyHint")}
            value={String(transferConcurrency)}
            onChange={(value) => setTransferConcurrency(Number(value))}
            options={Array.from({ length: 8 }, (_, index) => index + 1).map(
              (value) => ({ value: String(value), label: String(value) }),
            )}
          />
          <ToggleSetting
            title={t("settings.editorAutoSave")}
            hint={t("settings.editorAutoSaveHint")}
            checked={editorAutoSave}
            onChange={setEditorAutoSave}
          />
        </SettingGroup>
            </>
          )}
          {category === "sync" && (
            <>
        <SettingGroup title={t("sync.title")} hint={t("sync.hint")}>
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
          <ToggleSetting
            title={t("sync.includeSecrets")}
            hint={t("sync.includeSecretsHint")}
            checked={includeSecrets}
            onChange={setIncludeSecrets}
          />
          <div className="sync-actions">
            <button
              type="button"
              onClick={exportBackup}
              disabled={busy || !passphrase}
            >
              <DownloadIcon size={12} />
              {t("sync.export")}
            </button>
            <button
              type="button"
              onClick={() => fileRef.current?.click()}
              disabled={busy || !passphrase}
            >
              <UploadIcon size={12} />
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
        </SettingGroup>

        <SettingGroup title={t("sync.remoteTitle")} hint={t("sync.remoteHint")}>
          <SegmentedSetting
            title={t("sync.provider")}
            value={remoteKind}
            onChange={(value) => setRemoteKind(value as "gist" | "webdav")}
            options={[
              { value: "gist", label: t("sync.providerGist") },
              { value: "webdav", label: t("sync.providerWebdav") },
            ]}
          />
          {remoteKind === "webdav" && (
            <label className="field">
              <span>{t("sync.webdavUrl")}</span>
              <input
                value={davUrl}
                onChange={(e) => setDavUrl(e.target.value)}
                placeholder="https://dav.example.com/backups"
              />
            </label>
          )}
          {remoteKind === "gist" ? (
            <>
              <label className="field">
                <span>{t("sync.gistToken")}</span>
                <PasswordInput
                  value={gistToken}
                  onChange={setGistToken}
                  placeholder={
                    gistTokenSaved ? t("sync.secretPlaceholder") : undefined
                  }
                  shown={showPassword}
                  onToggle={() => setShowPassword(!showPassword)}
                />
                {gistTokenSaved && (
                  <span className="saved-secret">
                    <span className="badge ok">{t("sync.secretSaved")}</span>
                    <button
                      type="button"
                      className="link-btn"
                      onClick={() => void forgetSecret("sync:gist-token", "gist")}
                    >
                      {t("sync.forget")}
                    </button>
                  </span>
                )}
                <span className="hint">{t("sync.secretHint")}</span>
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
                  placeholder={
                    davPassSaved ? t("sync.secretPlaceholder") : undefined
                  }
                  shown={showPassword}
                  onToggle={() => setShowPassword(!showPassword)}
                />
                {davPassSaved && (
                  <span className="saved-secret">
                    <span className="badge ok">{t("sync.secretSaved")}</span>
                    <button
                      type="button"
                      className="link-btn"
                      onClick={() =>
                        void forgetSecret("sync:webdav-password", "webdav")
                      }
                    >
                      {t("sync.forget")}
                    </button>
                  </span>
                )}
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
              <CheckIcon size={12} />
              {t("sync.test")}
            </button>
            <button
              type="button"
              onClick={() => void runRemote("upload")}
              disabled={remoteBusy || !passphrase}
            >
              <UploadIcon size={12} />
              {t("sync.upload")}
            </button>
            <button
              type="button"
              onClick={() => void runRemote("download")}
              disabled={remoteBusy || !passphrase}
            >
              <DownloadIcon size={12} />
              {t("sync.download")}
            </button>
          </div>
          {remoteMessage && (
            <p className="hint sync-message">{remoteMessage}</p>
          )}
        </SettingGroup>

        <SettingGroup title={t("sync.autoTitle")} hint={t("sync.autoHint")}>
          <SelectSetting
            title={t("sync.policy")}
            value={syncPolicy}
            onChange={(value) => setSyncPolicy(value as SyncPolicy)}
            options={[
              { value: "manual", label: t("sync.policyManual") },
              { value: "onChange", label: t("sync.policyOnChange") },
              { value: "interval", label: t("sync.policyInterval") },
            ]}
          />
          {syncPolicy === "interval" && (
            <SelectSetting
              title={t("sync.interval")}
              value={String(syncInterval)}
              onChange={(value) =>
                setSyncInterval(Number(value) as SyncInterval)
              }
              options={[15, 30, 60].map((minutes) => ({
                value: String(minutes),
                label: t("sync.intervalMinutes", { count: minutes }),
              }))}
            />
          )}
          <div className="field">
            <span>{t("sync.scope")}</span>
            <CheckList
              label={t("sync.scope")}
              value={syncScope}
              onChange={(value) => setSyncScope(value as SyncScope[])}
              options={[
                { value: "hosts", label: t("sync.scopeHosts") },
                { value: "secrets", label: t("sync.scopeSecrets") },
                { value: "highlights", label: t("sync.scopeHighlights") },
                { value: "macros", label: t("sync.scopeMacros") },
                { value: "shortcuts", label: t("sync.scopeShortcuts") },
              ]}
            />
            <span className="hint">{t("sync.scopeHint")}</span>
          </div>
          <div className="sync-actions">
            <button
              type="button"
              onClick={() => void saveAutoPassphrase()}
              disabled={autoBusy || !passphrase}
            >
              {t("sync.passphraseSave")}
            </button>
            {autoPassSaved && (
              <button
                type="button"
                className="link-btn danger"
                onClick={() => void forgetAutoPassphrase()}
                disabled={autoBusy}
              >
                {t("sync.passphraseForget")}
              </button>
            )}
            <button
              type="button"
              onClick={() => void syncNow()}
              disabled={autoBusy}
            >
              {t("sync.syncNow")}
            </button>
          </div>
          <p className="hint">
            {autoPassSaved
              ? t("sync.passphraseSaved")
              : t("sync.passphraseNeeded")}
            {" · "}
            {t("sync.lastSync")}: {" "}
            {syncLastAt
              ? new Date(syncLastAt).toLocaleString()
              : t("sync.lastSyncNever")}
          </p>
          {autoMessage && (
            <p className="hint sync-message">{autoMessage}</p>
          )}
          {syncStatus.state !== "idle" && (
            <p className="hint sync-message">
              {syncStatus.state === "conflict"
                ? t("sync.statusConflict")
                : syncStatus.state === "error"
                  ? t("sync.statusError", {
                      time: new Date(syncStatus.at).toLocaleTimeString(),
                      error: syncStatus.message,
                    })
                  : t("sync.statusOk", {
                      time: new Date(syncStatus.at).toLocaleTimeString(),
                    })}
            </p>
          )}
          {(syncStatus.state === "conflict" || remoteConflict) && (
            <div className="sync-actions">
              <button
                type="button"
                onClick={() => void runRemote("download")}
                disabled={remoteBusy}
              >
                {t("sync.pullRemote")}
              </button>
              <button
                type="button"
                className="link-btn danger"
                onClick={() => void runRemote("upload", true)}
                disabled={remoteBusy}
              >
                {t("sync.overwriteRemote")}
              </button>
            </div>
          )}
        </SettingGroup>

        <SettingGroup
          title={t("sync.versionsTitle")}
          hint={t("sync.versionsHint")}
        >
          <div className="sync-actions">
            <button
              type="button"
              onClick={() => void loadVersions()}
              disabled={versionsBusy}
            >
              {t("sync.refresh")}
            </button>
          </div>
          {versions.length === 0 ? (
            <p className="hint">{t("sync.versionsEmpty")}</p>
          ) : (
            <div className="sync-version-list">
              {versions.map((entry) => (
                <div className="plugin-row sync-version-row" key={entry.id}>
                  <div className="sync-version-main">
                    <div className="sync-version-head">
                      <span className="sync-version-time">
                        {new Date(entry.createdAt * 1000).toLocaleString()}
                      </span>
                      <span className="badge">{sourceLabel(entry.source)}</span>
                      <span className="hint">{formatBytes(entry.bytes)}</span>
                    </div>
                    <span className="hint sync-version-summary">
                      {summaryText(entry.summary)}
                    </span>
                  </div>
                  <button
                    type="button"
                    className="link-btn"
                    disabled={restoringId !== null}
                    onClick={() => void restoreVersion(entry)}
                  >
                    {restoringId === entry.id
                      ? t("sync.restoring")
                      : t("sync.restore")}
                  </button>
                </div>
              ))}
            </div>
          )}
          {versionsMessage && (
            <p className="hint sync-message">{versionsMessage}</p>
          )}
        </SettingGroup>
            </>
          )}
          {category === "plugins" && (
            <>
        <div className="settings-tabs">
          <button
            type="button"
            className={
              pluginTab === "market" ? "settings-tab active" : "settings-tab"
            }
            aria-current={pluginTab === "market" ? "page" : undefined}
            onClick={() => {
              setPluginTab("market");
              void loadMarket();
            }}
          >
            {t("plugins.market")}
          </button>
          <button
            type="button"
            className={
              pluginTab === "installed"
                ? "settings-tab active"
                : "settings-tab"
            }
            aria-current={pluginTab === "installed" ? "page" : undefined}
            onClick={() => setPluginTab("installed")}
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
        {pluginTab === "installed" && (
        <fieldset className="settings-group">
          <p className="hint">{t("plugins.hint")}</p>
          <div className="plugin-actions">
            <button type="button" onClick={() => void refreshPlugins()}>
              <RefreshIcon size={12} />
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
                  {plugin.kind === "serial" && (
                    <button
                      type="button"
                      className="plugin-action"
                      onClick={() => {
                        setSerialDialogOpen(true);
                        onClose();
                      }}
                    >
                      {t("plugins.openSerial")}
                    </button>
                  )}
                  {(plugin.kind === "serial" || plugin.kind === "telnet") && (
                    <button
                      type="button"
                      className="plugin-action"
                      disabled={marketBusyId === plugin.id}
                      onClick={() => void uninstallPlugin(plugin)}
                    >
                      {marketBusyId === plugin.id
                        ? t("plugins.marketWorking")
                        : t("plugins.marketUninstall")}
                    </button>
                  )}
                </li>
              ))}
            </ul>
          )}
          {pluginScan && pluginScan.builtin.length > 0 && (
            <>
              <h3 className="plugin-section-title">
                {t("plugins.builtinTitle")}
              </h3>
              <p className="hint">{t("plugins.builtinHint")}</p>
              <ul className="plugin-list">
                {pluginScan.builtin.map((item) => {
                  const installed = pluginScan.plugins.some(
                    (plugin) => plugin.id === item.id,
                  );
                  const busy = marketBusyId === item.id;
                  return (
                    <li key={item.id} className="plugin-row">
                      <span className="plugin-icon">
                        <PuzzleIcon size={16} />
                      </span>
                      <span className="plugin-main">
                        <span className="plugin-name">
                          {item.name}
                          <span className="plugin-version"> {item.version}</span>
                          {installed && (
                            <span className="badge ok plugin-badge">
                              {t("plugins.marketInstalledBadge")}
                            </span>
                          )}
                        </span>
                        <span className="hint plugin-desc">
                          {t(`plugins.builtinDesc.${item.id}`, {
                            defaultValue: item.description,
                          })}
                        </span>
                      </span>
                      {installed ? (
                        <button
                          type="button"
                          className="plugin-action"
                          disabled={busy}
                          onClick={() => void uninstallPlugin(item)}
                        >
                          {busy
                            ? t("plugins.marketWorking")
                            : t("plugins.marketUninstall")}
                        </button>
                      ) : (
                        <button
                          type="button"
                          className="plugin-action"
                          disabled={busy || !isTauri}
                          onClick={() => void installBuiltin(item)}
                        >
                          {busy
                            ? t("plugins.marketWorking")
                            : t("plugins.builtinInstall")}
                        </button>
                      )}
                    </li>
                  );
                })}
              </ul>
            </>
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
        )}
        {pluginTab === "market" && (
        <fieldset className="settings-group">
          <p className="hint">{t("plugins.marketHint")}</p>
          <div className="plugin-actions">
            <button
              type="button"
              onClick={() => void loadMarket(true)}
              disabled={marketBusy || !isTauri}
            >
              {marketBusy
                ? t("plugins.marketLoading")
                : t("plugins.marketRefresh")}
            </button>
          </div>
          {marketMessage && (
            <p className="hint sync-message">{marketMessage}</p>
          )}
          {market && market.plugins.length > 0 && (
            <ul className="plugin-list">
              {market.plugins.map((entry) => {
                const installed = !!pluginScan?.plugins.some(
                  (plugin) => plugin.id === entry.id,
                );
                const busy = marketBusyId === entry.id;
                return (
                  <li key={entry.id} className="plugin-row">
                    <span className="plugin-icon">
                      <PuzzleIcon size={16} />
                    </span>
                    <span className="plugin-main">
                      <span className="plugin-name">
                        {entry.name}
                        {entry.version && (
                          <span className="plugin-version">
                            {" "}
                            {entry.version}
                          </span>
                        )}
                        {installed && (
                          <span className="badge ok plugin-badge">
                            {t("plugins.marketInstalledBadge")}
                          </span>
                        )}
                      </span>
                      <span className="hint plugin-desc">
                        {entry.description ||
                          t(`plugins.kinds.${entry.kind}`, {
                            defaultValue: entry.kind,
                          })}
                        {entry.author && ` · ${entry.author}`}
                      </span>
                    </span>
                    {installed ? (
                      <span className="plugin-actions-inline">
                        {entry.kind === "serial" && (
                          <button
                            type="button"
                            className="plugin-action"
                            disabled={busy}
                            onClick={() => {
                              setSerialDialogOpen(true);
                              onClose();
                            }}
                          >
                            {t("plugins.marketConnect")}
                          </button>
                        )}
                        <button
                          type="button"
                          className="plugin-action"
                          disabled={busy}
                          onClick={() =>
                            void uninstallPlugin(entry, "market")
                          }
                        >
                          {busy
                            ? t("plugins.marketWorking")
                            : t("plugins.marketUninstall")}
                        </button>
                      </span>
                    ) : (
                      <button
                        type="button"
                        className="plugin-action"
                        disabled={busy || !isTauri}
                        onClick={() => void installMarket(entry)}
                      >
                        {busy
                          ? t("plugins.marketWorking")
                          : t("plugins.marketInstall")}
                      </button>
                    )}
                  </li>
                );
              })}
            </ul>
          )}
          <p className="hint">{t("plugins.marketSource")}</p>
        </fieldset>
        )}
            </>
          )}
          {category === "about" && (
            <>
        <div className="settings-about">
          <div className="about-brand">
            <LogoMark size={26} />
            <p className="about-name">{t("app.name")}</p>
          </div>
          <p className="about-version">
            <span className="hint">{t("about.devVersion")}</span>
            <Chip tone="info">{DEV_VERSION}</Chip>
          </p>
          <VersionHistory />
          <UpdatePanel />
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
        </div>
            </>
          )}
          </SettingsSearchContext.Provider>
          </div>
        </div>
      </div>
    </Modal>
  );
}
