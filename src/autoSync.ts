import { useEffect } from "react";
import { api, isTauri, type RemoteConfig, type SyncScope } from "./api";
import { useAppStore } from "./store";

const DEBOUNCE_MS = 3_000;
/** Two unattended uploads never run closer than this, even during a burst of
 * edits. Manual uploads in the settings dialog are not throttled. */
const MIN_GAP_MS = 30_000;

export const SYNC_PASSPHRASE_REF = "sync:passphrase";

/** Rebuilds the remote config from persisted settings. Tokens and passwords
 * stay empty: the Rust side fills them from the encrypted local vault, so no
 * secret ever reaches the webview during an unattended run. */
async function remoteConfig(): Promise<RemoteConfig | null> {
  const settings = await api.getSettings();
  if (settings["sync:provider"] === "webdav") {
    const url = settings["sync:webdav-url"] ?? "";
    if (!url.trim()) return null;
    return {
      kind: "webdav",
      url,
      username: settings["sync:webdav-user"] ?? "",
      password: "",
      filename: settings["sync:webdav-file"] || "ferriterm-backup.ftbak",
    };
  }
  if (!(await api.secretSaved("sync:gist-token"))) return null;
  return {
    kind: "gist",
    token: "",
    gistId: settings["sync:gist-id"] ?? "",
    filename: settings["sync:gist-file"] || "ferriterm-backup.ftbak",
  };
}

/** One unattended upload. Incomplete setup and network failures are swallowed
 * on purpose: a background sync must never interrupt a terminal session. */
export async function runAutoSync(scopeOverride?: SyncScope[]): Promise<boolean> {
  if (!isTauri) return false;
  const state = useAppStore.getState();
  const scope = scopeOverride ?? state.syncScope;
  if (scope.length === 0) return false;
  try {
    if (!(await api.secretSaved(SYNC_PASSPHRASE_REF))) return false;
    const config = await remoteConfig();
    if (!config) return false;
    await api.syncAutoUpload(config, scope);
    state.setSyncLastAt(Date.now());
    return true;
  } catch (err) {
    console.warn("ferriterm: automatic sync skipped", err);
    return false;
  }
}

type AppState = ReturnType<typeof useAppStore.getState>;

/** Only managed data counts as a change. Session, layout and sync-status
 * updates are excluded so a completed upload cannot retrigger itself. */
function dataChanged(next: AppState, prev: AppState): boolean {
  return (
    next.hosts !== prev.hosts ||
    next.tags !== prev.tags ||
    next.keywordHighlights !== prev.keywordHighlights ||
    next.shortcuts !== prev.shortcuts
  );
}

/** Mount once in App. Re-subscribes when the policy or interval changes. */
export function useAutoSync(): void {
  const policy = useAppStore((s) => s.syncPolicy);
  const interval = useAppStore((s) => s.syncInterval);

  useEffect(() => {
    if (!isTauri || policy === "manual") return;
    let debounce: number | undefined;
    let lastRun = 0;
    let running = false;

    const run = async () => {
      if (running || Date.now() - lastRun < MIN_GAP_MS) return;
      running = true;
      lastRun = Date.now();
      try {
        await runAutoSync();
      } finally {
        running = false;
      }
    };

    const unsubscribe =
      policy === "onChange"
        ? useAppStore.subscribe((next, prev) => {
            if (dataChanged(next, prev)) {
              window.clearTimeout(debounce);
              debounce = window.setTimeout(() => void run(), DEBOUNCE_MS);
            }
          })
        : undefined;

    const timer =
      policy === "interval"
        ? window.setInterval(() => void run(), interval * 60_000)
        : undefined;

    return () => {
      window.clearTimeout(debounce);
      if (timer !== undefined) window.clearInterval(timer);
      unsubscribe?.();
    };
  }, [policy, interval]);
}
