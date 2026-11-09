import type { ITheme } from "@xterm/xterm";

function token(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

// Complete ANSI palette imported from iTerm2 or a Ferriterm TOML scheme. Built-in
// themes only set the four base colors so xterm keeps its default ANSI table.
export interface TerminalPalette {
  background: string;
  foreground: string;
  cursor: string;
  selection: string;
  black: string;
  red: string;
  green: string;
  yellow: string;
  blue: string;
  magenta: string;
  cyan: string;
  white: string;
  brightBlack: string;
  brightRed: string;
  brightGreen: string;
  brightYellow: string;
  brightBlue: string;
  brightMagenta: string;
  brightCyan: string;
  brightWhite: string;
}

let activePalette: TerminalPalette | null = null;

export function setTerminalPalette(palette: TerminalPalette | null) {
  activePalette = palette;
}

// xterm needs concrete colors; read them from the design tokens so the terminal
// palette follows the active theme instead of hardcoding values.
export function terminalTheme(): ITheme {
  const base: ITheme = {
    background: token("--bg-base"),
    foreground: token("--fg-primary"),
    cursor: token("--accent"),
    selectionBackground: token("--selection"),
  };
  if (!activePalette) return base;
  const { selection, ...ansi } = activePalette;
  return { ...base, ...ansi, selectionBackground: selection };
}

export type Theme = string;
export type ThemeBase = "dark" | "light";

export interface BuiltinTheme {
  id: Theme;
  base: ThemeBase;
}

export const BUILTIN_THEMES: BuiltinTheme[] = [
  { id: "dark", base: "dark" },
  { id: "light", base: "light" },
  { id: "nord", base: "dark" },
  { id: "dracula", base: "dark" },
  { id: "solarized", base: "light" },
];

const TOKEN_NAME = /^--[a-z0-9-]+$/i;
const TOKEN_VALUE = /^[#a-z0-9(),.%\s+-]+$/i;

let appliedPackTokens: string[] = [];

export function builtinTheme(id: Theme): BuiltinTheme | undefined {
  return BUILTIN_THEMES.find((item) => item.id === id);
}

function baseOf(id: Theme): ThemeBase {
  return builtinTheme(id)?.base ?? "dark";
}

// Theme packs ship untrusted token values, so only plain color-like values are
// allowed through to the CSS custom properties.
function sanitizeToken(name: string, value: string): boolean {
  if (!TOKEN_NAME.test(name)) return false;
  const trimmed = value.trim();
  if (!trimmed || trimmed.length > 128) return false;
  if (/url\s*\(|expression\s*\(|javascript:/i.test(trimmed)) return false;
  return TOKEN_VALUE.test(trimmed);
}

export function applyTheme(
  theme: Theme,
  tokens?: Record<string, string> | null,
) {
  const root = document.documentElement;
  for (const name of appliedPackTokens) root.style.removeProperty(name);
  appliedPackTokens = [];
  const base = baseOf(theme);
  root.dataset.theme =
    tokens && Object.keys(tokens).length > 0 ? base : theme;
  if (!tokens) return;
  for (const [name, value] of Object.entries(tokens)) {
    if (sanitizeToken(name, value)) {
      root.style.setProperty(name, value.trim());
      appliedPackTokens.push(name);
    }
  }
}

export function applyThemePack(
  id: Theme,
  base: string | undefined,
  tokens: Record<string, string> | null | undefined,
) {
  applyTheme(base === "light" ? "light" : base === "dark" ? "dark" : id, tokens);
}
