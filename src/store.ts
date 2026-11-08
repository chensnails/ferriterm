import { create } from "zustand";
import {
  api,
  isTauri,
  type HostEntry,
  type HostInput,
  type SessionKind,
  type SshConnectArgs,
  type Tag,
  type TransferJob,
} from "./api";
import { buildConnectArgs } from "./connect";
import { isSensitiveCommand } from "./commands";
import { applyTheme, type Theme } from "./theme";
import i18n, {
  detectLanguage,
  registerLanguagePack,
  type Language,
} from "./i18n";

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
}

export interface ExecLogEntry {
  id: string;
  at: number;
  kind: "command" | "event";
  sessionTitle: string;
  text: string;
}

export type PendingConnect =
  | { type: "ssh"; args: Omit<SshConnectArgs, "cols" | "rows"> }
  | { type: "pty" };

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

interface Status {
  text: string;
  isError: boolean;
}

export type SplitDirection = "horizontal" | "vertical";

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
  language: Language;
  languagePacks: LanguageOption[];
  status: Status;
  hosts: HostEntry[];
  tags: Tag[];
  hostFilter: string;
  collapsedFolders: Record<string, boolean>;
  selectedHostId: string | null;
  newWindowHostId: string | null;
  monitorOpen: boolean;
  toggleMonitor: () => void;
  forwardOpen: boolean;
  setForwardOpen: (open: boolean) => void;
  toggleForward: () => void;
  transferOpen: boolean;
  transferSessionId: string | null;
  openTransfer: (sessionId?: string | null) => void;
  closeTransfer: () => void;
  setTransferSession: (id: string | null) => void;
  transfers: TransferJob[];
  upsertTransfer: (job: TransferJob) => void;
  setTransfers: (jobs: TransferJob[]) => void;
  keepaliveSecs: number;
  setKeepalive: (seconds: number) => void;
  commandsOpen: boolean;
  toggleCommands: () => void;
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
  importRequested: boolean;
  requestImport: () => void;
  clearImportRequest: () => void;
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
  activate: (id: string) => void;
  markClosed: (id: string) => void;
  startRecording: (id: string, cols: number, rows: number) => Promise<void>;
  stopRecording: (id: string) => Promise<void>;
  consumePending: (id: string) => PendingConnect | undefined;
  reconnect: (id: string) => void;
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
  activeId: null,
  splitId: null,
  splitDirection: "horizontal",
  splitRatio: 0.5,
  broadcastTargets: [],
  broadcastRequested: false,
  focusedSessionId: null,
  commandHistory: [],
  execLog: [],
  execLogOpen: false,
  layoutRestored: false,
  theme: "dark",
  language: detectLanguage(),
  languagePacks: [],
  status: { text: "", isError: false },
  hosts: [],
  tags: [],
  hostFilter: "",
  collapsedFolders: {},
  selectedHostId: null,
  newWindowHostId: null,
  monitorOpen: false,
  forwardOpen: false,
  transferOpen: false,
  transferSessionId: null,
  transfers: [],
  keepaliveSecs: 30,
  commandsOpen: true,
  paletteOpen: false,
  searchOpen: false,
  settingsOpen: false,
  shortcutsOpen: false,
  importRequested: false,
  tagsRequested: false,
  connectRequestId: null,

  toggleMonitor: () => set((s) => ({ monitorOpen: !s.monitorOpen })),
  setForwardOpen: (open) => set({ forwardOpen: open }),
  toggleForward: () => set((s) => ({ forwardOpen: !s.forwardOpen })),
  openTransfer: (sessionId) =>
    set((s) => ({
      transferOpen: true,
      transferSessionId:
        sessionId !== undefined ? sessionId : s.transferSessionId,
    })),
  closeTransfer: () => set({ transferOpen: false }),
  setTransferSession: (id) => set({ transferSessionId: id }),
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
  toggleCommands: () => set((s) => ({ commandsOpen: !s.commandsOpen })),
  togglePalette: () => set((s) => ({ paletteOpen: !s.paletteOpen })),
  closePalette: () => set({ paletteOpen: false }),
  openSearch: () => set({ searchOpen: true }),
  closeSearch: () => set({ searchOpen: false }),
  setSettingsOpen: (open) => set({ settingsOpen: open }),
  setShortcutsOpen: (open) => set({ shortcutsOpen: open }),
  requestImport: () => set({ importRequested: true }),
  clearImportRequest: () => set({ importRequested: false }),
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
    const entries: StoredSession[] = get()
      .sessions.filter((session) => !session.closed)
      .map(({ kind, title, hostId, production }) => ({
        kind,
        title,
        hostId,
        production,
      }));
    api.setSetting("layout", JSON.stringify(entries)).catch(() => {});
  },

  restoreLayout: async () => {
    if (!isTauri || get().layoutRestored) return;
    set({ layoutRestored: true });
    if (get().sessions.length > 0) return;
    let raw = "";
    try {
      raw = (await api.getSettings()).layout ?? "";
    } catch {
      return;
    }
    if (!raw) return;
    let entries: StoredSession[];
    try {
      const parsed = JSON.parse(raw);
      if (!Array.isArray(parsed)) return;
      entries = parsed.slice(0, 20);
    } catch {
      return;
    }
    const hosts = get().hosts;
    for (const entry of entries) {
      if (entry.kind === "pty") {
        get().openSession(
          "pty",
          entry.title || i18n.t("session.localTitle"),
          { type: "pty" },
        );
        continue;
      }
      if (entry.kind !== "ssh" || !entry.hostId) continue;
      const host = hosts.find((item) => item.id === entry.hostId);
      if (!host) continue;
      try {
        const hasPrompt =
          host.authMethod === "keyboard-interactive" ||
          host.authMethod === "agent";
        const hasKey = host.authMethod === "key" && host.keyPath;
        const secret = hasPrompt
          ? null
          : await api.getCredential(host.id).catch(() => null);
        if (!hasPrompt && !hasKey && !secret) continue;
        const args = await buildConnectArgs(
          host,
          hosts,
          {
            password: hasKey ? undefined : (secret ?? undefined),
            passphrase: hasKey ? (secret ?? undefined) : undefined,
            keyPath: host.keyPath,
          },
          get().keepaliveSecs,
        );
        get().openSession(
          "ssh",
          entry.title || host.name || `${host.username}@${host.host}`,
          { type: "ssh", args },
          entry.production ?? host.production,
          host.id,
        );
      } catch {
        continue;
      }
    }
  },

  splitActive: () => {
    const { activeId, sessions, splitId } = get();
    if (!activeId || splitId) return;
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

  toggleExecLog: () => set((s) => ({ execLogOpen: !s.execLogOpen })),

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
        s.activeId === id ? (sessions[sessions.length - 1]?.id ?? null) : s.activeId;
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

  toggleTheme: () => {
    const theme = get().theme === "dark" ? "light" : "dark";
    applyTheme(theme);
    set({ theme });
    if (isTauri) api.setSetting("theme", theme).catch(() => {});
  },

  setTheme: (theme) => {
    applyTheme(theme);
    set({ theme });
    if (isTauri) api.setSetting("theme", theme).catch(() => {});
  },

  setLanguage: (lng) => {
    void i18n.changeLanguage(lng);
    set({ language: lng });
    if (isTauri) api.setSetting("language", lng).catch(() => {});
  },

  loadSettings: async () => {
    if (!isTauri) return;
    const settings = await api.getSettings();
    const theme = settings.theme === "light" ? "light" : "dark";
    const language = settings.language || get().language;
    const keepalive = Number.parseInt(settings.keepalive ?? "", 10);
    applyTheme(theme);
    void i18n.changeLanguage(language);
    set({
      theme,
      language,
      keepaliveSecs: Number.isFinite(keepalive) ? keepalive : get().keepaliveSecs,
      commandHistory: parseHistory(settings.commandHistory),
    });
  },

  loadLanguagePacks: async () => {
    if (!isTauri) return;
    const scan = await api.listLanguagePacks();
    scan.packs.forEach((pack) => registerLanguagePack(pack.locale, pack.strings));
    set({
      languagePacks: scan.packs.map(({ id, name, locale }) => ({
        id,
        name,
        locale,
      })),
    });
  },

  setStatus: (text, isError = false) => set({ status: { text, isError } }),

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
