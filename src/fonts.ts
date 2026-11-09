// Curated terminal font stacks. The three bundled families are OFL-licensed
// latin subsets shipped in public/fonts; the remaining entries use fonts that
// ship with macOS or Windows so no download is needed.

export interface TerminalFont {
  id: string;
  label: string;
  stack: string;
}

export const TERMINAL_FONTS: TerminalFont[] = [
  {
    id: "jetbrains",
    label: "JetBrains Mono",
    stack:
      '"Ferriterm JetBrains Mono", "JetBrains Mono", "Cascadia Code", Menlo, Consolas, "Courier New", monospace',
  },
  {
    id: "fira",
    label: "Fira Code",
    stack:
      '"Ferriterm Fira Code", "Fira Code", Menlo, Monaco, Consolas, monospace',
  },
  {
    id: "ibm-plex",
    label: "IBM Plex Mono",
    stack:
      '"Ferriterm IBM Plex Mono", "IBM Plex Mono", Menlo, Consolas, monospace',
  },
  {
    id: "sf-mono",
    label: "SF Mono",
    stack: '"SF Mono", SFMono-Regular, ui-monospace, Menlo, Monaco, monospace',
  },
  {
    id: "cascadia",
    label: "Cascadia Code",
    stack: '"Cascadia Code", "Cascadia Mono", Consolas, monospace',
  },
  {
    id: "menlo",
    label: "Menlo",
    stack: 'Menlo, Monaco, "Courier New", monospace',
  },
  {
    id: "monaco",
    label: "Monaco",
    stack: 'Monaco, Menlo, "Courier New", monospace',
  },
  {
    id: "consolas",
    label: "Consolas",
    stack: 'Consolas, "Courier New", monospace',
  },
  {
    id: "system",
    label: "System Mono",
    stack:
      'ui-monospace, "SF Mono", SFMono-Regular, Menlo, Monaco, Consolas, monospace',
  },
];

export const CUSTOM_FONT_ID = "custom";

export const DEFAULT_TERMINAL_FONT_STACK = TERMINAL_FONTS[0].stack;

const PRIMARY_FAMILY: Record<string, string> = {
  "ferriterm jetbrains mono": "jetbrains",
  "jetbrains mono": "jetbrains",
  "ferriterm fira code": "fira",
  "fira code": "fira",
  "ferriterm ibm plex mono": "ibm-plex",
  "ibm plex mono": "ibm-plex",
  "sf mono": "sf-mono",
  "cascadia code": "cascadia",
  menlo: "menlo",
  monaco: "monaco",
  consolas: "consolas",
  "ui-monospace": "system",
};

// Match on the primary family so stacks saved before the bundled fonts existed
// (for example "JetBrains Mono", "Cascadia Code", …) still show as presets.
export function fontChoiceFor(stack: string): string {
  const normalized = stack.replace(/\s+/g, " ").trim();
  const exact = TERMINAL_FONTS.find(
    (font) => font.stack.replace(/\s+/g, " ").trim() === normalized,
  );
  if (exact) return exact.id;
  const primary = normalized.split(",")[0].replace(/["']/g, "").trim().toLowerCase();
  return PRIMARY_FAMILY[primary] ?? CUSTOM_FONT_ID;
}
