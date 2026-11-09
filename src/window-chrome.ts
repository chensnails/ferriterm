import type { MouseEvent as ReactMouseEvent } from "react";
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

/**
 * Hands a pointer press back to the OS so the whole window moves. Tauri's
 * `data-tauri-drag-region` listener skips presses on interactive elements, so
 * the header calls this itself for the empty gutters between tabs and tools.
 */
export async function startWindowDrag(): Promise<void> {
  try {
    await (await currentWindow()).startDragging();
  } catch {
    // start-dragging needs a real mouse button held; ignore stray calls
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

/** Presses that belong to a control keep their click instead of moving the window. */
const DRAG_BLOCKERS =
  'button, a, input, select, textarea, label, summary, [role="tab"], [role="menuitem"], [role="option"], [contenteditable]:not([contenteditable="false"])';

function pressedOnControl(event: ReactMouseEvent<HTMLElement>): boolean {
  const target = event.target;
  return !(target instanceof Element) || target.closest(DRAG_BLOCKERS) !== null;
}

/**
 * Starts a window drag from a header press. Tauri's `data-tauri-drag-region`
 * helper skips any press whose target is clickable, and on macOS presses on
 * the transparent title-bar strip never reach the webview at all, so the
 * header calls the window directly for its own background.
 */
export function beginWindowDrag(event: ReactMouseEvent<HTMLElement>): void {
  if (!isTauri || event.button !== 0 || event.detail !== 1) return;
  if (event.defaultPrevented || pressedOnControl(event)) return;
  event.preventDefault();
  void startWindowDrag();
}

/** Double-clicking bare header space follows the platform zoom gesture. */
export function zoomWindowOnDoubleClick(
  event: ReactMouseEvent<HTMLElement>,
): void {
  if (!isTauri || pressedOnControl(event)) return;
  void toggleMaximizeWindow();
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
