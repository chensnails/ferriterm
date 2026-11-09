import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, isTauri, type RemoteMountInfo, type VfsEntry } from "../api";
import { useAppStore } from "../store";
import { ContextMenu, type MenuItem } from "./ContextMenu";
import { FileEditor } from "./FileEditor";
import { FilePane } from "./FilePane";
import { MountDialog } from "./MountDialog";
import { PermissionsDialog } from "./PermissionsDialog";
import { SyncDialog, type SyncDirection } from "./SyncDialog";
import { TransferQueue } from "./TransferQueue";
import { TransferTabs } from "./TransferTabs";

const LOCAL_TARGET = "local";

/**
 * Two sessions of the same host share a title, so a bare title would give the
 * user two identical tabs. Number only the labels that actually repeat.
 */
function disambiguate(
  ids: string[],
  base: (id: string) => string,
): Map<string, string> {
  const totals = new Map<string, number>();
  for (const id of ids) {
    const key = base(id);
    totals.set(key, (totals.get(key) ?? 0) + 1);
  }
  const seen = new Map<string, number>();
  const out = new Map<string, string>();
  for (const id of ids) {
    const key = base(id);
    if ((totals.get(key) ?? 0) <= 1) {
      out.set(id, key);
      continue;
    }
    const index = (seen.get(key) ?? 0) + 1;
    seen.set(key, index);
    out.set(id, `${key} (${index})`);
  }
  return out;
}

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
  const openTransferTab = useAppStore((s) => s.openTransferTab);
  const closeTransferTab = useAppStore((s) => s.closeTransferTab);
  const storedTabs = useAppStore((s) => s.transferTabs);
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

  const labels = useMemo(() => {
    const map = new Map<string, string>();
    for (const session of sshSessions) map.set(session.id, session.title);
    for (const target of mountTargets) map.set(target.id, target.title);
    return map;
  }, [sshSessions, mountTargets]);
  const labelOf = useCallback(
    (id: string) => labels.get(id) ?? id,
    [labels],
  );

  const bound =
    boundSession && knownTargets.has(boundSession) ? boundSession : null;
  // Tabs are the stored list minus dead targets, plus whatever the caller
  // bound explicitly, so opening the view with a session always shows a tab.
  const tabs = useMemo(() => {
    const open = storedTabs.filter((id) => knownTargets.has(id));
    if (bound && !open.includes(bound)) open.push(bound);
    if (open.length === 0) {
      const fallback =
        sshSessions.find((session) => session.id === activeId)?.id ??
        sshSessions[0]?.id ??
        mountTargets[0]?.id;
      if (fallback) open.push(fallback);
    }
    return open;
  }, [storedTabs, knownTargets, bound, sshSessions, activeId, mountTargets]);

  const sessionId = bound ?? tabs[0] ?? "";

  const [localPath, setLocalPath] = useState("");
  /** Each remote tab remembers the folder it was left in. */
  const [remotePaths, setRemotePaths] = useState<Record<string, string>>({});
  const remotePath = sessionId ? (remotePaths[sessionId] ?? "") : "";
  const setRemotePath = useCallback(
    (next: string) => {
      if (!sessionId) return;
      setRemotePaths((current) => ({ ...current, [sessionId]: next }));
    },
    [sessionId],
  );
  const [copyTo, setCopyTo] = useState<string>(LOCAL_TARGET);
  const [localHidden, setLocalHidden] = useState(
    () => useAppStore.getState().sftpShowHidden,
  );
  const [remoteHidden, setRemoteHidden] = useState(
    () => useAppStore.getState().sftpShowHidden,
  );
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
  const [picker, setPicker] = useState<{ x: number; y: number } | null>(null);

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

  // A destination that collapsed with its tab (or turned into the pane's own
  // target) would silently copy onto itself.
  useEffect(() => {
    if (copyTo === LOCAL_TARGET) return;
    if (copyTo === sessionId || !tabs.includes(copyTo)) {
      setCopyTo(LOCAL_TARGET);
    }
  }, [copyTo, sessionId, tabs]);

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

  /** Remote pane → local disk, or remote pane → another remote tab. */
  const sendFromRemote = async (entry: VfsEntry) => {
    if (!sessionId) {
      setError(t("transfer.chooseSession"));
      return;
    }
    if (copyTo === LOCAL_TARGET) {
      if (!localPath) {
        setError(t("transfer.chooseSession"));
        return;
      }
      await enqueue({
        id: crypto.randomUUID(),
        sourceSession: sessionId,
        sourcePath: entry.path,
        targetSession: null,
        targetPath: joinPath(localPath, entry.name),
        directory: entry.isDir,
      });
      return;
    }
    let destination = remotePaths[copyTo] ?? "";
    if (!destination) {
      try {
        destination = await api.vfsHome(copyTo);
      } catch (err) {
        setError(String(err));
        return;
      }
    }
    await enqueue({
      id: crypto.randomUUID(),
      sourceSession: sessionId,
      sourcePath: entry.path,
      targetSession: copyTo,
      targetPath: joinPath(destination, entry.name),
      directory: entry.isDir,
    });
  };

  const applyQueue = () => {
    void api
      .transferList()
      .then(setTransfers)
      .catch((err) => setError(String(err)));
  };

  const availableTargets = useMemo(
    () => [
      ...sshSessions.map((session) => ({
        id: session.id,
        title: session.title,
      })),
      ...mountTargets,
    ],
    [sshSessions, mountTargets],
  );

  const tabLabels = useMemo(
    () => disambiguate(tabs, labelOf),
    [tabs, labelOf],
  );
  const tabLabelOf = useCallback(
    (id: string) => tabLabels.get(id) ?? labelOf(id),
    [tabLabels, labelOf],
  );
  const pickerLabels = useMemo(
    () => disambiguate(availableTargets.map((target) => target.id), labelOf),
    [availableTargets, labelOf],
  );

  const pickerItems = useMemo<MenuItem[]>(() => {
    const rest = availableTargets.filter((target) => !tabs.includes(target.id));
    if (rest.length === 0) {
      return [{ label: t("transfer.noOtherTargets"), disabled: true }];
    }
    return rest.map((target) => ({
      label: pickerLabels.get(target.id) ?? target.title,
      onSelect: () => openTransferTab(target.id),
    }));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [availableTargets, pickerLabels, tabs, t]);

  const otherTabs = tabs.filter((id) => id !== sessionId);
  const copyLabel =
    copyTo === LOCAL_TARGET ? t("transfer.local") : tabLabelOf(copyTo);
  const sendLabel =
    copyTo === LOCAL_TARGET
      ? t("transfer.toLocal")
      : t("transfer.toTarget", { target: copyLabel });
  const canSync = Boolean(sessionId && localPath && remotePath);

  return (
    <div className="transfer-view">
      <div className="transfer-panel">
        <header className="transfer-head">
          <h3>{t("transfer.title")}</h3>
          <div className="transfer-head-tools">
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
              disabled={!canSync}
              onClick={() => setSyncDirection("twoway")}
            >
              ⇅ {t("transfer.syncTwoWay")}
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
          <div className="transfer-remote">
            <TransferTabs
              tabs={tabs}
              activeId={sessionId}
              labelOf={tabLabelOf}
              onSelect={setTransferSession}
              onClose={closeTransferTab}
              onOpenPicker={(rect) =>
                setPicker({ x: rect.left, y: rect.bottom + 2 })
              }
              copyTo={copyTo}
              copyTargets={otherTabs}
              onCopyToChange={setCopyTo}
              labels={{
                local: t("transfer.local"),
                copyTo: t("transfer.copyTo"),
                open: t("transfer.openTab"),
                close: t("transfer.closeTab"),
                empty: t("transfer.noSession"),
              }}
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
              onSend={(entry) => void sendFromRemote(entry)}
              sendLabel={sendLabel}
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
        </div>
        <TransferQueue jobs={jobs} onChanged={applyQueue} onError={setError} />
      </div>
      {picker && (
        <ContextMenu
          x={picker.x}
          y={picker.y}
          items={pickerItems}
          onClose={() => setPicker(null)}
        />
      )}
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
