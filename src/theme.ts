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

// "one-dark" and "atom-one-dark" shipped the same background, foreground and
// ANSI table, differentiated only by their accent. They were merged in favour
// of the four light/dark brand pairs, so a stored setting written by an older
// build must still resolve to a real theme instead of falling back to default.
const LEGACY_THEME_IDS: Record<string, Theme> = {
  "one-dark": "atom-one-dark",
};

export function normalizeThemeId(id: Theme): Theme {
  return LEGACY_THEME_IDS[id] ?? id;
}

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
  { id: "termius-light", base: "light" },
  { id: "iterm2-light", base: "light" },
  { id: "atom-one-light", base: "light" },
  { id: "termius-dark", base: "dark" },
  { id: "iterm2-dark", base: "dark" },
  { id: "atom-one-dark", base: "dark" },
  { id: "github-dark", base: "dark" },
  { id: "tokyo-night", base: "dark" },
  { id: "gruvbox", base: "dark" },
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
  "termius-light": palette("#f5fbfa", "#163b36", "#0ea5a0", "#0ea5a040", [
    "#1b3d38",
    "#e5484d",
    "#18824f",
    "#b26a06",
    "#1f7ae0",
    "#9a3fb0",
    "#0e9bb0",
    "#6b8a84",
    "#4a6b65",
    "#f0686c",
    "#3aa86a",
    "#d68a1a",
    "#4b97ec",
    "#c25fd0",
    "#34c2d6",
    "#aac3bd",
  ]),
  "iterm2-light": palette("#ffffff", "#2b2b35", "#6e56cf", "#6e56cf3a", [
    "#1a1a22",
    "#c23529",
    "#1a9d55",
    "#c08a1a",
    "#3b6fe0",
    "#b03fb0",
    "#1bb6c4",
    "#9aa0aa",
    "#5a5a68",
    "#e85a52",
    "#3ac06f",
    "#e0a82a",
    "#6b95f0",
    "#d06fd0",
    "#4ad0dc",
    "#c8ccd4",
  ]),
  "atom-one-light": palette("#fafafa", "#383a42", "#4078f2", "#4078f240", [
    "#1b1d23",
    "#e45649",
    "#50a14f",
    "#c18401",
    "#4078f2",
    "#a626a4",
    "#0184bc",
    "#f0f0f1",
    "#9aa0a8",
    "#e06c75",
    "#80c990",
    "#e3b341",
    "#6c9eff",
    "#d16fd0",
    "#4ab6d6",
    "#abb2bf",
  ]),
  "termius-dark": palette("#0b1f1d", "#d6efe9", "#0ea5a0", "#0ea5a040", [
    "#0f2624",
    "#ff6b6f",
    "#3ad17e",
    "#e0a82a",
    "#4b97ec",
    "#c45fd0",
    "#34c2d6",
    "#bcd8d1",
    "#3a5b54",
    "#ff8a8d",
    "#5fe09a",
    "#f0bd4a",
    "#74b0f2",
    "#d98ce0",
    "#5fd8e8",
    "#e6f5f0",
  ]),
  "iterm2-dark": palette("#16141f", "#e6e3f4", "#6e56cf", "#6e56cf3a", [
    "#1c1a28",
    "#ff6b6b",
    "#3ad17e",
    "#e0a82a",
    "#6b95f0",
    "#cf6fe0",
    "#4ad0dc",
    "#c6c2da",
    "#46425c",
    "#ff8a8a",
    "#5fe09a",
    "#f0bd4a",
    "#8fb0f5",
    "#dd92ee",
    "#6fdce6",
    "#ece9f7",
  ]),
  "atom-one-dark": palette("#282c34", "#abb2bf", "#4078f2", "#4078f240", [
    "#282c34",
    "#e06c75",
    "#98c379",
    "#e5c07b",
    "#4078f2",
    "#c678dd",
    "#56b6c2",
    "#abb2bf",
    "#5c6370",
    "#e06c75",
    "#98c379",
    "#e5c07b",
    "#4078f2",
    "#c678dd",
    "#56b6c2",
    "#ffffff",
  ]),
  "github-dark": palette("#0d1117", "#e6edf3", "#2f81f7", "#2f81f74d", [
    "#484f58",
    "#ff7b72",
    "#3fb950",
    "#d29922",
    "#58a6ff",
    "#bc8cff",
    "#39c5cf",
    "#b1bac4",
    "#6e7681",
    "#ffa198",
    "#56d364",
    "#e3b341",
    "#79c0ff",
    "#d2a8ff",
    "#56d4dd",
    "#f0f6fc",
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
  return BUILTIN_PALETTES[normalizeThemeId(id)] ?? null;
}

const TOKEN_NAME = /^--[a-z0-9-]+$/i;
const TOKEN_VALUE = /^[#a-z0-9(),.%\s+-]+$/i;

let appliedPackTokens: string[] = [];

export function builtinTheme(id: Theme): BuiltinTheme | undefined {
  const normalized = normalizeThemeId(id);
  return BUILTIN_THEMES.find((item) => item.id === normalized);
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
  const id = normalizeThemeId(theme);
  const base = baseOf(id);
  root.dataset.theme = tokens && Object.keys(tokens).length > 0 ? base : id;
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

/** Interface type scale. Every `--fs-*` token resolves from these three, so
 *  one number moves the whole chrome without touching the terminal. */
export function applyUiFont(size: number) {
  const root = document.documentElement;
  const base = Math.min(16, Math.max(11, Math.round(size)));
  root.style.setProperty("--font-ui", `${base}px`);
  root.style.setProperty("--font-ui-sm", `${Math.max(10, base - 1)}px`);
  root.style.setProperty("--font-title", `${base + 2}px`);
}

/** Motion is a preference, not a theme: `data-motion="reduced"` is what the
 *  stylesheet keys off, next to the OS-level `prefers-reduced-motion` query. */
export function applyMotion(reduced: boolean) {
  const root = document.documentElement;
  if (reduced) root.dataset.motion = "reduced";
  else delete root.dataset.motion;
}
