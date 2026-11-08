import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, isTauri, type RemoteMountInfo, type VfsEntry } from "../api";
import { useAppStore } from "../store";
import { FileEditor } from "./FileEditor";
import { FilePane } from "./FilePane";
import { MountDialog } from "./MountDialog";
import { PermissionsDialog } from "./PermissionsDialog";
import { SyncDialog, type SyncDirection } from "./SyncDialog";
import { TransferQueue } from "./TransferQueue";

function joinPath(base: string, name: string): string {
  if (!base) return `/${name}`;
  return `${base.replace(/\/+$/, "")}/${name}`;
}

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

export function TransferPanel() {
  const { t } = useTranslation();
  const closeTransfer = useAppStore((s) => s.closeTransfer);
  const sessions = useAppStore((s) => s.sessions);
  const activeId = useAppStore((s) => s.activeId);
  const boundSession = useAppStore((s) => s.transferSessionId);
  const setTransferSession = useAppStore((s) => s.setTransferSession);
  const jobs = useAppStore((s) => s.transfers);
  const setTransfers = useAppStore((s) => s.setTransfers);

  const sshSessions = useMemo(
    () => sessions.filter((session) => session.kind === "ssh" && !session.closed),
    [sessions],
  );
  const [mounts, setMounts] = useState<RemoteMountInfo[]>([]);
  const mountTargets = useMemo(
    () => mounts.map((mount) => ({ id: `@${mount.id}`, title: mount.label })),
    [mounts],
  );
  const knownTargets = useMemo(() => {
    const ids = new Set(sshSessions.map((session) => session.id));
    for (const target of mountTargets) ids.add(target.id);
    return ids;
  }, [sshSessions, mountTargets]);
  const sessionId =
    (boundSession && knownTargets.has(boundSession) ? boundSession : null) ??
    sshSessions.find((s) => s.id === activeId)?.id ??
    sshSessions[0]?.id ??
    mountTargets[0]?.id ??
    "";

  const [localPath, setLocalPath] = useState("");
  const [remotePath, setRemotePath] = useState("");
  const [localHidden, setLocalHidden] = useState(false);
  const [remoteHidden, setRemoteHidden] = useState(false);
  const [localBookmarks, setLocalBookmarks] = useState<string[]>([]);
  const [remoteBookmarks, setRemoteBookmarks] = useState<string[]>([]);
  const [error, setError] = useState("");
  const [refreshToken, setRefreshToken] = useState(0);
  const [syncDirection, setSyncDirection] = useState<SyncDirection | null>(null);
  const [editing, setEditing] = useState<{
    sessionId: string | null;
    path: string;
  } | null>(null);
  const [permissionTarget, setPermissionTarget] = useState<{
    sessionId: string | null;
    entry: VfsEntry;
  } | null>(null);
  const [mountOpen, setMountOpen] = useState(false);

  const loadMounts = useCallback(async () => {
    if (!isTauri) return;
    try {
      setMounts(await api.remoteMountList());
    } catch (err) {
      setError(String(err));
    }
  }, []);

  useEffect(() => {
    void loadMounts();
  }, [loadMounts]);

  useEffect(() => {
    setRemotePath("");
  }, [sessionId]);

  useEffect(() => {
    if (!isTauri) return;
    void (async () => {
      try {
        const settings = await api.getSettings();
        setLocalBookmarks(parseBookmarks(settings["bookmarks:local"]));
        setRemoteBookmarks(parseBookmarks(settings["bookmarks:remote"]));
        setTransfers(await api.transferList());
      } catch (err) {
        setError(String(err));
      }
    })();
  }, [setTransfers]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") closeTransfer();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [closeTransfer]);

  const refresh = useCallback(() => setRefreshToken((value) => value + 1), []);

  const saveBookmarks = (
    side: "local" | "remote",
    next: string[],
  ) => {
    if (side === "local") setLocalBookmarks(next);
    else setRemoteBookmarks(next);
    api
      .setSetting(`bookmarks:${side}`, JSON.stringify(next))
      .catch(() => {});
  };

  const addBookmark = (side: "local" | "remote", path: string) => {
    if (!path) return;
    const current = side === "local" ? localBookmarks : remoteBookmarks;
    if (current.includes(path)) return;
    saveBookmarks(side, [...current, path]);
  };

  const removeBookmark = (side: "local" | "remote", path: string) => {
    const current = side === "local" ? localBookmarks : remoteBookmarks;
    saveBookmarks(
      side,
      current.filter((item) => item !== path),
    );
  };

  const enqueue = async (
    request: Parameters<typeof api.transferEnqueue>[0],
  ) => {
    setError("");
    try {
      await api.transferEnqueue(request);
      setTransfers(await api.transferList());
    } catch (err) {
      setError(String(err));
    }
  };

  const sendToRemote = (entry: VfsEntry) => {
    if (!sessionId || !remotePath) {
      setError(t("transfer.chooseSession"));
      return;
    }
    void enqueue({
      id: crypto.randomUUID(),
      sourceSession: null,
      sourcePath: entry.path,
      targetSession: sessionId,
      targetPath: joinPath(remotePath, entry.name),
      directory: entry.isDir,
    });
  };

  const sendToLocal = (entry: VfsEntry) => {
    if (!sessionId || !localPath) {
      setError(t("transfer.chooseSession"));
      return;
    }
    void enqueue({
      id: crypto.randomUUID(),
      sourceSession: sessionId,
      sourcePath: entry.path,
      targetSession: null,
      targetPath: joinPath(localPath, entry.name),
      directory: entry.isDir,
    });
  };

  const applyQueue = () => {
    void api
      .transferList()
      .then(setTransfers)
      .catch((err) => setError(String(err)));
  };

  const canSync = Boolean(sessionId && localPath && remotePath);

  return (
    <div className="transfer-backdrop">
      <div className="transfer-panel" role="dialog" aria-modal="true">
        <header className="transfer-head">
          <h3>{t("transfer.title")}</h3>
          <div className="transfer-head-tools">
            <select
              value={sessionId}
              onChange={(e) => {
                setTransferSession(e.target.value || null);
                setRemotePath("");
              }}
            >
              {sshSessions.length === 0 && mountTargets.length === 0 && (
                <option value="">{t("transfer.noSession")}</option>
              )}
              {sshSessions.length > 0 && (
                <optgroup label={t("transfer.sshSessions")}>
                  {sshSessions.map((session) => (
                    <option key={session.id} value={session.id}>
                      {session.title}
                    </option>
                  ))}
                </optgroup>
              )}
              {mountTargets.length > 0 && (
                <optgroup label={t("transfer.mounts")}>
                  {mountTargets.map((target) => (
                    <option key={target.id} value={target.id}>
                      {target.title}
                    </option>
                  ))}
                </optgroup>
              )}
            </select>
            <button type="button" onClick={() => setMountOpen(true)}>
              + {t("transfer.addMount")}
            </button>
            <button
              type="button"
              disabled={!canSync}
              onClick={() => setSyncDirection("upload")}
            >
              ↑ {t("transfer.syncUpload")}
            </button>
            <button
              type="button"
              disabled={!canSync}
              onClick={() => setSyncDirection("download")}
            >
              ↓ {t("transfer.syncDownload")}
            </button>
            <button
              type="button"
              className="icon-btn"
              aria-label={t("common.close")}
              title={t("common.close")}
              onClick={closeTransfer}
            >
              ×
            </button>
          </div>
        </header>
        {error && (
          <p className="error-text transfer-error-bar" role="alert">
            {error}
          </p>
        )}
        <div className="transfer-panes">
          <FilePane
            title={t("transfer.local")}
            sessionId={null}
            path={localPath}
            onPathChange={setLocalPath}
            showHidden={localHidden}
            onToggleHidden={() => setLocalHidden((value) => !value)}
            bookmarks={localBookmarks}
            onAddBookmark={(path) => addBookmark("local", path)}
            onRemoveBookmark={(path) => removeBookmark("local", path)}
            onSend={sendToRemote}
            sendLabel={t("transfer.toRemote")}
            onEdit={(entry) => setEditing({ sessionId: null, path: entry.path })}
            onPermissions={(entry) =>
              setPermissionTarget({ sessionId: null, entry })
            }
            onError={setError}
            refreshToken={refreshToken}
          />
          <FilePane
            title={t("transfer.remote")}
            sessionId={sessionId || null}
            path={remotePath}
            onPathChange={setRemotePath}
            showHidden={remoteHidden}
            onToggleHidden={() => setRemoteHidden((value) => !value)}
            bookmarks={remoteBookmarks}
            onAddBookmark={(path) => addBookmark("remote", path)}
            onRemoveBookmark={(path) => removeBookmark("remote", path)}
            onSend={sendToLocal}
            sendLabel={t("transfer.toLocal")}
            onEdit={(entry) =>
              setEditing({ sessionId: sessionId || null, path: entry.path })
            }
            onPermissions={(entry) =>
              setPermissionTarget({ sessionId: sessionId || null, entry })
            }
            onError={setError}
            refreshToken={refreshToken}
          />
        </div>
        <TransferQueue jobs={jobs} onChanged={applyQueue} onError={setError} />
      </div>
      {editing && (
        <FileEditor
          sessionId={editing.sessionId}
          path={editing.path}
          onClose={() => setEditing(null)}
          onSaved={refresh}
        />
      )}
      {permissionTarget && (
        <PermissionsDialog
          sessionId={permissionTarget.sessionId}
          entry={permissionTarget.entry}
          onClose={() => setPermissionTarget(null)}
          onSaved={refresh}
          onError={setError}
        />
      )}
      {mountOpen && (
        <MountDialog
          mounts={mounts}
          onClose={() => setMountOpen(false)}
          onChanged={loadMounts}
        />
      )}
      {syncDirection && (
        <SyncDialog
          direction={syncDirection}
          localPath={localPath}
          remoteSession={sessionId}
          remotePath={remotePath}
          onClose={() => setSyncDirection(null)}
          onApplied={() => {
            refresh();
            applyQueue();
          }}
          onError={setError}
        />
      )}
    </div>
  );
}
