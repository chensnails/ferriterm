import { create } from "zustand";
import {
  api,
  isTauri,
  SYNC_SCOPES,
  type SyncScope,
  type HostEntry,
  type HostInput,
  type SessionKind,
  type Tag,
  type PluginScan,
  type TransferJob,
  type SshAlgorithms,
  type SshPreset,
  type SshSecuritySettings,
  type PolicyDecision,
} from "./api";
import {
  DEFAULT_SSH_SECURITY,
  SSH_PRESETS,
  parseSshSecurity,
  type SshAlgorithmGroupKey,
} from "./ssh-algorithms";
import {
  planHostConnect,
  stripPendingSecrets,
  type PendingConnect,
} from "./connect";
import { isSensitiveCommand } from "./commands";
import {
  normalizeCombo,
  parseShortcuts,
  SHORTCUT_DEFAULTS,
  type ShortcutAction,
  type ShortcutBindings,
} from "./keybindings";
import {
  applyTheme,
  applyThemePack,
  applyUiFont,
  applyMotion,
  builtinPalette,
  HIGHLIGHT_COLORS,
  normalizeThemeId,
  setTerminalPalette,
  type Theme,
} from "./theme";
import { parseStoredThemes, type ImportedTheme } from "./scheme";
import { DEFAULT_TERMINAL_FONT_STACK } from "./fonts";
import { initialWindowChrome, type WindowChrome } from "./window-chrome";
import i18n, {
  detectLanguage,
  registerLanguagePack,
  type Language,
} from "./i18n";

export type SyncPolicy = "manual" | "onChange" | "interval";
export type SyncInterval = 15 | 30 | 60;

export interface SyncStatus {
  state: "idle" | "ok" | "error" | "conflict";
  /** Human-readable detail; empty for idle and ok. */
  message: string;
  /** Unix ms of the attempt this status describes. */
  at: number;
}

/** Credentials are opt-in: an automatic bundle never leaves the local vault
 * unless the user explicitly adds the "secrets" scope. */
export const DEFAULT_SYNC_SCOPE: SyncScope[] = SYNC_SCOPES.filter(
  (scope) => scope !== "secrets",
);

function parseSyncPolicy(raw: string | undefined): SyncPolicy {
  return raw === "onChange" || raw === "interval" ? raw : "manual";
}

function parseSyncInterval(raw: string | undefined): SyncInterval {
  const value = Number(raw);
  return value === 15 || value === 30 || value === 60 ? value : 30;
}

function parseSyncScope(raw: string | undefined): SyncScope[] {
  if (raw === undefined) return [...DEFAULT_SYNC_SCOPE];
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [...DEFAULT_SYNC_SCOPE];
    return SYNC_SCOPES.filter((scope) => parsed.includes(scope));
  } catch {
    return [...DEFAULT_SYNC_SCOPE];
  }
}

// Terminal appearance defaults live here (instead of terminal.ts) so the store
// can own persisted settings without a circular import back into terminal.ts.
export const DEFAULT_TERMINAL_FONT_SIZE = 14;
export const DEFAULT_TERMINAL_FONT_FAMILY = DEFAULT_TERMINAL_FONT_STACK;
export const DEFAULT_TERMINAL_SCROLLBACK = 5000;
/** Interface type size. 13 is what the stylesheet's token block starts from. */
export const DEFAULT_UI_FONT_SIZE = 13;

// Terminal render / interaction preferences live here (rather than terminal.ts)
// so the store owns persisted settings without a circular import.
export type TerminalCursorStyle = "block" | "underline" | "bar";
export type TerminalBell = "off" | "visual" | "sound";

export interface LanguageOption {
  id: string;
  name: string;
  locale: string;
}

export interface HistoryEntry {
  id: string;
  sessionId: string;
  sessionTitle: string;
  text: string;
  at: number;
  hostId?: string;
}

export interface KeywordRule {
  text: string;
  color: string;
}

const DEFAULT_KEYWORD_RULES: KeywordRule[] = [
  { text: "ERROR", color: "--tag-red" },
  { text: "WARN", color: "--tag-amber" },
];

function sanitizeKeywordRules(rules: KeywordRule[]): KeywordRule[] {
  const seen = new Set<string>();
  const cleaned: KeywordRule[] = [];
  for (const rule of rules) {
    const text = rule.text.trim();
    const key = text.toLowerCase();
    if (!text || text.length > 64 || seen.has(key)) continue;
    seen.add(key);
    cleaned.push({
      text,
      color: HIGHLIGHT_COLORS.includes(rule.color)
        ? rule.color
        : HIGHLIGHT_COLORS[0],
    });
    if (cleaned.length >= 20) break;
  }
  return cleaned;
}

function parseKeywordRules(raw: string | undefined): KeywordRule[] {
  if (!raw) return DEFAULT_KEYWORD_RULES;
  try {
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return DEFAULT_KEYWORD_RULES;
    return sanitizeKeywordRules(
      parsed.filter(
        (entry): entry is KeywordRule =>
          Boolean(entry) &&
          typeof entry.text === "string" &&
          typeof entry.color === "string",
      ),
    );
  } catch {
    return DEFAULT_KEYWORD_RULES;
  }
}

export interface ExecLogEntry {
  id: string;
  at: number;
  kind: "command" | "event";
  sessionTitle: string;
  text: string;
}

export type { PendingConnect } from "./connect";

export interface SessionMeta {
  id: string;
  kind: SessionKind;
  title: string;
  closed: boolean;
  production?: boolean;
  hostId?: string;
  recording?: boolean;
  recordingPath?: string;
  pending?: PendingConnect;
  lastConnect?: PendingConnect;
}

interface StoredSession {
  kind: SessionKind;
  title: string;
  hostId?: string;
  production?: boolean;
}

/** One restart-to-restart description of the window. Version 1 was a bare
    array of sessions in the `layout` setting; version 2 is an object in
    `layout.snapshot` that also remembers the active tab, the split and the
    sidebar. `parseLayoutSnapshot` upgrades the old shape on read. */
interface LayoutSnapshot {
  version: 2;
  sessions: StoredSession[];
  activeIndex: number;
  split: { index: number; direction: SplitDirection; ratio: number } | null;
  sidebar: { open: boolean; view: SidebarView };
}

interface Status {
  text: string;
  isError: boolean;
}

/** In-app stand-in for `window.confirm` / `window.prompt`. The embedded
    WebView implements neither, so the native dialogs silently do nothing;
    these carry the same request through a React dialog and resolve the
    caller's promise when it settles. */
export interface ConfirmPrompt {
  kind: "confirm";
  title: string;
  body?: string;
  confirmLabel?: string;
  danger?: boolean;
}

export interface TextPrompt {
  kind: "text";
  title: string;
  body?: string;
  value: string;
  placeholder?: string;
}

export type AppPrompt = ConfirmPrompt | TextPrompt | ApprovePrompt;

/** How long an approval lasts. `always` writes a rule; the others are memory
    only, so a session cannot quietly widen what future sessions may run. */
export type ApprovalChoice = "once" | "session" | "always";

/** The command gate. Shown when the policy answers "ask" — never for a
    command the person typed themselves. */
export interface ApprovePrompt {
  kind: "approve";
  command: string;
  host: string;
  decision: PolicyDecision;
}

interface PendingPrompt {
  request: AppPrompt;
  settle: (value: string | boolean | null) => void;
}

let statusTimer: number | undefined;
const STATUS_INFO_MS = 4500;
const STATUS_ERROR_MS = 9000;

export type SplitDirection = "horizontal" | "vertical";

export type TabNumbering = "asNeed" | "always" | "none";

/** Which module the left rail currently shows. */
export type SidebarView = "hosts" | "tools";

export type NetToolId = "ping" | "geoip" | "ipcalc";

/** Companion panels that live in the right dock. */
export type DockCardId = "commands" | "monitor" | "log" | "ai";

export const DOCK_WIDTH_MIN = 252;
export const DOCK_WIDTH_MAX = 560;
export const DOCK_CARD_MIN_H = 132;

function parseIdList(raw: string | undefined): string[] {
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed)
      ? parsed.filter((item): item is string => typeof item === "string")
      : [];
  } catch {
    return [];
  }
}

function persistTransferTabs(tabs: string[]) {
  if (isTauri) {
    api.setSetting("transferTabs", JSON.stringify(tabs)).catch(() => {});
  }
}

export const TRANSFER_CONCURRENCY_MIN = 1;
export const TRANSFER_CONCURRENCY_MAX = 8;

function clampTransferConcurrency(value: number): number {
  if (!Number.isFinite(value)) return 6;
  return Math.min(
    TRANSFER_CONCURRENCY_MAX,
    Math.max(TRANSFER_CONCURRENCY_MIN, Math.round(value)),
  );
}

const DOCK_CARD_DEFAULT_H: Record<DockCardId, number> = {
  commands: 300,
  monitor: 296,
  log: 208,
  ai: 340,
};

function clampDockWidth(width: number): number {
  if (!Number.isFinite(width)) return 320;
  return Math.min(DOCK_WIDTH_MAX, Math.max(DOCK_WIDTH_MIN, Math.round(width)));
}

const SIDEBAR_W_DEFAULT = 300;
const SIDEBAR_W_MIN = 220;
const SIDEBAR_W_MAX = 460;

function clampSidebarWidth(width: number): number {
  if (!Number.isFinite(width) || width <= 0) return SIDEBAR_W_DEFAULT;
  return Math.min(SIDEBAR_W_MAX, Math.max(SIDEBAR_W_MIN, Math.round(width)));
}

let sidebarWidthTimer: number | undefined;

function scheduleSidebarWidthSave(get: () => AppState) {
  if (!isTauri) return;
  window.clearTimeout(sidebarWidthTimer);
  sidebarWidthTimer = window.setTimeout(() => {
    const width = get().sidebarWidth;
    api.setSetting("sidebarWidth", String(width)).catch(() => {});
  }, 300);
}

function clampDockCardHeight(id: DockCardId, height: number): number {
  if (!Number.isFinite(height)) return DOCK_CARD_DEFAULT_H[id];
  return Math.min(900, Math.max(DOCK_CARD_MIN_H, Math.round(height)));
}

function parseDockHeights(raw: string | undefined): Record<DockCardId, number> {
  const heights = { ...DOCK_CARD_DEFAULT_H };
  if (!raw) return heights;
  try {
    const parsed = JSON.parse(raw) as Partial<Record<DockCardId, unknown>>;
    if (!parsed || typeof parsed !== "object") return heights;
    (Object.keys(heights) as DockCardId[]).forEach((id) => {
      const value = parsed[id];
      if (typeof value === "number") heights[id] = clampDockCardHeight(id, value);
    });
  } catch {
    // a malformed blob just falls back to the defaults
  }
  return heights;
}

// Resizing a card fires on every pointer move, so persistence is debounced.
let dockSaveTimer: number | undefined;

function scheduleDockSave(read: () => {
  dockWidth: number;
  dockHeights: Record<DockCardId, number>;
}): void {
  if (!isTauri) return;
  window.clearTimeout(dockSaveTimer);
  dockSaveTimer = window.setTimeout(() => {
    const { dockWidth, dockHeights } = read();
    api.setSetting("dockWidth", String(dockWidth)).catch(() => {});
    api.setSetting("dockHeights", JSON.stringify(dockHeights)).catch(() => {});
  }, 320);
}

export const HOME_TAB = "home";
export const TRANSFER_TAB = "transfer";

interface AppState {
  sessions: SessionMeta[];
  activeId: string | null;
  splitId: string | null;
  splitDirection: SplitDirection;
  splitRatio: number;
  layoutRestored: boolean;
  splitActive: () => void;
  focusedSessionId: string | null;
  setFocusedSession: (id: string | null) => void;
  commandHistory: HistoryEntry[];
  recordCommand: (sessionId: string, text: string) => void;
  clearCommandHistory: () => void;
  execLog: ExecLogEntry[];
  execLogOpen: boolean;
  toggleExecLog: () => void;
  aiOpen: boolean;
  toggleAi: () => void;
  /** Bumped whenever providers or their keys change, so the panel refetches. */
  aiConfigVersion: number;
  bumpAiConfig: () => void;
  clearExecLog: () => void;
  logEvent: (text: string) => void;
  broadcastTargets: string[];
  broadcastRequested: boolean;
  sendInput: (kind: SessionKind, id: string, data: string) => void;
  requestBroadcast: () => void;
  cancelBroadcast: () => void;
  startBroadcast: (ids: string[]) => void;
  stopBroadcast: () => void;
  splitWith: (id: string, direction: SplitDirection) => void;
  closeSplit: () => void;
  setSplitDirection: (direction: SplitDirection) => void;
  setSplitRatio: (ratio: number) => void;
  theme: Theme;
  themeVersion: number;
  importedThemes: ImportedTheme[];
  importTheme: (scheme: ImportedTheme) => void;
  removeImportedTheme: (id: string) => void;
  language: Language;
  languagePacks: LanguageOption[];
  pluginScan: PluginScan | null;
  commandLibraryVersion: number;
  bumpCommandLibrary: () => void;
  status: Status;
  appPrompt: PendingPrompt | null;
  askConfirm: (request: Omit<ConfirmPrompt, "kind">) => Promise<boolean>;
  askText: (
    request: Omit<TextPrompt, "kind" | "value"> & { value?: string },
  ) => Promise<string | null>;
  settlePrompt: (value: string | boolean | null) => void;
  askApprove: (request: {
    command: string;
    host: string;
    decision: PolicyDecision;
  }) => Promise<ApprovalChoice | null>;
  clearStatus: () => void;
  connectRetry: { hostId: string; error: string } | null;
  reportConnectError: (hostId: string, error: string) => void;
  clearConnectRetry: () => void;
  hosts: HostEntry[];
  tags: Tag[];
  hostFilter: string;
  collapsedFolders: Record<string, boolean>;
  selectedHostId: string | null;
  newWindowHostId: string | null;
  monitorOpen: boolean;
  toggleMonitor: () => void;
  /**
   * Host the monitor panel reads from: "local" or an open SSH session id.
   * Lifted out of the panel so the status-bar readout and the card cannot
   * disagree about which machine they are showing.
   */
  monitorTarget: string;
  setMonitorTarget: (target: string) => void;
  /** Persistent status-bar monitor readout (on by default). */
  statusMonitor: boolean;
  setStatusMonitor: (enabled: boolean) => void;
  /** Interface type size in px (11–16); drives every `--fs-*` token. */
  uiFontSize: number;
  setUiFontSize: (size: number) => void;
  /** Global switch for transitions and animations. */
  reduceMotion: boolean;
  setReduceMotion: (enabled: boolean) => void;
  /** When on, opening a dock card closes the other two. */
  dockSingleCard: boolean;
  setDockSingleCard: (enabled: boolean) => void;
  forwardOpen: boolean;
  setForwardOpen: (open: boolean) => void;
  toggleForward: () => void;
  transferOpen: boolean;
  transferSessionId: string | null;
  openTransfer: (sessionId?: string | null) => void;
  closeTransfer: () => void;
  setTransferSession: (id: string | null) => void;
  /** Remote targets kept open as tabs in the transfer view. */
  transferTabs: string[];
  openTransferTab: (id: string) => void;
  closeTransferTab: (id: string) => void;
  goHome: () => void;
  quickConnectOpen: boolean;
  setQuickConnectOpen: (open: boolean) => void;
  serialDialogOpen: boolean;
  setSerialDialogOpen: (open: boolean) => void;
  transfers: TransferJob[];
  upsertTransfer: (job: TransferJob) => void;
  setTransfers: (jobs: TransferJob[]) => void;
  keepaliveSecs: number;
  setKeepalive: (seconds: number) => void;
  autoLockMinutes: number;
  setAutoLockMinutes: (minutes: number) => void;
  terminalFontFamily: string;
  setTerminalFontFamily: (value: string) => void;
  terminalFontSize: number;
  setTerminalFontSize: (value: number) => void;
  terminalScrollback: number;
  setTerminalScrollback: (value: number) => void;
  terminalCursorBlink: boolean;
  setTerminalCursorBlink: (value: boolean) => void;
  terminalCursorStyle: TerminalCursorStyle;
  setTerminalCursorStyle: (value: TerminalCursorStyle) => void;
  terminalBell: TerminalBell;
  setTerminalBell: (value: TerminalBell) => void;
  terminalRightClickPaste: boolean;
  setTerminalRightClickPaste: (value: boolean) => void;
  terminalScrollOnOutput: boolean;
  setTerminalScrollOnOutput: (value: boolean) => void;
  terminalAutoCopy: boolean;
  setTerminalAutoCopy: (value: boolean) => void;
  sshSecurity: SshSecuritySettings;
  setSshPreset: (preset: SshPreset) => void;
  setSshAlgorithmGroup: (
    group: SshAlgorithmGroupKey,
    names: string[],
  ) => void;
  osc52: boolean;
  setOsc52: (enabled: boolean) => void;
  keywordHighlights: KeywordRule[];
  setKeywordHighlights: (rules: KeywordRule[]) => void;
  tabNumbering: TabNumbering;
  setTabNumbering: (mode: TabNumbering) => void;
  /** Whether the saved tab/split layout is restored at startup. */
  restoreLayoutOnStartup: boolean;
  setRestoreLayoutOnStartup: (value: boolean) => void;
  /** Restores sidebar/dock/split dimensions to their shipped defaults. */
  resetLayoutSizes: () => void;
  confirmClose: boolean;
  setConfirmClose: (value: boolean) => void;
  sftpShowHidden: boolean;
  setSftpShowHidden: (value: boolean) => void;
  /** Write an edited file back on its own once typing pauses. */
  editorAutoSave: boolean;
  setEditorAutoSave: (value: boolean) => void;
  /** How many transfers may run at once (1–8). */
  transferConcurrency: number;
  setTransferConcurrency: (value: number) => void;
  /** Cloud sync: trigger mode, scheduled interval and included data types. */
  syncPolicy: SyncPolicy;
  setSyncPolicy: (policy: SyncPolicy) => void;
  syncInterval: SyncInterval;
  setSyncInterval: (minutes: SyncInterval) => void;
  syncScope: SyncScope[];
  setSyncScope: (scope: SyncScope[]) => void;
  /** Unix ms of the last successful sync; 0 when never. */
  syncLastAt: number;
  setSyncLastAt: (at: number) => void;
  /** Outcome of the most recent sync attempt, shown in Settings and the
   * status bar. Session state: it describes this run, not persisted history. */
  syncStatus: SyncStatus;
  setSyncStatus: (status: SyncStatus) => void;
  shortcuts: ShortcutBindings;
  setShortcut: (action: ShortcutAction, combo: string) => void;
  resetShortcuts: () => void;
  commandsOpen: boolean;
  toggleCommands: () => void;
  sidebarOpen: boolean;
  toggleSidebar: () => void;
  sidebarView: SidebarView;
  showSidebarView: (view: SidebarView) => void;
  /** Width of the host sidebar in CSS pixels, clamped and persisted. */
  sidebarWidth: number;
  setSidebarWidth: (width: number) => void;
  netTool: NetToolId;
  setNetTool: (tool: NetToolId) => void;
  /** Width of the right dock and the height of each card inside it. */
  dockWidth: number;
  dockHeights: Record<DockCardId, number>;
  setDockWidth: (width: number) => void;
  setDockCardHeight: (id: DockCardId, height: number) => void;
  /** Frame shape the header must draw around, resolved once at startup. */
  windowChrome: WindowChrome;
  setWindowChrome: (chrome: WindowChrome) => void;
  paletteOpen: boolean;
  togglePalette: () => void;
  closePalette: () => void;
  searchOpen: boolean;
  openSearch: () => void;
  closeSearch: () => void;
  settingsOpen: boolean;
  setSettingsOpen: (open: boolean) => void;
  shortcutsOpen: boolean;
  setShortcutsOpen: (open: boolean) => void;
  recordingsOpen: boolean;
  setRecordingsOpen: (open: boolean) => void;
  importRequested: boolean;
  requestImport: () => void;
  clearImportRequest: () => void;
  newHostRequested: boolean;
  requestNewHost: () => void;
  clearNewHostRequest: () => void;
  newFolderRequested: boolean;
  requestNewFolder: () => void;
  clearNewFolderRequest: () => void;
  tagsRequested: boolean;
  requestTags: () => void;
  clearTagsRequest: () => void;
  connectRequestId: string | null;
  requestConnect: (id: string) => void;
  clearConnectRequest: () => void;
  openSession: (
    kind: SessionKind,
    title: string,
    pending: PendingConnect,
    production?: boolean,
    hostId?: string,
  ) => string;
  saveLayout: () => void;
  restoreLayout: () => Promise<void>;
  closeSession: (id: string) => void;
  renameSession: (id: string, title: string) => void;
  closeOtherSessions: (id: string) => void;
  closeAllSessions: () => void;
  pendingCloseId: string | null;
  requestCloseSession: (id: string) => void;
  cancelCloseSession: () => void;
  confirmCloseSession: () => void;
  activate: (id: string) => void;
  markClosed: (id: string) => void;
  startRecording: (id: string, cols: number, rows: number) => Promise<void>;
  stopRecording: (id: string) => Promise<void>;
  consumePending: (id: string) => PendingConnect | undefined;
  reconnect: (id: string) => void;
  /** Drops ephemeral passwords from a session so a retry cannot resend them. */
  scrubSessionSecrets: (id: string) => void;
  toggleTheme: () => void;
  setLanguage: (lng: Language) => void;
  setTheme: (theme: Theme) => void;
  loadSettings: () => Promise<void>;
  loadLanguagePacks: () => Promise<void>;
  setStatus: (text: string, isError?: boolean) => void;
  loadHosts: () => Promise<void>;
  loadTags: () => Promise<void>;
  createHost: (input: HostInput) => Promise<void>;
  updateHost: (input: HostInput) => Promise<void>;
  deleteHost: (id: string) => Promise<void>;
  createTag: (tag: Tag) => Promise<void>;
  updateTag: (tag: Tag) => Promise<void>;
  deleteTag: (id: string) => Promise<void>;
  setHostFilter: (text: string) => void;
  toggleFolder: (id: string) => void;
  selectHost: (id: string | null) => void;
  clearNewWindowHost: () => void;
}

/** Reads the window description, upgrading the version 1 array on the fly.
    A malformed snapshot falls back to the old key, and a malformed old key
    falls back to "start empty" rather than to a crash on launch. */
function parseLayoutSnapshot(
  raw: string | undefined,
  legacy: string | undefined,
): LayoutSnapshot | null {
  if (raw) {
    try {
      const parsed = JSON.parse(raw) as Partial<LayoutSnapshot>;
      if (parsed && parsed.version === 2 && Array.isArray(parsed.sessions)) {
        const split = parsed.split;
        return {
          version: 2,
          sessions: parsed.sessions.slice(0, 20),
          activeIndex:
            typeof parsed.activeIndex === "number" ? parsed.activeIndex : -1,
          split:
            split && typeof split.index === "number"
              ? {
                  index: split.index,
                  direction:
                    split.direction === "vertical" ? "vertical" : "horizontal",
                  ratio:
                    typeof split.ratio === "number" &&
                    split.ratio > 0 &&
                    split.ratio < 1
                      ? split.ratio
                      : 0.5,
                }
              : null,
          sidebar: {
            open: parsed.sidebar?.open !== false,
            view: parsed.sidebar?.view === "tools" ? "tools" : "hosts",
          },
        };
      }
    } catch {
      // fall through to the legacy shape
    }
  }
  if (!legacy) return null;
  try {
    const parsed = JSON.parse(legacy);
    if (!Array.isArray(parsed)) return null;
    return {
      version: 2,
      sessions: parsed.slice(0, 20),
      activeIndex: 0,
      split: null,
      sidebar: { open: true, view: "hosts" },
    };
  } catch {
    return null;
  }
}

function parseHistory(raw: string | undefined): HistoryEntry[] {
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed
      .filter(
        (entry): entry is HistoryEntry =>
          Boolean(entry) &&
          typeof entry.id === "string" &&
          typeof entry.sessionId === "string" &&
          typeof entry.sessionTitle === "string" &&
          typeof entry.text === "string" &&
          typeof entry.at === "number" &&
          (typeof entry.hostId === "undefined" ||
            typeof entry.hostId === "string") &&
          !isSensitiveCommand(entry.text),
      )
      .slice(0, 200);
  } catch {
    return [];
  }
}

let historySaveTimer: number | undefined;

function scheduleHistorySave() {
  if (!isTauri) return;
  window.clearTimeout(historySaveTimer);
  historySaveTimer = window.setTimeout(() => {
    const entries = useAppStore.getState().commandHistory;
    api.setSetting("commandHistory", JSON.stringify(entries)).catch(() => {});
  }, 600);
}

export const useAppStore = create<AppState>((set, get) => ({
  sessions: [],
  activeId: HOME_TAB,
  splitId: null,
  splitDirection: "horizontal",
  splitRatio: 0.5,
  broadcastTargets: [],
  broadcastRequested: false,
  focusedSessionId: null,
  commandHistory: [],
  execLog: [],
  execLogOpen: false,
  aiOpen: false,
  aiConfigVersion: 0,
  layoutRestored: false,
  theme: "dark",
  themeVersion: 0,
  importedThemes: [],
  language: detectLanguage(),
  languagePacks: [],
  pluginScan: null,
  commandLibraryVersion: 0,
  status: { text: "", isError: false },
  appPrompt: null,
  hosts: [],
  tags: [],
  hostFilter: "",
  collapsedFolders: {},
  selectedHostId: null,
  newWindowHostId: null,
  monitorOpen: false,
  monitorTarget: "local",
  statusMonitor: true,
  uiFontSize: DEFAULT_UI_FONT_SIZE,
  reduceMotion: false,
  dockSingleCard: false,
  forwardOpen: false,
  transferOpen: false,
  transferSessionId: null,
  transferTabs: [],
  quickConnectOpen: false,
  serialDialogOpen: false,
  transfers: [],
  keepaliveSecs: 30,
  autoLockMinutes: 0,
  terminalFontFamily: DEFAULT_TERMINAL_FONT_FAMILY,
  terminalFontSize: DEFAULT_TERMINAL_FONT_SIZE,
  terminalScrollback: DEFAULT_TERMINAL_SCROLLBACK,
  terminalCursorBlink: true,
  terminalCursorStyle: "block",
  terminalBell: "off",
  terminalRightClickPaste: false,
  terminalScrollOnOutput: true,
  terminalAutoCopy: false,
  sshSecurity: { ...DEFAULT_SSH_SECURITY },
  osc52: true,
  keywordHighlights: DEFAULT_KEYWORD_RULES,
  tabNumbering: "asNeed",
  restoreLayoutOnStartup: true,
  confirmClose: true,
  sftpShowHidden: false,
  editorAutoSave: false,
  transferConcurrency: 6,
  syncPolicy: "manual",
  syncInterval: 30,
  syncScope: [...DEFAULT_SYNC_SCOPE],
  syncLastAt: 0,
  syncStatus: { state: "idle", message: "", at: 0 },
  pendingCloseId: null,
  shortcuts: { ...SHORTCUT_DEFAULTS },
  commandsOpen: true,
  sidebarOpen: true,
  sidebarView: "hosts",
  sidebarWidth: SIDEBAR_W_DEFAULT,
  netTool: "ping",
  dockWidth: 320,
  dockHeights: { ...DOCK_CARD_DEFAULT_H },
  windowChrome: initialWindowChrome,
  paletteOpen: false,
  searchOpen: false,
  settingsOpen: false,
  shortcutsOpen: false,
  recordingsOpen: false,
  importRequested: false,
  tagsRequested: false,
  newHostRequested: false,
  newFolderRequested: false,
  connectRequestId: null,

  toggleMonitor: () =>
    set((s) => {
      const open = !s.monitorOpen;
      const single = s.dockSingleCard && open;
      if (isTauri) {
        api.setSetting("monitorOpen", open ? "1" : "0").catch(() => {});
        if (single) {
          api.setSetting("commandsOpen", "0").catch(() => {});
          api.setSetting("execLogOpen", "0").catch(() => {});
          api.setSetting("aiOpen", "0").catch(() => {});
        }
      }
      return {
        monitorOpen: open,
        ...(single ? { commandsOpen: false, execLogOpen: false, aiOpen: false } : {}),
      };
    }),
  setMonitorTarget: (monitorTarget) => set({ monitorTarget }),
  setStatusMonitor: (enabled) => {
    set({ statusMonitor: enabled });
    if (isTauri) {
      api.setSetting("statusMonitor", enabled ? "1" : "0").catch(() => {});
    }
  },
  setUiFontSize: (size) => {
    const next = Math.min(16, Math.max(11, Math.round(size)));
    set({ uiFontSize: next });
    applyUiFont(next);
    if (isTauri) api.setSetting("uiFontSize", String(next)).catch(() => {});
  },
  setReduceMotion: (enabled) => {
    set({ reduceMotion: enabled });
    applyMotion(enabled);
    if (isTauri) {
      api.setSetting("reduceMotion", enabled ? "1" : "0").catch(() => {});
    }
  },
  setDockSingleCard: (enabled) => {
    set({ dockSingleCard: enabled });
    if (isTauri) {
      api.setSetting("dockSingleCard", enabled ? "1" : "0").catch(() => {});
    }
  },
  setForwardOpen: (open) => set({ forwardOpen: open }),
  toggleForward: () => set((s) => ({ forwardOpen: !s.forwardOpen })),
  openTransfer: (sessionId) =>
    set((s) => {
      const bound =
        sessionId !== undefined ? sessionId : s.transferSessionId;
      let tabs = s.transferTabs;
      if (bound && !tabs.includes(bound)) {
        tabs = [...tabs, bound];
        persistTransferTabs(tabs);
      }
      return {
        transferOpen: true,
        activeId: TRANSFER_TAB,
        transferSessionId: bound,
        transferTabs: tabs,
      };
    }),
  closeTransfer: () =>
    set((s) => ({
      transferOpen: false,
      activeId:
        s.activeId === TRANSFER_TAB
          ? (s.sessions[s.sessions.length - 1]?.id ?? HOME_TAB)
          : s.activeId,
    })),
  setTransferSession: (id) => set({ transferSessionId: id }),
  openTransferTab: (id) =>
    set((s) => {
      if (s.transferTabs.includes(id)) {
        return { transferSessionId: id };
      }
      const tabs = [...s.transferTabs, id];
      persistTransferTabs(tabs);
      return { transferTabs: tabs, transferSessionId: id };
    }),
  closeTransferTab: (id) =>
    set((s) => {
      const index = s.transferTabs.indexOf(id);
      if (index < 0) return {};
      const tabs = s.transferTabs.filter((tab) => tab !== id);
      persistTransferTabs(tabs);
      const next =
        s.transferSessionId === id
          ? (tabs[Math.min(index, tabs.length - 1)] ?? null)
          : s.transferSessionId;
      return { transferTabs: tabs, transferSessionId: next };
    }),
  goHome: () => set({ activeId: HOME_TAB }),
  setQuickConnectOpen: (open) => set({ quickConnectOpen: open }),
  setSerialDialogOpen: (open) => set({ serialDialogOpen: open }),
  upsertTransfer: (job) =>
    set((s) => {
      const exists = s.transfers.some((item) => item.id === job.id);
      return {
        transfers: exists
          ? s.transfers.map((item) => (item.id === job.id ? job : item))
          : [...s.transfers, job],
      };
    }),
  setTransfers: (jobs) => set({ transfers: jobs }),
  setKeepalive: (seconds) => {
    const value = Math.max(0, Math.round(seconds));
    set({ keepaliveSecs: value });
    if (isTauri) api.setSetting("keepalive", String(value)).catch(() => {});
  },
  setAutoLockMinutes: (minutes) => {
    const allowed = [0, 5, 15, 30, 60];
    const value = allowed.includes(minutes) ? minutes : 0;
    set({ autoLockMinutes: value });
    if (isTauri) api.setSetting("vault:autoLock", String(value)).catch(() => {});
  },
  setTerminalFontFamily: (value) => {
    const terminalFontFamily = value.trim() || DEFAULT_TERMINAL_FONT_FAMILY;
    set({ terminalFontFamily });
    if (isTauri) {
      api.setSetting("terminalFontFamily", terminalFontFamily).catch(() => {});
    }
  },
  setTerminalFontSize: (value) => {
    const terminalFontSize = Math.min(
      32,
      Math.max(8, Math.round(value) || DEFAULT_TERMINAL_FONT_SIZE),
    );
    set({ terminalFontSize });
    if (isTauri) {
      api.setSetting("terminalFontSize", String(terminalFontSize)).catch(() => {});
    }
  },
  setTerminalScrollback: (value) => {
    const terminalScrollback = Math.min(
      50000,
      Math.max(200, Math.round(value) || DEFAULT_TERMINAL_SCROLLBACK),
    );
    set({ terminalScrollback });
    if (isTauri) {
      api
        .setSetting("terminalScrollback", String(terminalScrollback))
        .catch(() => {});
    }
  },
  setTerminalCursorBlink: (value) => {
    set({ terminalCursorBlink: value });
    if (isTauri) {
      api
        .setSetting("terminalCursorBlink", value ? "1" : "0")
        .catch(() => {});
    }
  },
  setTerminalCursorStyle: (value) => {
    set({ terminalCursorStyle: value });
    if (isTauri) {
      api.setSetting("terminalCursorStyle", value).catch(() => {});
    }
  },
  setTerminalBell: (value) => {
    set({ terminalBell: value });
    if (isTauri) api.setSetting("terminalBell", value).catch(() => {});
  },
  setTerminalRightClickPaste: (value) => {
    set({ terminalRightClickPaste: value });
    if (isTauri) {
      api
        .setSetting("terminalRightClickPaste", value ? "1" : "0")
        .catch(() => {});
    }
  },
  setTerminalScrollOnOutput: (value) => {
    set({ terminalScrollOnOutput: value });
    if (isTauri) {
      api
        .setSetting("terminalScrollOnOutput", value ? "1" : "0")
        .catch(() => {});
    }
  },
  setTerminalAutoCopy: (value) => {
    set({ terminalAutoCopy: value });
    if (isTauri) {
      api.setSetting("terminalAutoCopy", value ? "1" : "0").catch(() => {});
    }
  },
  setSshPreset: (preset) => {
    const base: SshAlgorithms =
      preset === "custom" ? get().sshSecurity : SSH_PRESETS[preset];
    const next: SshSecuritySettings = { ...base, preset };
    set({ sshSecurity: next });
    if (isTauri) {
      api.setSetting("sshSecurity", JSON.stringify(next)).catch(() => {});
    }
  },
  setSshAlgorithmGroup: (group, names) => {
    const previous = get().sshSecurity;
    const next: SshSecuritySettings = {
      ...previous,
      preset: "custom",
      [group]: names,
    };
    set({ sshSecurity: next });
    if (isTauri) {
      api.setSetting("sshSecurity", JSON.stringify(next)).catch(() => {});
    }
  },
  setOsc52: (enabled) => {
    set({ osc52: enabled });
    if (isTauri) api.setSetting("osc52", enabled ? "1" : "0").catch(() => {});
  },
  setKeywordHighlights: (rules) => {
    const keywordHighlights = sanitizeKeywordRules(rules);
    set({ keywordHighlights });
    if (isTauri) {
      api
        .setSetting("keywordHighlights", JSON.stringify(keywordHighlights))
        .catch(() => {});
    }
  },
  setTabNumbering: (mode) => {
    set({ tabNumbering: mode });
    if (isTauri) api.setSetting("tabNumbering", mode).catch(() => {});
  },
  setRestoreLayoutOnStartup: (value) => {
    set({ restoreLayoutOnStartup: value });
    if (isTauri) {
      api
        .setSetting("restoreLayoutOnStartup", value ? "1" : "0")
        .catch(() => {});
    }
  },
  resetLayoutSizes: () => {
    set({
      sidebarWidth: SIDEBAR_W_DEFAULT,
      dockWidth: 320,
      dockHeights: { ...DOCK_CARD_DEFAULT_H },
      splitRatio: 0.5,
    });
    if (isTauri) {
      void api
        .setSetting("sidebarWidth", String(SIDEBAR_W_DEFAULT))
        .catch(() => {});
      void api.setSetting("dockWidth", "320").catch(() => {});
      void api
        .setSetting("dockHeights", JSON.stringify(DOCK_CARD_DEFAULT_H))
        .catch(() => {});
    }
  },
  setConfirmClose: (value) => {
    set({ confirmClose: value });
    if (isTauri) api.setSetting("confirmClose", value ? "1" : "0").catch(() => {});
  },
  setSftpShowHidden: (value) => {
    set({ sftpShowHidden: value });
    if (isTauri) api.setSetting("sftpShowHidden", value ? "1" : "0").catch(() => {});
  },
  setEditorAutoSave: (value) => {
    set({ editorAutoSave: value });
    if (isTauri) api.setSetting("editorAutoSave", value ? "1" : "0").catch(() => {});
  },
  setTransferConcurrency: (value) => {
    const next = clampTransferConcurrency(value);
    set({ transferConcurrency: next });
    if (isTauri) {
      api.setSetting("transferConcurrency", String(next)).catch(() => {});
      api.setTransferConcurrency(next).catch(() => {});
    }
  },
  setSyncPolicy: (policy) => {
    set({ syncPolicy: policy });
    if (isTauri) api.setSetting("sync:policy", policy).catch(() => {});
  },
  setSyncInterval: (minutes) => {
    const next = parseSyncInterval(String(minutes));
    set({ syncInterval: next });
    if (isTauri) api.setSetting("sync:interval", String(next)).catch(() => {});
  },
  setSyncScope: (scope) => {
    const next = SYNC_SCOPES.filter((item) => scope.includes(item));
    set({ syncScope: next });
    if (isTauri)
      api.setSetting("sync:scope", JSON.stringify(next)).catch(() => {});
  },
  setSyncLastAt: (at) => {
    set({ syncLastAt: at });
    if (isTauri) api.setSetting("sync:lastAt", String(at)).catch(() => {});
  },
  setSyncStatus: (status) => set({ syncStatus: status }),
  setShortcut: (action, combo) => {
    const value = combo ? normalizeCombo(combo) : "";
    if (value === null) return;
    const shortcuts = { ...get().shortcuts, [action]: value };
    set({ shortcuts });
    if (isTauri) {
      api.setSetting("shortcuts", JSON.stringify(shortcuts)).catch(() => {});
    }
  },
  resetShortcuts: () => {
    const shortcuts = { ...SHORTCUT_DEFAULTS };
    set({ shortcuts });
    if (isTauri) {
      api.setSetting("shortcuts", JSON.stringify(shortcuts)).catch(() => {});
    }
  },
  toggleCommands: () =>
    set((s) => {
      const open = !s.commandsOpen;
      const single = s.dockSingleCard && open;
      if (isTauri) {
        api.setSetting("commandsOpen", open ? "1" : "0").catch(() => {});
        if (single) {
          api.setSetting("monitorOpen", "0").catch(() => {});
          api.setSetting("execLogOpen", "0").catch(() => {});
          api.setSetting("aiOpen", "0").catch(() => {});
        }
      }
      return {
        commandsOpen: open,
        ...(single ? { monitorOpen: false, execLogOpen: false, aiOpen: false } : {}),
      };
    }),
  toggleSidebar: () =>
    set((s) => {
      const open = !s.sidebarOpen;
      if (isTauri) api.setSetting("sidebarOpen", open ? "1" : "0").catch(() => {});
      return { sidebarOpen: open };
    }),
  showSidebarView: (view) =>
    set({ sidebarView: view, activeId: HOME_TAB, sidebarOpen: true }),
  setSidebarWidth: (width) => {
    set({ sidebarWidth: clampSidebarWidth(width) });
    scheduleSidebarWidthSave(get);
  },
  setNetTool: (tool) => set({ netTool: tool }),
  setDockWidth: (width) => {
    set({ dockWidth: clampDockWidth(width) });
    scheduleDockSave(get);
  },
  setDockCardHeight: (id, height) => {
    set((s) => ({
      dockHeights: { ...s.dockHeights, [id]: clampDockCardHeight(id, height) },
    }));
    scheduleDockSave(get);
  },
  setWindowChrome: (chrome) => set({ windowChrome: chrome }),
  togglePalette: () => set((s) => ({ paletteOpen: !s.paletteOpen })),
  closePalette: () => set({ paletteOpen: false }),
  openSearch: () => set({ searchOpen: true }),
  closeSearch: () => set({ searchOpen: false }),
  setSettingsOpen: (open) => set({ settingsOpen: open }),
  setShortcutsOpen: (open) => set({ shortcutsOpen: open }),
  setRecordingsOpen: (open) => set({ recordingsOpen: open }),
  requestImport: () => set({ importRequested: true }),
  clearImportRequest: () => set({ importRequested: false }),
  requestNewHost: () => set({ newHostRequested: true, activeId: HOME_TAB }),
  clearNewHostRequest: () => set({ newHostRequested: false }),
  requestNewFolder: () => set({ newFolderRequested: true, activeId: HOME_TAB }),
  clearNewFolderRequest: () => set({ newFolderRequested: false }),
  requestTags: () => set({ tagsRequested: true }),
  clearTagsRequest: () => set({ tagsRequested: false }),
  requestConnect: (id) => set({ connectRequestId: id }),
  clearConnectRequest: () => set({ connectRequestId: null }),

  openSession: (kind, title, pending, production = false, hostId) => {
    const id = crypto.randomUUID();
    set((s) => ({
      sessions: [
        ...s.sessions,
        { id, kind, title, closed: false, production, hostId, pending },
      ],
      activeId: id,
    }));
    return id;
  },

  saveLayout: () => {
    if (!isTauri) return;
    const state = get();
    // Serial devices are usually unplugged between runs, so they are not
    // restored; leaving them out keeps the saved indices pointing at tabs
    // that can actually come back.
    const live = state.sessions.filter(
      (session) => !session.closed && session.kind !== "serial",
    );
    const entries: StoredSession[] = live.map(
      ({ kind, title, hostId, production }) => ({
        kind,
        title,
        hostId,
        production,
      }),
    );
    const splitIndex = live.findIndex(
      (session) => session.id === state.splitId,
    );
    const snapshot: LayoutSnapshot = {
      version: 2,
      sessions: entries,
      activeIndex: live.findIndex((session) => session.id === state.activeId),
      split:
        splitIndex >= 0
          ? {
              index: splitIndex,
              direction: state.splitDirection,
              ratio: state.splitRatio,
            }
          : null,
      sidebar: { open: state.sidebarOpen, view: state.sidebarView },
    };
    api.setSetting("layout.snapshot", JSON.stringify(snapshot)).catch(() => {});
  },

  restoreLayout: async () => {
    if (!isTauri || get().layoutRestored) return;
    set({ layoutRestored: true });
    if (get().sessions.length > 0) return;
    let settings: Record<string, string | undefined>;
    try {
      settings = await api.getSettings();
    } catch {
      return;
    }
    const snapshot = parseLayoutSnapshot(
      settings["layout.snapshot"],
      settings.layout,
    );
    if (!snapshot) return;

    // The sidebar is restored before the sessions so the first paint already
    // shows the right module instead of flashing the default one.
    set({
      sidebarOpen: snapshot.sidebar.open,
      sidebarView: snapshot.sidebar.view,
    });
    if (snapshot.sessions.length === 0) return;

    const hosts = get().hosts;
    // A locked master-password vault cannot supply secrets yet; restoring
    // would only produce a burst of failed logins.
    const vault = await api.vaultStatus().catch(() => null);
    const secretsLocked = vault?.mode === "master" && vault.locked;
    // Sessions that fail to come back leave a null behind, so the indices in
    // the snapshot keep pointing at the right tab.
    const restored: (string | null)[] = [];
    for (const entry of snapshot.sessions) {
      if (entry.kind === "pty" && !entry.hostId) {
        restored.push(
          get().openSession(
            "pty",
            entry.title || i18n.t("session.localTitle"),
            { type: "pty" },
          ),
        );
        continue;
      }
      const host = entry.hostId
        ? hosts.find((item) => item.id === entry.hostId)
        : undefined;
      if (!host) {
        restored.push(null);
        continue;
      }
      const plan = planHostConnect(host, hosts, get().keepaliveSecs);
      if (plan.needsCredentials && secretsLocked) {
        restored.push(null);
        continue;
      }
      try {
        restored.push(
          get().openSession(
            plan.kind,
            entry.title || host.name || `${host.username}@${host.host}`,
            plan.pending,
            entry.production ?? host.production,
            host.id,
          ),
        );
      } catch {
        restored.push(null);
      }
    }

    const active = restored[snapshot.activeIndex];
    if (active) set({ activeId: active });
    if (snapshot.split) {
      const other = restored[snapshot.split.index];
      if (other && other !== active) {
        set({
          splitId: other,
          splitDirection: snapshot.split.direction,
          splitRatio: snapshot.split.ratio,
        });
      }
    }
  },

  splitActive: () => {
    const { activeId, sessions, splitId } = get();
    if (!activeId || splitId) return;
    if (!sessions.some((s) => s.id === activeId && !s.closed)) return;
    const other = sessions.find((s) => s.id !== activeId && !s.closed);
    if (other) {
      set({ splitId: other.id, splitRatio: 0.5 });
      return;
    }
    const id = crypto.randomUUID();
    set((s) => ({
      sessions: [
        ...s.sessions,
        {
          id,
          kind: "pty",
          title: i18n.t("session.localTitle"),
          closed: false,
          pending: { type: "pty" },
        },
      ],
      splitId: id,
      splitRatio: 0.5,
    }));
  },

  closeSplit: () => set({ splitId: null }),

  setFocusedSession: (id) => set({ focusedSessionId: id }),

  recordCommand: (sessionId, text) => {
    const line = text.trim();
    if (!line || isSensitiveCommand(line)) return;
    const session = get().sessions.find((entry) => entry.id === sessionId);
    const entry: HistoryEntry = {
      id: crypto.randomUUID(),
      sessionId,
      sessionTitle: session?.title ?? "",
      text: line,
      at: Date.now(),
      hostId: session?.hostId,
    };
    set((s) => ({
      commandHistory: [entry, ...s.commandHistory].slice(0, 200),
      execLog: [
        {
          id: entry.id,
          at: entry.at,
          kind: "command" as const,
          sessionTitle: entry.sessionTitle,
          text: entry.text,
        },
        ...s.execLog,
      ].slice(0, 500),
    }));
    scheduleHistorySave();
  },

  clearCommandHistory: () => {
    set({ commandHistory: [] });
    if (isTauri) api.setSetting("commandHistory", "[]").catch(() => {});
  },

  toggleExecLog: () =>
    set((s) => {
      const open = !s.execLogOpen;
      const single = s.dockSingleCard && open;
      if (isTauri) {
        api.setSetting("execLogOpen", open ? "1" : "0").catch(() => {});
        if (single) {
          api.setSetting("commandsOpen", "0").catch(() => {});
          api.setSetting("monitorOpen", "0").catch(() => {});
          api.setSetting("aiOpen", "0").catch(() => {});
        }
      }
      return {
        execLogOpen: open,
        ...(single ? { commandsOpen: false, monitorOpen: false, aiOpen: false } : {}),
      };
    }),

  toggleAi: () =>
    set((s) => {
      const open = !s.aiOpen;
      const single = s.dockSingleCard && open;
      if (isTauri) {
        api.setSetting("aiOpen", open ? "1" : "0").catch(() => {});
        if (single) {
          api.setSetting("commandsOpen", "0").catch(() => {});
          api.setSetting("monitorOpen", "0").catch(() => {});
          api.setSetting("execLogOpen", "0").catch(() => {});
        }
      }
      return {
        aiOpen: open,
        ...(single
          ? { commandsOpen: false, monitorOpen: false, execLogOpen: false }
          : {}),
      };
    }),

  clearExecLog: () => set({ execLog: [] }),

  logEvent: (text) =>
    set((s) => ({
      execLog: [
        {
          id: crypto.randomUUID(),
          at: Date.now(),
          kind: "event" as const,
          sessionTitle: "",
          text,
        },
        ...s.execLog,
      ].slice(0, 500),
    })),

  requestBroadcast: () => set({ broadcastRequested: true }),
  cancelBroadcast: () => set({ broadcastRequested: false }),
  startBroadcast: (ids) =>
    set({ broadcastTargets: ids, broadcastRequested: false }),
  stopBroadcast: () => set({ broadcastTargets: [] }),

  sendInput: (kind, id, data) => {
    const { broadcastTargets, sessions } = get();
    const writes: Promise<void>[] = [api.write(kind, id, data)];
    for (const targetId of broadcastTargets) {
      if (targetId === id) continue;
      const target = sessions.find((s) => s.id === targetId && !s.closed);
      if (target) writes.push(api.write(target.kind, targetId, data));
    }
    void Promise.allSettled(writes);
  },

  splitWith: (id, splitDirection) =>
    set((s) => {
      if (!s.activeId || id === s.activeId) return {};
      return { splitId: id, splitDirection, splitRatio: 0.5 };
    }),

  setSplitDirection: (splitDirection) => set({ splitDirection }),

  setSplitRatio: (ratio) => set({ splitRatio: Math.min(0.85, Math.max(0.15, ratio)) }),

  closeSession: (id) => {
    const entry = get().sessions.find((s) => s.id === id);
    if (!entry) return;
    api.close(entry.kind, id).catch(() => {});
    if (entry.recording) api.recordingStop(id).catch(() => {});
    set((s) => {
      const sessions = s.sessions.filter((x) => x.id !== id);
      let splitId = s.splitId === id ? null : s.splitId;
      if (splitId && !sessions.some((x) => x.id === splitId)) splitId = null;
      let activeId =
        s.activeId === id ? (sessions[sessions.length - 1]?.id ?? HOME_TAB) : s.activeId;
      if (activeId && splitId === activeId) splitId = null;
      return {
        sessions,
        activeId,
        splitId,
        broadcastTargets: s.broadcastTargets.filter(
          (targetId) => targetId !== id,
        ),
        focusedSessionId: s.focusedSessionId === id ? null : s.focusedSessionId,
      };
    });
  },

  renameSession: (id, title) =>
    set((s) => ({
      sessions: s.sessions.map((x) =>
        x.id === id ? { ...x, title: title.trim() || x.title } : x,
      ),
    })),

  closeOtherSessions: (id) => {
    const state = get();
    if (!state.sessions.some((s) => s.id === id)) return;
    state.sessions
      .filter((session) => session.id !== id)
      .forEach((session) => {
        api.close(session.kind, session.id).catch(() => {});
        if (session.recording) api.recordingStop(session.id).catch(() => {});
      });
    set((s) => ({
      sessions: s.sessions.filter((x) => x.id === id),
      activeId: id,
      splitId: null,
      broadcastTargets: s.broadcastTargets.filter((target) => target === id),
      focusedSessionId: s.focusedSessionId === id ? s.focusedSessionId : null,
    }));
  },

  closeAllSessions: () => {
    const state = get();
    state.sessions.forEach((session) => {
      api.close(session.kind, session.id).catch(() => {});
      if (session.recording) api.recordingStop(session.id).catch(() => {});
    });
    set({
      sessions: [],
      activeId: HOME_TAB,
      splitId: null,
      broadcastTargets: [],
      focusedSessionId: null,
    });
  },

  requestCloseSession: (id) => {
    const state = get();
    const entry = state.sessions.find((s) => s.id === id);
    if (!entry) return;
    if (state.confirmClose && !entry.closed) set({ pendingCloseId: id });
    else state.closeSession(id);
  },

  cancelCloseSession: () => set({ pendingCloseId: null }),

  confirmCloseSession: () => {
    const id = get().pendingCloseId;
    set({ pendingCloseId: null });
    if (id) get().closeSession(id);
  },

  activate: (id) =>
    set((s) => ({
      activeId: id,
      splitId: s.splitId === id ? null : s.splitId,
    })),

  markClosed: (id) => {
    const entry = get().sessions.find((s) => s.id === id);
    if (entry?.recording) api.recordingStop(id).catch(() => {});
    set((s) => ({
      sessions: s.sessions.map((x) =>
        x.id === id ? { ...x, closed: true, recording: false } : x,
      ),
      splitId: s.splitId === id ? null : s.splitId,
    }));
  },

  startRecording: async (id, cols, rows) => {
    if (!isTauri) return;
    const session = get().sessions.find((s) => s.id === id);
    if (!session || session.recording) return;
    try {
      const path = await api.recordingStart(id, session.title, cols, rows);
      set((s) => ({
        sessions: s.sessions.map((x) =>
          x.id === id ? { ...x, recording: true, recordingPath: path } : x,
        ),
        status: { text: i18n.t("sessions.recordingStarted", { path }), isError: false },
      }));
    } catch (err) {
      set({ status: { text: String(err), isError: true } });
    }
  },

  stopRecording: async (id) => {
    const session = get().sessions.find((s) => s.id === id);
    if (!session?.recording) return;
    try {
      const path = await api.recordingStop(id);
      set((s) => ({
        sessions: s.sessions.map((x) =>
          x.id === id ? { ...x, recording: false } : x,
        ),
        status: {
          text: i18n.t("sessions.recordingStopped", { path: path ?? "" }),
          isError: false,
        },
      }));
    } catch (err) {
      set({ status: { text: String(err), isError: true } });
    }
  },

  consumePending: (id) => {
    const pending = get().sessions.find((s) => s.id === id)?.pending;
    if (pending) {
      set((s) => ({
        sessions: s.sessions.map((x) =>
          x.id === id ? { ...x, pending: undefined, lastConnect: pending } : x,
        ),
      }));
    }
    return pending;
  },

  reconnect: (id) => {
    const state = get();
    const session = state.sessions.find((s) => s.id === id);
    if (!session?.lastConnect) return;
    state.openSession(
      session.kind,
      session.title,
      session.lastConnect,
      session.production,
      session.hostId,
    );
    state.closeSession(id);
  },

  scrubSessionSecrets: (id) => {
    set((s) => ({
      sessions: s.sessions.map((x) =>
        x.id === id
          ? {
              ...x,
              pending: stripPendingSecrets(x.pending),
              lastConnect: stripPendingSecrets(x.lastConnect),
            }
          : x,
      ),
    }));
  },

  toggleTheme: () => {
    const theme = get().theme === "light" ? "dark" : "light";
    get().setTheme(theme);
  },

  setTheme: (theme) => {
    const pack = get().pluginScan?.themes.find((item) => item.id === theme);
    const custom = get().importedThemes.find((item) => item.id === theme);
    if (pack) {
      applyThemePack(theme, pack.base, pack.tokens);
      setTerminalPalette(null);
    } else if (custom) {
      applyThemePack(theme, custom.base, custom.tokens);
      setTerminalPalette(custom.palette);
    } else {
      applyTheme(theme);
      setTerminalPalette(builtinPalette(theme));
    }
    set((s) => ({ theme, themeVersion: s.themeVersion + 1 }));
    if (isTauri) api.setSetting("theme", theme).catch(() => {});
  },

  importTheme: (scheme) => {
    const importedThemes = [
      ...get().importedThemes.filter((item) => item.id !== scheme.id),
      scheme,
    ];
    set({ importedThemes });
    if (isTauri) {
      api.setSetting("themes:imported", JSON.stringify(importedThemes)).catch(
        () => {},
      );
    }
    get().setTheme(scheme.id);
  },

  removeImportedTheme: (id) => {
    const importedThemes = get().importedThemes.filter((item) => item.id !== id);
    set({ importedThemes });
    if (isTauri) {
      api.setSetting("themes:imported", JSON.stringify(importedThemes)).catch(
        () => {},
      );
    }
    if (get().theme === id) get().setTheme("dark");
  },

  setLanguage: (lng) => {
    void i18n.changeLanguage(lng);
    set({ language: lng });
    if (isTauri) api.setSetting("language", lng).catch(() => {});
  },

  loadSettings: async () => {
    if (!isTauri) return;
    const settings = await api.getSettings();
    const theme = normalizeThemeId(settings.theme || "dark");
    const language = settings.language || get().language;
    const keepalive = Number.parseInt(settings.keepalive ?? "", 10);
    const fontSize = Number.parseInt(settings.terminalFontSize ?? "", 10);
    const scrollback = Number.parseInt(settings.terminalScrollback ?? "", 10);
    const fontFamily = settings.terminalFontFamily?.trim();
    const numbering = settings.tabNumbering;
    const tabNumbering: TabNumbering =
      numbering === "always" || numbering === "none" ? numbering : "asNeed";
    const importedThemes = parseStoredThemes(settings["themes:imported"]);
    const uiFontSize = Number.parseInt(settings.uiFontSize ?? "", 10);
    const reduceMotion = settings.reduceMotion === "1";
    const dockSingleCard = settings.dockSingleCard === "1";
    const pack = get().pluginScan?.themes.find((item) => item.id === theme);
    const custom = importedThemes.find((item) => item.id === theme);
    if (pack) {
      applyThemePack(theme, pack.base, pack.tokens);
      setTerminalPalette(null);
    } else if (custom) {
      applyThemePack(theme, custom.base, custom.tokens);
      setTerminalPalette(custom.palette);
    } else {
      applyTheme(theme);
      setTerminalPalette(builtinPalette(theme));
    }
    void i18n.changeLanguage(language);
    set({
      theme,
      themeVersion: get().themeVersion + 1,
      importedThemes,
      language,
      keepaliveSecs: Number.isFinite(keepalive) ? keepalive : get().keepaliveSecs,
      autoLockMinutes: [0, 5, 15, 30, 60].includes(
        Number.parseInt(settings["vault:autoLock"] ?? "", 10),
      )
        ? Number.parseInt(settings["vault:autoLock"] ?? "", 10)
        : get().autoLockMinutes,
      terminalFontFamily: fontFamily || DEFAULT_TERMINAL_FONT_FAMILY,
      terminalFontSize: Number.isFinite(fontSize)
        ? Math.min(32, Math.max(8, fontSize))
        : get().terminalFontSize,
      terminalScrollback: Number.isFinite(scrollback)
        ? Math.min(50000, Math.max(200, scrollback))
        : get().terminalScrollback,
      terminalCursorBlink: (settings.terminalCursorBlink ?? "1") !== "0",
      terminalCursorStyle:
        settings.terminalCursorStyle === "underline" ||
        settings.terminalCursorStyle === "bar"
          ? settings.terminalCursorStyle
          : "block",
      terminalBell:
        settings.terminalBell === "visual" ||
        settings.terminalBell === "sound"
          ? settings.terminalBell
          : "off",
      terminalRightClickPaste: (settings.terminalRightClickPaste ?? "0") === "1",
      terminalScrollOnOutput: (settings.terminalScrollOnOutput ?? "1") !== "0",
      terminalAutoCopy: (settings.terminalAutoCopy ?? "0") === "1",
      sshSecurity: parseSshSecurity(settings.sshSecurity),
      osc52: (settings.osc52 ?? "1") !== "0",
      keywordHighlights: parseKeywordRules(settings.keywordHighlights),
      tabNumbering,
      restoreLayoutOnStartup: (settings.restoreLayoutOnStartup ?? "1") !== "0",
      confirmClose: (settings.confirmClose ?? "1") !== "0",
      sftpShowHidden: settings.sftpShowHidden === "1",
      editorAutoSave: settings.editorAutoSave === "1",
      commandHistory: parseHistory(settings.commandHistory),
      shortcuts: parseShortcuts(settings.shortcuts),
      sidebarOpen: (settings.sidebarOpen ?? "1") !== "0",
      sidebarWidth: clampSidebarWidth(Number(settings.sidebarWidth)),
      dockWidth: clampDockWidth(Number(settings.dockWidth)),
      dockHeights: parseDockHeights(settings.dockHeights),
      // Older builds had no persisted panel state; keep their defaults.
      commandsOpen: (settings.commandsOpen ?? "1") !== "0",
      monitorOpen: (settings.monitorOpen ?? "0") === "1",
      // Session ids do not survive a restart, so the readout starts on this
      // machine again; only the on/off choice is persisted.
      monitorTarget: "local",
      statusMonitor: (settings.statusMonitor ?? "1") !== "0",
      uiFontSize: Number.isFinite(uiFontSize)
        ? Math.min(16, Math.max(11, uiFontSize))
        : get().uiFontSize,
      reduceMotion,
      dockSingleCard,
      execLogOpen: (settings.execLogOpen ?? "0") === "1",
      aiOpen: (settings.aiOpen ?? "0") === "1",
      transferConcurrency: clampTransferConcurrency(
        Number(settings.transferConcurrency),
      ),
      transferTabs: parseIdList(settings.transferTabs),
      syncPolicy: parseSyncPolicy(settings["sync:policy"]),
      syncInterval: parseSyncInterval(settings["sync:interval"]),
      syncScope: parseSyncScope(settings["sync:scope"]),
      syncLastAt: Number(settings["sync:lastAt"]) || 0,
    });
    if (isTauri) {
      // The queue starts at its built-in default, so push the saved value.
      api.setTransferConcurrency(get().transferConcurrency).catch(() => {});
      // Retire the duplicate theme id from storage too, so exports and later
      // loads do not keep carrying a value no bundled theme resolves to.
      if (settings.theme && theme !== settings.theme) {
        api.setSetting("theme", theme).catch(() => {});
      }
    }
    // The stored type scale and motion preference must reach the DOM even when
    // they equal the defaults, because a previous session may have changed them.
    applyUiFont(get().uiFontSize);
    applyMotion(reduceMotion);
  },

  loadLanguagePacks: async () => {
    if (!isTauri) return;
    const scan = await api.pluginScan();
    scan.packs.forEach((pack) => registerLanguagePack(pack.locale, pack.strings));
    set({
      pluginScan: scan,
      languagePacks: scan.packs.map(({ id, name, locale }) => ({
        id,
        name,
        locale,
      })),
    });
    const current = get().theme;
    const pack = scan.themes.find((item) => item.id === current);
    const custom = get().importedThemes.find((item) => item.id === current);
    if (pack) {
      applyThemePack(current, pack.base, pack.tokens);
      setTerminalPalette(null);
    } else if (custom) {
      applyThemePack(current, custom.base, custom.tokens);
      setTerminalPalette(custom.palette);
    } else {
      setTerminalPalette(builtinPalette(current));
    }
  },

  bumpCommandLibrary: () =>
    set((s) => ({ commandLibraryVersion: s.commandLibraryVersion + 1 })),
  bumpAiConfig: () =>
    set((s) => ({ aiConfigVersion: s.aiConfigVersion + 1 })),

  setStatus: (text, isError = false) => {
    set({ status: { text, isError } });
    window.clearTimeout(statusTimer);
    if (!text) return;
    // A message should not sit in the status bar forever; errors stay long
    // enough to be read, plain confirmations go quickly.
    statusTimer = window.setTimeout(
      () => set({ status: { text: "", isError: false } }),
      isError ? STATUS_ERROR_MS : STATUS_INFO_MS,
    );
  },

  clearStatus: () => {
    window.clearTimeout(statusTimer);
    set({ status: { text: "", isError: false } });
  },

  connectRetry: null,
  reportConnectError: (hostId, error) =>
    set({ connectRetry: { hostId, error } }),
  clearConnectRetry: () => set({ connectRetry: null }),

  askConfirm: (request) =>
    new Promise<boolean>((resolve) => {
      set({
        appPrompt: {
          request: { ...request, kind: "confirm" },
          settle: (value) => resolve(value === true),
        },
      });
    }),

  askText: (request) =>
    new Promise<string | null>((resolve) => {
      set({
        appPrompt: {
          request: { ...request, kind: "text", value: request.value ?? "" },
          settle: (value) => resolve(typeof value === "string" ? value : null),
        },
      });
    }),

  askApprove: (request) =>
    new Promise<ApprovalChoice | null>((resolve) => {
      set({
        appPrompt: {
          request: { ...request, kind: "approve" },
          settle: (value) =>
            resolve(
              value === "once" || value === "session" || value === "always"
                ? value
                : null,
            ),
        },
      });
    }),

  settlePrompt: (value) => {
    const pending = get().appPrompt;
    if (!pending) return;
    set({ appPrompt: null });
    pending.settle(value);
  },

  loadHosts: async () => {
    if (!isTauri) return;
    const hosts = await api.listHosts();
    set((s) => ({
      hosts,
      selectedHostId:
        s.selectedHostId && hosts.some((h) => h.id === s.selectedHostId)
          ? s.selectedHostId
          : null,
    }));
  },

  loadTags: async () => {
    if (!isTauri) return;
    set({ tags: await api.listTags() });
  },

  createHost: async (input) => {
    await api.createHost(input);
    set({ hosts: await api.listHosts(), selectedHostId: input.id });
  },

  updateHost: async (input) => {
    await api.updateHost(input);
    set({ hosts: await api.listHosts() });
  },

  deleteHost: async (id) => {
    await api.deleteHost(id);
    const hosts = await api.listHosts();
    set((s) => ({
      hosts,
      selectedHostId: s.selectedHostId === id ? null : s.selectedHostId,
      connectRetry: s.connectRetry?.hostId === id ? null : s.connectRetry,
      // A deleted host must not keep a failed password alive on its tabs; the
      // retry path would otherwise keep resending a credential that is gone.
      sessions: s.sessions.map((x) =>
        x.hostId === id
          ? {
              ...x,
              pending: stripPendingSecrets(x.pending),
              lastConnect: stripPendingSecrets(x.lastConnect),
            }
          : x,
      ),
    }));
  },

  createTag: async (tag) => {
    await api.createTag(tag);
    set({ tags: await api.listTags() });
  },

  updateTag: async (tag) => {
    await api.updateTag(tag);
    const [tags, hosts] = await Promise.all([api.listTags(), api.listHosts()]);
    set({ tags, hosts });
  },

  deleteTag: async (id) => {
    await api.deleteTag(id);
    const [tags, hosts] = await Promise.all([api.listTags(), api.listHosts()]);
    set({ tags, hosts });
  },

  setHostFilter: (text) => set({ hostFilter: text }),

  toggleFolder: (id) =>
    set((s) => ({
      collapsedFolders: { ...s.collapsedFolders, [id]: !s.collapsedFolders[id] },
    })),

  selectHost: (id) => set({ selectedHostId: id }),

  clearNewWindowHost: () => set({ newWindowHostId: null }),
}));
