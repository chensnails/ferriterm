import { useEffect } from "react";
import { api, isTauri, type RemoteConfig, type SyncScope } from "./api";
import i18n from "./i18n";
import { useAppStore, type SyncStatus } from "./store";

const DEBOUNCE_MS = 3_000;
/** Two unattended uploads never run closer than this, even during a burst of
 * edits. Manual uploads in the settings dialog are not throttled. */
const MIN_GAP_MS = 30_000;
/** A transient failure (network down, server restarting) is retried after
 * these delays. A conflict is not retried: it needs the user's decision. */
const RETRY_DELAYS_MS = [60_000, 180_000];

export type AutoSyncResult =
  | { state: "ok" }
  | { state: "conflict" }
  | { state: "failed"; message: string }
  | { state: "skipped" };

export const SYNC_PASSPHRASE_REF = "sync:passphrase";

/** Rebuilds the remote config from persisted settings. Tokens and passwords
 * stay empty: the Rust side fills them from the encrypted local vault, so no
 * secret ever reaches the webview during an unattended run. */
export async function remoteConfig(): Promise<RemoteConfig | null> {
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

let retryTimer: number | undefined;
let retryCount = 0;

function clearRetry() {
  retryCount = 0;
  window.clearTimeout(retryTimer);
  retryTimer = undefined;
}

function scheduleRetry(scope: SyncScope[]) {
  if (retryCount >= RETRY_DELAYS_MS.length) return;
  const delay = RETRY_DELAYS_MS[retryCount];
  retryCount += 1;
  window.clearTimeout(retryTimer);
  retryTimer = window.setTimeout(() => {
    retryTimer = undefined;
    void runAutoSync(scope);
  }, delay);
}

/** One unattended upload. Incomplete setup stays silent, but a network failure
 * or a conflict is recorded in `syncStatus` and retried, so the user can see
 * why the remote is behind instead of the change vanishing quietly. */
export async function runAutoSync(
  scopeOverride?: SyncScope[],
): Promise<AutoSyncResult> {
  if (!isTauri) return { state: "skipped" };
  const state = useAppStore.getState();
  const scope = scopeOverride ?? state.syncScope;
  if (scope.length === 0) return { state: "skipped" };
  const record = (next: Omit<SyncStatus, "at">) =>
    state.setSyncStatus({ ...next, at: Date.now() });
  try {
    if (!(await api.secretSaved(SYNC_PASSPHRASE_REF))) return { state: "skipped" };
    const config = await remoteConfig();
    if (!config) return { state: "skipped" };
    const outcome = await api.syncAutoUpload(config, scope);
    if (outcome.state === "conflict") {
      clearRetry();
      record({ state: "conflict", message: "" });
      state.setStatus(i18n.t("sync.conflictShort"), true);
      return { state: "conflict" };
    }
    clearRetry();
    record({ state: "ok", message: "" });
    state.setSyncLastAt(Date.now());
    return { state: "ok" };
  } catch (err) {
    const message = String(err);
    record({ state: "error", message });
    scheduleRetry(scope);
    return { state: "failed", message };
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
