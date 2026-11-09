import { isTauri } from "./api";

/**
 * How the native frame is drawn around the webview.
 *
 * - "native": the OS title bar stays; the header is a normal tab strip.
 * - "custom": the frame is removed at runtime and the header draws its own
 *   window buttons (Windows/Linux).
 * - "overlay": macOS keeps the native traffic lights, drawn over our header,
 *   which only reserves a gutter for them.
 */
export type WindowChrome = "native" | "custom" | "overlay";

const isMac =
  typeof navigator !== "undefined" && /Mac OS X|Macintosh/.test(navigator.userAgent);

/** Synchronous first guess so the header never flashes the wrong shape. */
export const initialWindowChrome: WindowChrome = isMac ? "overlay" : "native";

/**
 * Removes the OS frame when the build grants `core:window:allow-set-decorations`.
 * Builds that predate that permission keep the native frame instead of showing
 * window buttons that cannot work.
 */
export async function resolveWindowChrome(): Promise<WindowChrome> {
  if (isMac) return "overlay";
  if (!isTauri) return "native";
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    const win = getCurrentWindow();
    await win.setDecorations(false);
    return "custom";
  } catch {
    return "native";
  }
}

async function currentWindow() {
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  return getCurrentWindow();
}

export async function minimizeWindow(): Promise<void> {
  try {
    await (await currentWindow()).minimize();
  } catch {
    // window commands are best-effort; the OS frame still covers a fallback
  }
}

export async function toggleMaximizeWindow(): Promise<void> {
  try {
    await (await currentWindow()).toggleMaximize();
  } catch {
    // ignore
  }
}

export async function closeWindow(): Promise<void> {
  try {
    await (await currentWindow()).close();
  } catch {
    // ignore
  }
}

export type ResizeEdge =
  | "North"
  | "South"
  | "East"
  | "West"
  | "NorthEast"
  | "NorthWest"
  | "SouthEast"
  | "SouthWest";

/**
 * With the OS frame removed there is nothing left to grab, so the header draws
 * eight invisible edges that hand the drag back to the window manager.
 */
export async function beginResize(edge: ResizeEdge): Promise<void> {
  try {
    await (await currentWindow()).startResizeDragging(edge);
  } catch {
    // the OS may refuse while maximized; the window is already at full size
  }
}
