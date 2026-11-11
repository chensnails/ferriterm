import {
  readText as nativeReadText,
  writeText as nativeWriteText,
} from "@tauri-apps/plugin-clipboard-manager";
import { isTauri } from "./api";

/**
 * Webview clipboard access needs a secure context and can be denied by the
 * platform; the native plugin keeps copy and paste working there instead of
 * leaving the terminal without a clipboard.
 */
export async function readClipboardText(): Promise<string> {
  try {
    if (navigator.clipboard?.readText) {
      return await navigator.clipboard.readText();
    }
  } catch {
    // denied or unavailable: fall through to the native path
  }
  if (isTauri) return nativeReadText();
  return "";
}

export async function writeClipboardText(text: string): Promise<void> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return;
    }
  } catch {
    // denied or unavailable: fall through to the native path
  }
  if (isTauri) await nativeWriteText(text);
}
