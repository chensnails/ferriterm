import type { ITheme } from "@xterm/xterm";

function token(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

// xterm needs concrete colors; read them from the design tokens so the terminal
// palette follows the active theme instead of hardcoding values.
export function terminalTheme(): ITheme {
  return {
    background: token("--bg-base"),
    foreground: token("--fg-primary"),
    cursor: token("--accent"),
    selectionBackground: token("--selection"),
  };
}

export type Theme = "dark" | "light";

export function applyTheme(theme: Theme) {
  document.documentElement.dataset.theme = theme;
}