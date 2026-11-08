import { create } from "zustand";
import { api, type SessionKind, type SshConnectArgs } from "./api";
import { applyTheme, type Theme } from "./theme";
import i18n, { detectLanguage, type Language } from "./i18n";

export type PendingConnect =
  | { type: "ssh"; args: Omit<SshConnectArgs, "cols" | "rows"> }
  | { type: "pty" };

export interface SessionMeta {
  id: string;
  kind: SessionKind;
  title: string;
  closed: boolean;
  pending?: PendingConnect;
}

interface Status {
  text: string;
  isError: boolean;
}

interface AppState {
  sessions: SessionMeta[];
  activeId: string | null;
  theme: Theme;
  language: Language;
  status: Status;
  openSession: (kind: SessionKind, title: string, pending: PendingConnect) => string;
  closeSession: (id: string) => void;
  activate: (id: string) => void;
  markClosed: (id: string) => void;
  consumePending: (id: string) => PendingConnect | undefined;
  toggleTheme: () => void;
  setLanguage: (lng: Language) => void;
  setStatus: (text: string, isError?: boolean) => void;
}

export const useAppStore = create<AppState>((set, get) => ({
  sessions: [],
  activeId: null,
  theme: "dark",
  language: detectLanguage(),
  status: { text: "", isError: false },

  openSession: (kind, title, pending) => {
    const id = crypto.randomUUID();
    set((s) => ({
      sessions: [...s.sessions, { id, kind, title, closed: false, pending }],
      activeId: id,
    }));
    return id;
  },

  closeSession: (id) => {
    const entry = get().sessions.find((s) => s.id === id);
    if (!entry) return;
    api.close(entry.kind, id).catch(() => {});
    set((s) => {
      const sessions = s.sessions.filter((x) => x.id !== id);
      const activeId =
        s.activeId === id ? (sessions[sessions.length - 1]?.id ?? null) : s.activeId;
      return { sessions, activeId };
    });
  },

  activate: (id) => set({ activeId: id }),

  markClosed: (id) =>
    set((s) => ({
      sessions: s.sessions.map((x) => (x.id === id ? { ...x, closed: true } : x)),
    })),

  consumePending: (id) => {
    const pending = get().sessions.find((s) => s.id === id)?.pending;
    if (pending) {
      set((s) => ({
        sessions: s.sessions.map((x) =>
          x.id === id ? { ...x, pending: undefined } : x,
        ),
      }));
    }
    return pending;
  },

  toggleTheme: () => {
    const theme = get().theme === "dark" ? "light" : "dark";
    applyTheme(theme);
    set({ theme });
  },

  setLanguage: (lng) => {
    void i18n.changeLanguage(lng);
    set({ language: lng });
  },

  setStatus: (text, isError = false) => set({ status: { text, isError } }),
}));