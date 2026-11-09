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

// Tag tokens offered for keyword highlight rules; resolved at render time so
// highlights follow the active theme.
export const HIGHLIGHT_COLORS = [
  "--tag-red",
  "--tag-amber",
  "--tag-green",
  "--tag-blue",
  "--tag-purple",
  "--tag-cyan",
];

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
  { id: "solarized-dark", base: "dark" },
  { id: "github-light", base: "light" },
  { id: "tokyo-night", base: "dark" },
  { id: "gruvbox", base: "dark" },
  { id: "one-dark", base: "dark" },
  { id: "catppuccin", base: "dark" },
  { id: "rose-pine", base: "dark" },
];

function palette(
  background: string,
  foreground: string,
  cursor: string,
  selection: string,
  colors: [
    string,
    string,
    string,
    string,
    string,
    string,
    string,
    string,
    string,
    string,
    string,
    string,
    string,
    string,
    string,
    string,
  ],
): TerminalPalette {
  return {
    background,
    foreground,
    cursor,
    selection,
    black: colors[0],
    red: colors[1],
    green: colors[2],
    yellow: colors[3],
    blue: colors[4],
    magenta: colors[5],
    cyan: colors[6],
    white: colors[7],
    brightBlack: colors[8],
    brightRed: colors[9],
    brightGreen: colors[10],
    brightYellow: colors[11],
    brightBlue: colors[12],
    brightMagenta: colors[13],
    brightCyan: colors[14],
    brightWhite: colors[15],
  };
}

// Full ANSI palettes for every built-in theme except the neutral dark/light
// baseline, which keeps xterm's stock ANSI table.
export const BUILTIN_PALETTES: Record<string, TerminalPalette> = {
  nord: palette("#2e3440", "#d8dee9", "#88c0d0", "#88c0d055", [
    "#3b4252",
    "#bf616a",
    "#a3be8c",
    "#ebcb8b",
    "#81a1c1",
    "#b48ead",
    "#88c0d0",
    "#e5e9f0",
    "#4c566a",
    "#bf616a",
    "#a3be8c",
    "#ebcb8b",
    "#81a1c1",
    "#b48ead",
    "#8fbcbb",
    "#eceff4",
  ]),
  dracula: palette("#282a36", "#f8f8f2", "#bd93f9", "#44475a88", [
    "#21222c",
    "#ff5555",
    "#50fa7b",
    "#f1fa8c",
    "#bd93f9",
    "#ff79c6",
    "#8be9fd",
    "#f8f8f2",
    "#6272a4",
    "#ff6e6e",
    "#69ff94",
    "#ffffa5",
    "#d6acff",
    "#ff92df",
    "#a4ffff",
    "#ffffff",
  ]),
  solarized: palette("#fdf6e3", "#657b83", "#268bd2", "#b3d4e088", [
    "#073642",
    "#dc322f",
    "#859900",
    "#b58900",
    "#268bd2",
    "#d33682",
    "#2aa198",
    "#eee8d5",
    "#002b36",
    "#cb4b16",
    "#586e75",
    "#657b83",
    "#839496",
    "#6c71c4",
    "#93a1a1",
    "#fdf6e3",
  ]),
  "solarized-dark": palette("#002b36", "#93a1a1", "#93a1a1", "#1f5a6699", [
    "#073642",
    "#dc322f",
    "#859900",
    "#b58900",
    "#268bd2",
    "#d33682",
    "#2aa198",
    "#eee8d5",
    "#002b36",
    "#cb4b16",
    "#586e75",
    "#657b83",
    "#839496",
    "#6c71c4",
    "#93a1a1",
    "#fdf6e3",
  ]),
  "github-light": palette("#ffffff", "#1f2328", "#0969da", "#b6d7ff99", [
    "#24292f",
    "#cf222e",
    "#116329",
    "#4d2d00",
    "#0969da",
    "#8250df",
    "#1b7c83",
    "#6e7781",
    "#57606a",
    "#a40e26",
    "#1a7f37",
    "#633c01",
    "#218bff",
    "#a475f9",
    "#3192aa",
    "#8c959f",
  ]),
  "tokyo-night": palette("#1a1b26", "#c0caf5", "#c0caf5", "#33467c99", [
    "#15161e",
    "#f7768e",
    "#9ece6a",
    "#e0af68",
    "#7aa2f7",
    "#bb9af7",
    "#7dcfff",
    "#a9b1d6",
    "#414868",
    "#ff7a93",
    "#b9f27c",
    "#ff9e64",
    "#7da6ff",
    "#bb9af7",
    "#0db9d7",
    "#c0caf5",
  ]),
  gruvbox: palette("#282828", "#ebdbb2", "#ebdbb2", "#665c5488", [
    "#282828",
    "#cc241d",
    "#98971a",
    "#d79921",
    "#458588",
    "#b16286",
    "#689d6a",
    "#a89984",
    "#928374",
    "#fb4934",
    "#b8bb26",
    "#fabd2f",
    "#83a598",
    "#d3869b",
    "#8ec07c",
    "#ebdbb2",
  ]),
  "one-dark": palette("#282c34", "#abb2bf", "#528bff", "#3e445188", [
    "#282c34",
    "#e06c75",
    "#98c379",
    "#e5c07b",
    "#61afef",
    "#c678dd",
    "#56b6c2",
    "#abb2bf",
    "#5c6370",
    "#e06c75",
    "#98c379",
    "#e5c07b",
    "#61afef",
    "#c678dd",
    "#56b6c2",
    "#ffffff",
  ]),
  catppuccin: palette("#1e1e2e", "#cdd6f4", "#f5e0dc", "#585b7088", [
    "#45475a",
    "#f38ba8",
    "#a6e3a1",
    "#f9e2af",
    "#89b4fa",
    "#f5c2e7",
    "#94e2d5",
    "#bac2de",
    "#585b70",
    "#f37799",
    "#89d88b",
    "#ebd391",
    "#74a8fc",
    "#f2aede",
    "#6bd7ca",
    "#a6adc8",
  ]),
  "rose-pine": palette("#191724", "#e0def4", "#e0def4", "#403d5288", [
    "#26233a",
    "#eb6f92",
    "#31748f",
    "#f6c177",
    "#9ccfd8",
    "#c4a7e7",
    "#ebbcba",
    "#e0def4",
    "#6e6a86",
    "#eb6f92",
    "#31748f",
    "#f6c177",
    "#9ccfd8",
    "#c4a7e7",
    "#ebbcba",
    "#e0def4",
  ]),
};

export function builtinPalette(id: Theme): TerminalPalette | null {
  return BUILTIN_PALETTES[id] ?? null;
}

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
