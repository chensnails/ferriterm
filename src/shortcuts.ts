import i18n from "./i18n";
import { useAppStore } from "./store";

function isFormField(target: EventTarget | null): boolean {
  const element = target as HTMLElement | null;
  if (!element) return false;
  if (element.closest(".xterm")) return false;
  const tag = element.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || element.isContentEditable;
}

export function installShortcuts(): () => void {
  const onKey = (event: KeyboardEvent) => {
    if (!event.metaKey && !event.ctrlKey) return;
    if (event.altKey) return;
    const store = useAppStore.getState();
    const key = event.key.toLowerCase();

    if (key === "f" && !event.shiftKey) {
      if (store.activeId) {
        event.preventDefault();
        store.openSearch();
      }
      return;
    }

    if (event.key === "/") {
      event.preventDefault();
      store.setShortcutsOpen(true);
      return;
    }

    if (isFormField(event.target)) return;

    if (key === "d" && event.shiftKey) {
      event.preventDefault();
      if (store.splitId) store.closeSplit();
      else store.splitActive();
      return;
    }

    if (key === "t" && !event.shiftKey) {
      event.preventDefault();
      store.openSession("pty", i18n.t("session.localTitle"), { type: "pty" });
      return;
    }

    if (key === "w" && !event.shiftKey) {
      if (store.activeId) {
        event.preventDefault();
        store.closeSession(store.activeId);
      }
      return;
    }

    if (key === "," && !event.shiftKey) {
      event.preventDefault();
      store.setSettingsOpen(true);
      return;
    }

    if (event.key === "Tab") {
      if (store.sessions.length < 2) return;
      event.preventDefault();
      const current = store.sessions.findIndex((s) => s.id === store.activeId);
      const step = event.shiftKey ? -1 : 1;
      const next =
        (current + step + store.sessions.length) % store.sessions.length;
      store.activate(store.sessions[next].id);
    }
  };
  window.addEventListener("keydown", onKey);
  return () => window.removeEventListener("keydown", onKey);
}
