import type { TerminalPalette, ThemeBase } from "./theme";

// A user-imported color scheme: full ANSI palette for xterm plus the design
// tokens derived from it so the app chrome matches the terminal colors.
// TOML format: [theme] name/base, [colors] background, foreground and the 16
// ANSI names (black…brightWhite, camel or snake case). cursor/selection are
// optional and fall back to foreground and a blended foreground/background.
export interface ImportedTheme {
  id: string;
  name: string;
  base: ThemeBase;
  tokens: Record<string, string>;
  palette: TerminalPalette;
}

const ANSI_KEYS = [
  "black",
  "red",
  "green",
  "yellow",
  "blue",
  "magenta",
  "cyan",
  "white",
  "brightBlack",
  "brightRed",
  "brightGreen",
  "brightYellow",
  "brightBlue",
  "brightMagenta",
  "brightCyan",
  "brightWhite",
] as const;

type AnsiKey = (typeof ANSI_KEYS)[number];

const HEX = /^#?([0-9a-f]{3}|[0-9a-f]{6})$/i;

function normalizeHex(raw: string): string | null {
  const match = HEX.exec(raw.trim());
  if (!match) return null;
  const digits = match[1].toLowerCase();
  if (digits.length === 6) return `#${digits}`;
  return `#${digits
    .split("")
    .map((ch) => ch + ch)
    .join("")}`;
}

function channels(hex: string): [number, number, number] {
  const value = normalizeHex(hex) ?? "#000000";
  return [
    Number.parseInt(value.slice(1, 3), 16),
    Number.parseInt(value.slice(3, 5), 16),
    Number.parseInt(value.slice(5, 7), 16),
  ];
}

function toHex(red: number, green: number, blue: number): string {
  const channel = (value: number) =>
    Math.max(0, Math.min(255, Math.round(value)))
      .toString(16)
      .padStart(2, "0");
  return `#${channel(red)}${channel(green)}${channel(blue)}`;
}

export function blendHex(from: string, to: string, ratio: number): string {
  const a = channels(from);
  const b = channels(to);
  return toHex(
    a[0] + (b[0] - a[0]) * ratio,
    a[1] + (b[1] - a[1]) * ratio,
    a[2] + (b[2] - a[2]) * ratio,
  );
}

function luminance(hex: string): number {
  const linear = channels(hex).map((value) => {
    const channel = value / 255;
    return channel <= 0.03928
      ? channel / 12.92
      : ((channel + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2];
}

function schemeId(name: string): string {
  const slug = name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return slug ? `custom-${slug}` : "custom-scheme";
}

function schemeName(filename: string): string {
  return filename.replace(/\.(itermcolors|toml|tml)$/i, "").trim() || "Custom";
}

function schemeTokens(
  palette: TerminalPalette,
  base: ThemeBase,
): Record<string, string> {
  const dark = base === "dark";
  const shade = dark ? palette.foreground : "#000000";
  return {
    "--bg-base": palette.background,
    "--bg-panel": blendHex(palette.background, shade, dark ? 0.06 : 0.04),
    "--bg-elevated": blendHex(palette.background, shade, dark ? 0.1 : 0.07),
    "--border": blendHex(palette.background, shade, dark ? 0.18 : 0.14),
    "--fg-primary": palette.foreground,
    "--fg-muted": blendHex(palette.foreground, palette.background, 0.4),
    "--fg-on-accent":
      luminance(palette.cursor) > 0.45
        ? blendHex(palette.background, "#000000", 0.5)
        : palette.foreground,
    "--accent": palette.cursor,
    "--accent-hover": blendHex(palette.cursor, dark ? "#ffffff" : "#000000", 0.18),
    "--danger": palette.red,
    "--warning": palette.yellow,
    "--success": palette.green,
    "--selection": palette.selection,
  };
}

function buildScheme(
  name: string,
  base: ThemeBase | undefined,
  palette: TerminalPalette,
): ImportedTheme {
  const resolved =
    base ?? (luminance(palette.background) > 0.45 ? "light" : "dark");
  return {
    id: schemeId(name),
    name,
    base: resolved,
    tokens: schemeTokens(palette, resolved),
    palette,
  };
}

function readPlistColor(dict: Element): string | null {
  const entries = new Map<string, string>();
  const children = Array.from(dict.children);
  for (let i = 0; i < children.length; i += 1) {
    if (children[i].tagName !== "key") continue;
    entries.set(
      children[i].textContent?.trim() ?? "",
      children[i + 1]?.textContent?.trim() ?? "",
    );
    i += 1;
  }
  const component = (name: string): number | null => {
    const value = Number.parseFloat(entries.get(name) ?? "");
    return Number.isFinite(value) ? Math.max(0, Math.min(1, value)) : null;
  };
  const red = component("Red Component");
  const green = component("Green Component");
  const blue = component("Blue Component");
  if (red === null || green === null || blue === null) return null;
  return toHex(red * 255, green * 255, blue * 255);
}

export function parseItermColors(
  text: string,
  filename: string,
): ImportedTheme {
  const doc = new DOMParser().parseFromString(text, "application/xml");
  if (doc.querySelector("parsererror")) {
    throw new Error("invalid plist XML");
  }
  const root = doc.documentElement;
  if (!root || root.tagName !== "plist") {
    throw new Error("not an iTerm2 color preset");
  }
  const top = Array.from(root.children).find((el) => el.tagName === "dict");
  if (!top) throw new Error("missing color dictionary");
  const colors = new Map<string, string>();
  const children = Array.from(top.children);
  for (let i = 0; i < children.length; i += 1) {
    if (children[i].tagName !== "key") continue;
    const value = children[i + 1];
    if (value?.tagName === "dict") {
      const color = readPlistColor(value);
      if (color) colors.set(children[i].textContent?.trim() ?? "", color);
    }
    i += 1;
  }
  const background = colors.get("Background Color");
  const foreground = colors.get("Foreground Color");
  if (!background || !foreground) {
    throw new Error("missing background or foreground color");
  }
  const ansi = ANSI_KEYS.map((_, index) => {
    const color = colors.get(`Ansi ${index} Color`);
    if (!color) throw new Error(`missing Ansi ${index} Color`);
    return color;
  });
  const cursor = colors.get("Cursor Color") ?? foreground;
  const selection =
    colors.get("Selection Color") ?? blendHex(cursor, background, 0.5);
  const palette: TerminalPalette = {
    background,
    foreground,
    cursor,
    selection,
    black: ansi[0],
    red: ansi[1],
    green: ansi[2],
    yellow: ansi[3],
    blue: ansi[4],
    magenta: ansi[5],
    cyan: ansi[6],
    white: ansi[7],
    brightBlack: ansi[8],
    brightRed: ansi[9],
    brightGreen: ansi[10],
    brightYellow: ansi[11],
    brightBlue: ansi[12],
    brightMagenta: ansi[13],
    brightCyan: ansi[14],
    brightWhite: ansi[15],
  };
  return buildScheme(schemeName(filename), undefined, palette);
}

function stripComment(line: string): string {
  let quoted = false;
  for (let i = 0; i < line.length; i += 1) {
    if (line[i] === '"') quoted = !quoted;
    else if (line[i] === "#" && !quoted) return line.slice(0, i);
  }
  return line;
}

const PALETTE_ALIASES = new Map<string, keyof TerminalPalette>([
  ["background", "background"],
  ["bg", "background"],
  ["foreground", "foreground"],
  ["fg", "foreground"],
  ["cursor", "cursor"],
  ["selection", "selection"],
  ...ANSI_KEYS.map(
    (key): [string, keyof TerminalPalette] => [key.toLowerCase(), key],
  ),
]);

export function parseTomlScheme(text: string, filename: string): ImportedTheme {
  const meta = new Map<string, string>();
  const colors = new Map<keyof TerminalPalette, string>();
  let section = "";
  text.split(/\r?\n/).forEach((raw, index) => {
    const line = stripComment(raw).trim();
    if (!line) return;
    if (line.startsWith("[")) {
      const match = /^\[([a-z-]+)\]$/i.exec(line);
      const name = match?.[1].toLowerCase();
      if (name !== "theme" && name !== "colors") {
        throw new Error(`line ${index + 1}: unknown section ${line}`);
      }
      section = name;
      return;
    }
    const match = /^([A-Za-z0-9_-]+)\s*=\s*"([^"]*)"$/.exec(line);
    if (!match) {
      throw new Error(`line ${index + 1}: expected key = "value"`);
    }
    const key = match[1].toLowerCase().replace(/[-_]/g, "");
    const value = match[2].trim();
    if (section === "theme") {
      if (key !== "name" && key !== "base") {
        throw new Error(`line ${index + 1}: unknown theme key ${match[1]}`);
      }
      meta.set(key, value);
      return;
    }
    if (section !== "colors") {
      throw new Error(`line ${index + 1}: value outside a section`);
    }
    const paletteKey = PALETTE_ALIASES.get(key);
    if (!paletteKey) {
      throw new Error(`line ${index + 1}: unknown color ${match[1]}`);
    }
    const hex = normalizeHex(value);
    if (!hex) {
      throw new Error(`line ${index + 1}: ${match[1]} needs a hex color`);
    }
    colors.set(paletteKey, hex);
  });

  const read = (key: keyof TerminalPalette): string => {
    const value = colors.get(key);
    if (!value) throw new Error(`missing color ${key}`);
    return value;
  };
  const base = meta.get("base");
  if (base !== undefined && base !== "dark" && base !== "light") {
    throw new Error("theme base must be dark or light");
  }
  const background = read("background");
  const foreground = read("foreground");
  const cursor = colors.get("cursor") ?? foreground;
  const palette: TerminalPalette = {
    background,
    foreground,
    cursor,
    selection: colors.get("selection") ?? blendHex(cursor, background, 0.5),
    black: read("black"),
    red: read("red"),
    green: read("green"),
    yellow: read("yellow"),
    blue: read("blue"),
    magenta: read("magenta"),
    cyan: read("cyan"),
    white: read("white"),
    brightBlack: read("brightBlack"),
    brightRed: read("brightRed"),
    brightGreen: read("brightGreen"),
    brightYellow: read("brightYellow"),
    brightBlue: read("brightBlue"),
    brightMagenta: read("brightMagenta"),
    brightCyan: read("brightCyan"),
    brightWhite: read("brightWhite"),
  };
  return buildScheme(
    meta.get("name") || schemeName(filename),
    base as ThemeBase | undefined,
    palette,
  );
}

export function parseSchemeFile(
  filename: string,
  text: string,
): ImportedTheme {
  const lower = filename.toLowerCase();
  if (lower.endsWith(".itermcolors")) return parseItermColors(text, filename);
  if (lower.endsWith(".toml") || lower.endsWith(".tml")) {
    return parseTomlScheme(text, filename);
  }
  if (/<\?xml|<plist|<dict/i.test(text)) return parseItermColors(text, filename);
  return parseTomlScheme(text, filename);
}

function isPalette(value: unknown): value is TerminalPalette {
  if (!value || typeof value !== "object") return false;
  const record = value as Record<string, unknown>;
  const keys: (keyof TerminalPalette)[] = [
    "background",
    "foreground",
    "cursor",
    "selection",
    ...ANSI_KEYS,
  ];
  return keys.every((key) => typeof record[key] === "string");
}

function isImportedTheme(value: unknown): value is ImportedTheme {
  if (!value || typeof value !== "object") return false;
  const record = value as Record<string, unknown>;
  return (
    typeof record.id === "string" &&
    typeof record.name === "string" &&
    (record.base === "dark" || record.base === "light") &&
    !!record.tokens &&
    typeof record.tokens === "object" &&
    isPalette(record.palette)
  );
}

export function parseStoredThemes(raw: string | undefined): ImportedTheme[] {
  if (!raw) return [];
  try {
    const value: unknown = JSON.parse(raw);
    return Array.isArray(value) ? value.filter(isImportedTheme) : [];
  } catch {
    return [];
  }
}
