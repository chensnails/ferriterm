export const SHORTCUT_ACTIONS = [
  "palette",
  "search",
  "newTab",
  "closeTab",
  "nextTab",
  "prevTab",
  "split",
  "transfer",
  "settings",
  "help",
  "log",
] as const;

export type ShortcutAction = (typeof SHORTCUT_ACTIONS)[number];

export type ShortcutBindings = Record<ShortcutAction, string>;

// Keys are stored canonically (lowercase single characters) so bindings
// captured from events compare equal without extra normalisation.
export const SHORTCUT_DEFAULTS: ShortcutBindings = {
  palette: "Mod+k",
  search: "Mod+f",
  newTab: "Mod+t",
  closeTab: "Mod+w",
  nextTab: "Mod+tab",
  prevTab: "Mod+Shift+tab",
  split: "Mod+Shift+d",
  transfer: "Mod+Shift+t",
  settings: "Mod+,",
  help: "Mod+/",
  log: "Mod+`",
};

export interface Combo {
  mod: boolean;
  alt: boolean;
  shift: boolean;
  key: string;
}

const MODIFIERS = new Set([
  "shift",
  "control",
  "ctrl",
  "meta",
  "cmd",
  "command",
  "alt",
  "option",
  "altgraph",
  "capslock",
]);

const NAMED_KEYS = new Set([
  "tab",
  "enter",
  "backspace",
  "delete",
  "escape",
  "space",
  "home",
  "end",
  "pageup",
  "pagedown",
  "insert",
  "arrowup",
  "arrowdown",
  "arrowleft",
  "arrowright",
]);

function normalizeKey(raw: string): string | null {
  const key = raw.trim();
  if (!key) return null;
  if (key === " ") return "space";
  const lower = key.toLowerCase();
  if (lower === "esc") return "escape";
  if (key.length === 1) return lower;
  if (NAMED_KEYS.has(lower)) return lower;
  if (/^f([1-9]|1[0-9]|2[0-4])$/.test(lower)) return lower;
  return null;
}

export function parseCombo(combo: string): Combo | null {
  if (!combo) return null;
  const parts = combo.split("+").map((part) => part.trim());
  const parsed: Combo = { mod: false, alt: false, shift: false, key: "" };
  for (const part of parts) {
    if (!part) continue;
    const lower = part.toLowerCase();
    if (lower === "mod" || lower === "ctrl" || lower === "cmd" || lower === "meta") {
      parsed.mod = true;
    } else if (lower === "alt" || lower === "option") {
      parsed.alt = true;
    } else if (lower === "shift") {
      parsed.shift = true;
    } else {
      const key = normalizeKey(part);
      if (key) parsed.key = key;
    }
  }
  // A bare key would swallow terminal typing; require a real modifier.
  if (!parsed.key || (!parsed.mod && !parsed.alt)) return null;
  return parsed;
}

export function normalizeCombo(combo: string): string | null {
  const parsed = parseCombo(combo);
  if (!parsed) return null;
  const parts: string[] = [];
  if (parsed.mod) parts.push("Mod");
  if (parsed.alt) parts.push("Alt");
  if (parsed.shift) parts.push("Shift");
  parts.push(parsed.key);
  return parts.join("+");
}

export function comboFromEvent(event: KeyboardEvent): string | null {
  if (MODIFIERS.has(event.key.toLowerCase())) return null;
  const mod = event.metaKey || event.ctrlKey;
  if (!mod && !event.altKey) return null;
  const key = normalizeKey(event.key);
  if (!key) return null;
  const parts: string[] = [];
  if (mod) parts.push("Mod");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");
  parts.push(key);
  return parts.join("+");
}

function keyLabel(key: string): string {
  if (key.length === 1) return key.toUpperCase();
  if (key.startsWith("arrow")) {
    return key.replace("arrow", "Arrow ");
  }
  if (/^f\d+$/.test(key)) return key.toUpperCase();
  return key.charAt(0).toUpperCase() + key.slice(1);
}

export function displayCombo(combo: string): string {
  const parsed = parseCombo(combo);
  if (!parsed) return "—";
  const parts: string[] = [];
  if (parsed.mod) parts.push("Ctrl/⌘");
  if (parsed.alt) parts.push("Alt");
  if (parsed.shift) parts.push("Shift");
  parts.push(keyLabel(parsed.key));
  return parts.join(" ");
}

export function findConflict(
  bindings: Record<string, string>,
  action: string,
  combo: string,
): ShortcutAction | null {
  const target = normalizeCombo(combo);
  if (!target) return null;
  for (const [id, value] of Object.entries(bindings)) {
    if (id === action) continue;
    if (value && normalizeCombo(value) === target) return id as ShortcutAction;
  }
  return null;
}

export function parseShortcuts(raw: string | undefined): ShortcutBindings {
  const bindings: ShortcutBindings = { ...SHORTCUT_DEFAULTS };
  if (!raw) return bindings;
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return bindings;
  }
  if (!parsed || typeof parsed !== "object") return bindings;
  for (const action of SHORTCUT_ACTIONS) {
    const value = (parsed as Record<string, unknown>)[action];
    if (typeof value !== "string") continue;
    if (value === "") {
      bindings[action] = "";
      continue;
    }
    const normalized = normalizeCombo(value);
    if (normalized) bindings[action] = normalized;
  }
  return bindings;
}
