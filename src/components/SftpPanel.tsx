import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, isTauri, onClosed, type VfsEntry } from "../api";
import { useAppStore, type SessionMeta } from "../store";
import { connectAlgorithms } from "../ssh-algorithms";
import { FileEditor } from "./FileEditor";
import { FilePane } from "./FilePane";
import { PermissionsDialog } from "./PermissionsDialog";

function parseBookmarks(raw: string | undefined): string[] {
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed)
      ? parsed.filter((item): item is string => typeof item === "string")
      : [];
  } catch {
    return [];
  }
}

type Phase = "connecting" | "ready" | "failed";

/**
 * The tab an SFTP host opens into. It rides the ordinary SSH transport but
 * shows the remote disk instead of a shell, so uploads, edits and permission
 * changes never need the Transfer view's second pane.
 */
export function SftpPanel({
  session,
  pane,
}: {
  session: SessionMeta;
  pane: "primary" | "secondary" | null;
}) {
  const { t } = useTranslation();
  const [phase, setPhase] = useState<Phase>(isTauri ? "connecting" : "ready");
  const attemptRef = useRef<string | null>(null);
  const [path, setPath] = useState("");
  const [showHidden, setShowHidden] = useState(
    () => useAppStore.getState().sftpShowHidden,
  );
  const [bookmarks, setBookmarks] = useState<string[]>([]);
  const [error, setError] = useState("");
  const [refreshToken, setRefreshToken] = useState(0);
  const [editing, setEditing] = useState<string | null>(null);
  const [permissionTarget, setPermissionTarget] = useState<VfsEntry | null>(
    null,
  );

  useEffect(() => {
    if (!isTauri) return;
    // One dial per (session, epoch). Without the guard, any unrelated re-render
    // that changes this effect's identity would re-run it, find the pending
    // connect already consumed, and flip a healthy tab to "failed".
    const attempt = `${session.id}:${session.connectEpoch}`;
    if (attemptRef.current === attempt) return;
    attemptRef.current = attempt;
    const store = useAppStore.getState();
    const pending = store.consumePending(session.id);
    if (!pending || (pending.type !== "ssh" && pending.type !== "sftp")) {
      setPhase("failed");
      setError(t("sftp.noLink"));
      store.markSessionError(session.id, t("sftp.noLink"));
      return;
    }
    let cancelled = false;
    store.markConnecting(session.id);
    store.setStatus(t("sftp.connecting"));
    const algorithms = connectAlgorithms(store.sshSecurity);
    api
      .sshConnect(session.id, {
        ...pending.args,
        ...(algorithms ? { algorithms } : {}),
        cols: 80,
        rows: 24,
      })
      .then(() => {
        if (cancelled) return;
        setPhase("ready");
        setError("");
        useAppStore.getState().markConnected(session.id);
        useAppStore.getState().setStatus("");
      })
      .catch((err) => {
        if (cancelled) return;
        const message = String(err);
        setError(message);
        setPhase("failed");
        useAppStore.getState().setStatus(message, true);
        useAppStore.getState().markSessionError(session.id, message);
        // Surface the failure on the host row the same way the terminal does,
        // so a bad key or password is visible where the host was launched.
        if (pending.args.hostId) {
          useAppStore.getState().reportConnectError(pending.args.hostId, message);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [session.id, session.connectEpoch, t]);

  useEffect(() => {
    if (!isTauri) return;
    let disposed = false;
    let unlisten: (() => void) | null = null;
    void onClosed(session.id, (payload) => {
      if (disposed) return;
      setPhase("failed");
      setError(t(`connection.reason.${payload?.reason ?? "unknown"}`));
      useAppStore.getState().markClosed(session.id, payload);
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [session.id, t]);

  useEffect(() => {
    if (!isTauri) return;
    let cancelled = false;
    api
      .getSettings()
      .then((settings) => {
        if (!cancelled) {
          setBookmarks(parseBookmarks(settings["bookmarks:remote"]));
        }
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  const refresh = useCallback(() => setRefreshToken((value) => value + 1), []);

  const saveBookmarks = (next: string[]) => {
    setBookmarks(next);
    if (isTauri) {
      api.setSetting("bookmarks:remote", JSON.stringify(next)).catch(() => {});
    }
  };

  const addBookmark = (target: string) => {
    if (!target || bookmarks.includes(target)) return;
    saveBookmarks([...bookmarks, target]);
  };

  const removeBookmark = (target: string) => {
    saveBookmarks(bookmarks.filter((item) => item !== target));
  };

  return (
    <div
      className={`terminal-panel sftp-panel${pane ? ` pane-${pane}` : ""}`}
      onPointerDown={() =>
        useAppStore.getState().setFocusedSession(session.id)
      }
    >
      {error && (
        <p className="error-text sftp-error" role="alert">
          {error}
        </p>
      )}
      {phase === "ready" ? (
        <FilePane
          title={t("sftp.title", { name: session.title })}
          sessionId={session.id}
          path={path}
          onPathChange={setPath}
          showHidden={showHidden}
          onToggleHidden={() => setShowHidden((value) => !value)}
          bookmarks={bookmarks}
          onAddBookmark={addBookmark}
          onRemoveBookmark={removeBookmark}
          onEdit={(entry) => setEditing(entry.path)}
          onPermissions={setPermissionTarget}
          onError={setError}
          refreshToken={refreshToken}
        />
      ) : (
        <div className="sftp-status">
          {phase === "connecting" ? (
            <p className="hint">
              <span className="ui-loading-spinner" aria-hidden="true" />
              {t("sftp.connectingHint")}
            </p>
          ) : (
            <>
              <p className="hint">{t("sftp.failedHint")}</p>
              {session.lastConnect && (
                <button
                  type="button"
                  className="primary"
                  onClick={() => useAppStore.getState().reconnect(session.id)}
                >
                  {t("connection.reconnect")}
                </button>
              )}
            </>
          )}
        </div>
      )}
      {editing && (
        <FileEditor
          sessionId={session.id}
          path={editing}
          onClose={() => setEditing(null)}
          onSaved={refresh}
        />
      )}
      {permissionTarget && (
        <PermissionsDialog
          sessionId={session.id}
          entry={permissionTarget}
          onClose={() => setPermissionTarget(null)}
          onSaved={refresh}
          onError={setError}
        />
      )}
    </div>
  );
}
