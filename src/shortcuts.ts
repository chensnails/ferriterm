import i18n from "./i18n";
import { comboFromEvent, SHORTCUT_ACTIONS, type ShortcutAction } from "./keybindings";
import { HOME_TAB, TRANSFER_TAB, useAppStore } from "./store";

const FORM_ALLOWED: ShortcutAction[] = ["palette", "search", "help"];

function isFormField(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const element = target;
  if (element.closest(".xterm")) return false;
  const tag = element.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || element.isContentEditable;
}

function cycleSession(step: number) {
  const store = useAppStore.getState();
  if (store.sessions.length < 2) return;
  const current = store.sessions.findIndex((s) => s.id === store.activeId);
  const next =
    (current + step + store.sessions.length) % store.sessions.length;
  store.activate(store.sessions[next].id);
}

function runAction(action: ShortcutAction): boolean {
  const store = useAppStore.getState();
  switch (action) {
    case "palette":
      store.togglePalette();
      return true;
    case "search":
      if (!store.sessions.some((s) => s.id === store.activeId)) return false;
      store.openSearch();
      return true;
    case "newTab":
      store.openSession("pty", i18n.t("session.localTitle"), { type: "pty" });
      return true;
    case "closeTab":
      if (store.activeId === TRANSFER_TAB) {
        store.closeTransfer();
        return true;
      }
      if (!store.activeId || store.activeId === HOME_TAB) return false;
      store.requestCloseSession(store.activeId);
      return true;
    case "nextTab":
      if (store.sessions.length < 2) return false;
      cycleSession(1);
      return true;
    case "prevTab":
      if (store.sessions.length < 2) return false;
      cycleSession(-1);
      return true;
    case "split":
      if (store.splitId) store.closeSplit();
      else store.splitActive();
      return true;
    case "settings":
      store.setSettingsOpen(true);
      return true;
    case "help":
      store.setShortcutsOpen(true);
      return true;
    case "log":
      store.toggleExecLog();
      return true;
  }
}

export function installShortcuts(): () => void {
  const onKey = (event: KeyboardEvent) => {
    if (event.isComposing) return;
    const combo = comboFromEvent(event);
    if (!combo) return;
    const bindings = useAppStore.getState().shortcuts;
    const action = SHORTCUT_ACTIONS.find(
      (id) => bindings[id] && bindings[id] === combo,
    );
    if (!action) return;
    if (isFormField(event.target) && !FORM_ALLOWED.includes(action)) return;
    if (runAction(action)) event.preventDefault();
  };
  window.addEventListener("keydown", onKey);
  return () => window.removeEventListener("keydown", onKey);
}
