import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { api, isTauri, type HostEntry, type HostInput } from "../api";
import { writeClipboardText } from "../clipboard";
import { planHostConnect } from "../connect";
import { HostIcon } from "../host-icons";
import { useAppStore } from "../store";
import { ConnectHostDialog } from "./ConnectHostDialog";
import { ContextMenu, type MenuItem } from "./ContextMenu";
import { HostDialog } from "./HostDialog";
import { ImportDialog } from "./ImportDialog";
import { Modal } from "./Modal";
import { TagChip } from "./TagChip";
import { TagManagerDialog } from "./TagManagerDialog";
import {
  Chevron,
  FilesIcon,
  FolderIcon,
  FolderPlusIcon,
  GearIcon,
  PlayIcon,
  PlusIcon,
  SearchIcon,
  TagIcon,
} from "./icons";
import { EmptyState, IconButton, PanelHeader, SectionHeader } from "./ui";

interface DialogState {
  kind: "host" | "folder";
  host?: HostEntry;
  parentId: string | null;
}

interface MenuState {
  x: number;
  y: number;
  node?: HostEntry;
}

function toInput(node: HostEntry, parentId: string | null): HostInput {
  return {
    id: node.id,
    name: node.name,
    host: node.host,
    port: node.port,
    username: node.username,
    authMethod: node.authMethod,
    keyPath: node.keyPath,
    keyId: node.keyId,
    passwordId: node.passwordId,
    jumpHostId: node.jumpHostId,
    x11: node.x11,
    parentId,
    isFolder: node.isFolder,
    production: node.production,
    protocol: node.protocol,
    protocolOptions: node.protocolOptions,
    icon: node.icon,
    notes: node.notes,
    tagIds: node.tags.map((tag) => tag.id),
  };
}

function hostMeta(node: HostEntry): string {
  const options = node.protocolOptions;
  switch (node.protocol) {
    case "telnet":
      return `telnet · ${node.host}:${node.port}`;
    case "serial":
      return [options.serialPath, options.baudRate].filter(Boolean).join(" · ");
    case "local":
      return options.shell;
    default:
      return `${node.username}@${node.host}:${node.port}`;
  }
}

function hostAddress(node: HostEntry): string {
  switch (node.protocol) {
    case "telnet":
      return `telnet://${node.host}:${node.port}`;
    case "serial":
      return node.protocolOptions.serialPath;
    case "local":
      return node.protocolOptions.shell || node.name;
    default:
      return `${node.username}@${node.host}:${node.port}`;
  }
}

function Highlight({ text, query }: { text: string; query: string }) {
  if (!query) return <>{text}</>;
  const index = text.toLowerCase().indexOf(query);
  if (index < 0) return <>{text}</>;
  return (
    <>
      {text.slice(0, index)}
      <mark>{text.slice(index, index + query.length)}</mark>
      {text.slice(index + query.length)}
    </>
  );
}

export function HostTree() {
  const { t } = useTranslation();
  const hosts = useAppStore((s) => s.hosts);
  const tags = useAppStore((s) => s.tags);
  const hostFilter = useAppStore((s) => s.hostFilter);
  const collapsedFolders = useAppStore((s) => s.collapsedFolders);
  const selectedHostId = useAppStore((s) => s.selectedHostId);
  const newWindowHostId = useAppStore((s) => s.newWindowHostId);
  const connectRequestId = useAppStore((s) => s.connectRequestId);
  const importRequested = useAppStore((s) => s.importRequested);
  const tagsRequested = useAppStore((s) => s.tagsRequested);
  const newHostRequested = useAppStore((s) => s.newHostRequested);
  const newFolderRequested = useAppStore((s) => s.newFolderRequested);
  const clearConnectRequest = useAppStore((s) => s.clearConnectRequest);
  const clearImportRequest = useAppStore((s) => s.clearImportRequest);
  const clearTagsRequest = useAppStore((s) => s.clearTagsRequest);
  const clearNewHostRequest = useAppStore((s) => s.clearNewHostRequest);
  const clearNewFolderRequest = useAppStore((s) => s.clearNewFolderRequest);
  const updateHost = useAppStore((s) => s.updateHost);
  const deleteHost = useAppStore((s) => s.deleteHost);
  const loadHosts = useAppStore((s) => s.loadHosts);
  const setStatus = useAppStore((s) => s.setStatus);
  const setHostFilter = useAppStore((s) => s.setHostFilter);
  const toggleFolder = useAppStore((s) => s.toggleFolder);
  const selectHost = useAppStore((s) => s.selectHost);
  const clearNewWindowHost = useAppStore((s) => s.clearNewWindowHost);
  const openSession = useAppStore((s) => s.openSession);
  const keepaliveSecs = useAppStore((s) => s.keepaliveSecs);

  const [dialog, setDialog] = useState<DialogState | null>(null);
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [connectTarget, setConnectTarget] = useState<HostEntry | null>(null);
  const [tagManagerOpen, setTagManagerOpen] = useState(false);
  const [importParent, setImportParent] = useState<string | null>(null);
  const [importOpen, setImportOpen] = useState(false);
  const [pendingDelete, setPendingDelete] = useState<HostEntry | null>(null);
  const [dragId, setDragId] = useState<string | null>(null);
  const [dropTarget, setDropTarget] = useState<string | null>(null);
  const [treeCollapsed, setTreeCollapsed] = useState(false);

  const connectRetry = useAppStore((s) => s.connectRetry);
  const [connectError, setConnectError] = useState<string | null>(null);

  // 认证失败后由 TerminalPanel 通过 connectRetry 重新弹窗并带错误信息
  useEffect(() => {
    if (!connectRetry) return;
    const host = hosts.find((item) => item.id === connectRetry.hostId);
    if (host) {
      setConnectTarget(host);
      setConnectError(connectRetry.error);
    }
    useAppStore.getState().clearConnectRetry();
  }, [connectRetry, hosts]);

  // SSH-family hosts: 已保存凭据且保险库未锁时直连（不再询问）；否则走凭据弹窗
  const connectTo = useCallback(
    async (node: HostEntry) => {
      selectHost(node.id);
      if (node.protocol === "ssh" || node.protocol === "sftp") {
        let skipDialog = false;
        try {
          const vault = await api.vaultStatus();
          const locked = vault.mode === "master" && vault.locked;
          if (!locked) {
            const status = await api.credentialStatus(
              node.id,
              node.keyId || undefined,
              node.passwordId || undefined,
            );
            const keyAuth = node.authMethod === "key";
            const agentAuth = node.authMethod === "agent";
            const keyboardAuth = node.authMethod === "keyboard-interactive";
            if (agentAuth) {
              skipDialog = true;
            } else if (keyboardAuth) {
              skipDialog = false;
            } else if (keyAuth) {
              const keys = await api.sshKeyList();
              const managedKey = keys.find((item) => item.id === node.keyId);
              const passphraseSaved = node.keyId
                ? Boolean(status.keyPassphrase)
                : Boolean(status.passphrase);
              const needsPassphrase = Boolean(managedKey?.hasPassphrase);
              skipDialog = !needsPassphrase || passphraseSaved;
            } else {
              skipDialog = Boolean(status.password);
            }
          }
        } catch {
          skipDialog = false;
        }
        if (skipDialog) {
          const plan = planHostConnect(node, hosts, keepaliveSecs);
          const title = node.name || `${node.host}:${node.port}`;
          openSession(
            plan.kind,
            title,
            plan.pending,
            node.production,
            node.id,
          );
          return;
        }
        setConnectTarget(node);
        return;
      }
      const plan = planHostConnect(node, hosts, keepaliveSecs);
      const title =
        node.name ||
        (node.protocol === "local"
          ? t("session.localTitle")
          : node.protocol === "serial"
            ? node.protocolOptions.serialPath
            : `${node.host}:${node.port}`);
      openSession(
        plan.kind,
        title,
        plan.pending,
        node.production,
        node.id,
      );
    },
    [hosts, keepaliveSecs, openSession, selectHost, t],
  );

  useEffect(() => {
    if (!newWindowHostId) return;
    const host = hosts.find((entry) => entry.id === newWindowHostId && !entry.isFolder);
    if (host) {
      connectTo(host);
      clearNewWindowHost();
    } else if (hosts.length > 0) {
      clearNewWindowHost();
    }
  }, [newWindowHostId, hosts, connectTo, clearNewWindowHost]);

  useEffect(() => {
    if (!connectRequestId) return;
    const host = hosts.find(
      (entry) => entry.id === connectRequestId && !entry.isFolder,
    );
    if (host) connectTo(host);
    clearConnectRequest();
  }, [connectRequestId, hosts, connectTo, clearConnectRequest]);

  useEffect(() => {
    if (!importRequested) return;
    setImportParent(null);
    setImportOpen(true);
    clearImportRequest();
  }, [importRequested, clearImportRequest]);

  useEffect(() => {
    if (!tagsRequested) return;
    setTagManagerOpen(true);
    clearTagsRequest();
  }, [tagsRequested, clearTagsRequest]);

  useEffect(() => {
    if (!newHostRequested) return;
    setDialog({ kind: "host", parentId: null });
    clearNewHostRequest();
  }, [newHostRequested, clearNewHostRequest]);

  useEffect(() => {
    if (!newFolderRequested) return;
    setDialog({ kind: "folder", parentId: null });
    clearNewFolderRequest();
  }, [newFolderRequested, clearNewFolderRequest]);

  const query = hostFilter.trim().toLowerCase();

  const byParent = useMemo(() => {
    const map = new Map<string | null, HostEntry[]>();
    for (const host of hosts) {
      const key = host.parentId ?? null;
      const list = map.get(key);
      if (list) list.push(host);
      else map.set(key, [host]);
    }
    for (const list of map.values()) {
      list.sort(
        (a, b) =>
          Number(b.isFolder) - Number(a.isFolder) || a.name.localeCompare(b.name),
      );
    }
    return map;
  }, [hosts]);

  const hostCount = useMemo(() => hosts.filter((h) => !h.isFolder).length, [hosts]);

  const countHosts = (parentId: string | null): number => {
    let total = 0;
    for (const child of byParent.get(parentId) ?? []) {
      total += child.isFolder ? countHosts(child.id) : 1;
    }
    return total;
  };

  const matches = (host: HostEntry) =>
    !query ||
    [host.name, host.host, host.username, ...host.tags.map((tag) => tag.name)].some(
      (value) => value.toLowerCase().includes(query),
    );

  const visible = useMemo(() => {
    if (!query) return null;
    const cache = new Map<string, boolean>();
    const visit = (host: HostEntry): boolean => {
      const cached = cache.get(host.id);
      if (cached !== undefined) return cached;
      let hit = matches(host);
      if (host.isFolder) {
        for (const child of byParent.get(host.id) ?? []) {
          hit = visit(child) || hit;
        }
      }
      cache.set(host.id, hit);
      return hit;
    };
    for (const host of hosts) visit(host);
    return cache;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hosts, byParent, query]);

  const moveNode = async (id: string | null, parentId: string | null) => {
    if (!id) return;
    const node = hosts.find((host) => host.id === id);
    if (!node || node.id === parentId || node.parentId === parentId) return;
    try {
      await updateHost(toInput(node, parentId));
    } catch (err) {
      setStatus(String(err), true);
    }
  };

  const toggleHostTag = async (node: HostEntry, tagId: string) => {
    const assigned = node.tags.some((tag) => tag.id === tagId);
    const tagIds = assigned
      ? node.tags.filter((tag) => tag.id !== tagId).map((tag) => tag.id)
      : [...node.tags.map((tag) => tag.id), tagId];
    try {
      await updateHost({ ...toInput(node, node.parentId), tagIds });
    } catch (err) {
      setStatus(String(err), true);
    }
  };

  const copyAddress = (node: HostEntry) => {
    writeClipboardText(hostAddress(node))
      .then(() => setStatus(t("hosts.copied")))
      .catch((err) => setStatus(String(err), true));
  };

  const refreshHosts = () => {
    loadHosts().catch((err) => setStatus(String(err), true));
  };

  const expandAll = () => useAppStore.setState({ collapsedFolders: {} });

  const collapseAll = () =>
    useAppStore.setState({
      collapsedFolders: Object.fromEntries(
        hosts.filter((host) => host.isFolder).map((host) => [host.id, true]),
      ),
    });

  const openInNewWindow = async (node: HostEntry) => {
    if (!isTauri) return;
    const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
    const popup = new WebviewWindow(`session-${node.id}-${Date.now()}`, {
      url: `?connect=${encodeURIComponent(node.id)}`,
      title: node.name || node.host,
      width: 1000,
      height: 700,
      center: true,
    });
    popup.once("tauri://error", (event) =>
      setStatus(String(event.payload), true),
    );
  };

  const openImport = (parentId: string | null) => {
    setImportParent(parentId);
    setImportOpen(true);
  };

  const menuItems = (): MenuItem[] => {
    if (!menu) return [];
    const node = menu.node;
    const treeActions: MenuItem[] = [
      { label: t("hosts.refresh"), onSelect: refreshHosts },
      { label: t("hosts.expandAll"), onSelect: expandAll },
      { label: t("hosts.collapseAll"), onSelect: collapseAll },
    ];
    if (!node) {
      return [
        { label: t("hosts.newHost"), onSelect: () => setDialog({ kind: "host", parentId: null }) },
        {
          label: t("hosts.newFolder"),
          onSelect: () => setDialog({ kind: "folder", parentId: null }),
        },
        { label: t("hosts.import"), onSelect: () => openImport(null) },
        { label: t("hosts.manageTags"), onSelect: () => setTagManagerOpen(true) },
        ...treeActions,
      ];
    }
    if (node.isFolder) {
      return [
        {
          label: t("hosts.newHost"),
          onSelect: () => setDialog({ kind: "host", parentId: node.id }),
        },
        {
          label: t("hosts.newFolder"),
          onSelect: () => setDialog({ kind: "folder", parentId: node.id }),
        },
        {
          label: t("hosts.rename"),
          onSelect: () => setDialog({ kind: "folder", host: node, parentId: node.parentId }),
        },
        { label: t("hosts.import"), onSelect: () => openImport(node.id) },
        ...treeActions,
        {
          label: t("hosts.delete"),
          danger: true,
          onSelect: () => setPendingDelete(node),
        },
      ];
    }
    return [
      { label: t("hosts.connect"), onSelect: () => connectTo(node) },
      { label: t("hosts.newWindow"), onSelect: () => void openInNewWindow(node) },
      { label: t("hosts.copy"), onSelect: () => copyAddress(node) },
      {
        label: t("hosts.rename"),
        onSelect: () => setDialog({ kind: "host", host: node, parentId: node.parentId }),
      },
      {
        label: t("hosts.tags"),
        children: [
          ...tags.map((tag) => ({
            label: tag.name,
            checked: node.tags.some((assigned) => assigned.id === tag.id),
            onSelect: () => void toggleHostTag(node, tag.id),
          })),
          {
            label: t("hosts.manageTags"),
            onSelect: () => setTagManagerOpen(true),
          },
        ],
      },
      {
        label: t("hosts.properties"),
        onSelect: () => setDialog({ kind: "host", host: node, parentId: node.parentId }),
      },
      { label: t("hosts.refresh"), onSelect: refreshHosts },
      {
        label: t("hosts.delete"),
        danger: true,
        onSelect: () => setPendingDelete(node),
      },
    ];
  };

  const renderNodes = (parentId: string | null, depth: number): ReactNode[] =>
    (byParent.get(parentId) ?? [])
      .filter((node) => !visible || visible.get(node.id))
      .map((node) => {
        const open = node.isFolder && (query ? true : !collapsedFolders[node.id]);
        const selected = node.id === selectedHostId;
        const rowClass = [
          "tree-row",
          node.isFolder ? "" : "two-line",
          selected ? "selected" : "",
          dropTarget === node.id ? "drop" : "",
        ]
          .filter(Boolean)
          .join(" ");
        return (
          <div className="tree-branch" key={node.id}>
            <div
              className={rowClass}
              role="treeitem"
              data-testid="host-row"
              data-host-name={node.name}
              aria-expanded={node.isFolder ? open : undefined}
              aria-selected={selected}
              tabIndex={0}
              style={{
                paddingLeft: `calc(${depth} * var(--space-4) + var(--space-2))`,
              }}
              title={node.isFolder ? node.name : hostMeta(node)}
              onClick={() => {
                selectHost(node.id);
                if (node.isFolder) toggleFolder(node.id);
              }}
              onDoubleClick={() => {
                if (!node.isFolder) connectTo(node);
              }}
              onContextMenu={(e) => {
                e.preventDefault();
                selectHost(node.id);
                setMenu({ x: e.clientX, y: e.clientY, node });
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  if (node.isFolder) toggleFolder(node.id);
                  else connectTo(node);
                } else if (e.key === "F2") {
                  setDialog({
                    kind: node.isFolder ? "folder" : "host",
                    host: node,
                    parentId: node.parentId,
                  });
                } else if (e.key === "Delete") {
                  setPendingDelete(node);
                } else if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) {
                  e.preventDefault();
                  const rect = e.currentTarget.getBoundingClientRect();
                  setMenu({ x: rect.left + 16, y: rect.bottom, node });
                }
              }}
              draggable
              onDragStart={(e) => {
                e.dataTransfer.setData("text/plain", node.id);
                e.dataTransfer.effectAllowed = "move";
                setDragId(node.id);
              }}
              onDragEnd={() => {
                setDragId(null);
                setDropTarget(null);
              }}
              onDragOver={
                node.isFolder
                  ? (e) => {
                      if (dragId && dragId !== node.id) {
                        e.preventDefault();
                        setDropTarget(node.id);
                      }
                    }
                  : undefined
              }
              onDragLeave={
                node.isFolder
                  ? () => {
                      if (dropTarget === node.id) setDropTarget(null);
                    }
                  : undefined
              }
              onDrop={
                node.isFolder
                  ? (e) => {
                      e.preventDefault();
                      e.stopPropagation();
                      void moveNode(dragId, node.id);
                      setDragId(null);
                      setDropTarget(null);
                    }
                  : undefined
              }
            >
              <span className="tree-row-line">
                {node.isFolder ? (
                  <Chevron open={open} />
                ) : (
                  <span className="tree-caret-spacer" />
                )}
                {node.isFolder ? (
                  <FolderIcon />
                ) : (
                  <HostIcon icon={node.icon} protocol={node.protocol} />
                )}
                {node.production && !node.isFolder && (
                  <span className="prod-dot" title={t("hosts.production")} />
                )}
                <span className="tree-name">
                  <Highlight text={node.name} query={query} />
                </span>
                {node.isFolder && (
                  <span className="tree-count">({countHosts(node.id)})</span>
                )}
                <span className="tree-tags">
                  {node.tags.map((tag) => (
                    <TagChip key={tag.id} tag={tag} small />
                  ))}
                </span>
                {!node.isFolder && (
                  <span className="tree-row-actions">
                    <IconButton
                      label={t("hosts.connect")}
                      size="sm"
                      data-testid="host-connect"
                      disabled={!isTauri}
                      onClick={(e) => {
                        e.stopPropagation();
                        connectTo(node);
                      }}
                    >
                      <PlayIcon />
                    </IconButton>
                    <IconButton
                      label={t("hosts.copy")}
                      size="sm"
                      onClick={(e) => {
                        e.stopPropagation();
                        copyAddress(node);
                      }}
                    >
                      <FilesIcon />
                    </IconButton>
                    <IconButton
                      label={t("hosts.properties")}
                      size="sm"
                      onClick={(e) => {
                        e.stopPropagation();
                        setDialog({
                          kind: "host",
                          host: node,
                          parentId: node.parentId,
                        });
                      }}
                    >
                      <GearIcon />
                    </IconButton>
                  </span>
                )}
              </span>
              {!node.isFolder && hostMeta(node) && (
                <span className="tree-row-meta">{hostMeta(node)}</span>
              )}
            </div>
            {node.isFolder && open && renderNodes(node.id, depth + 1)}
          </div>
        );
      });

  const noMatches = visible !== null && hosts.length > 0 && ![...visible.values()].some(Boolean);

  return (
    <section className="host-section">
      <PanelHeader
        dragRegion
        title={
          <>
            {t("hosts.title")}
            {hostCount > 0 && <span className="head-count"> ({hostCount})</span>}
          </>
        }
        actions={
          <>
            <IconButton
              label={t("hosts.newHost")}
              size="sm"
              disabled={!isTauri}
              onClick={() => setDialog({ kind: "host", parentId: null })}
            >
              <PlusIcon />
            </IconButton>
            <IconButton
              label={t("hosts.newFolder")}
              size="sm"
              disabled={!isTauri}
              onClick={() => setDialog({ kind: "folder", parentId: null })}
            >
              <FolderPlusIcon />
            </IconButton>
            <IconButton
              label={t("hosts.manageTags")}
              size="sm"
              disabled={!isTauri}
              onClick={() => setTagManagerOpen(true)}
            >
              <TagIcon />
            </IconButton>
          </>
        }
      />

      <label className="search-field">
        <SearchIcon />
        <input
          value={hostFilter}
          onChange={(e) => setHostFilter(e.target.value)}
          placeholder={t("hosts.searchPlaceholder")}
          aria-label={t("hosts.searchPlaceholder")}
          autoComplete="off"
          spellCheck={false}
        />
      </label>

      <SectionHeader
        title={t("hosts.sectionAll")}
        count={hostCount}
        collapsed={treeCollapsed}
        onToggle={() => setTreeCollapsed((value) => !value)}
      />

      <div
        className={`tree-scroll${dropTarget === "__root__" ? " drop-root" : ""}`}
        role="tree"
        hidden={treeCollapsed}
        aria-label={t("hosts.title")}
        onContextMenu={(e) => {
          if (e.target === e.currentTarget) {
            e.preventDefault();
            setMenu({ x: e.clientX, y: e.clientY });
          }
        }}
        onDragOver={(e) => {
          if (dragId && e.target === e.currentTarget) {
            e.preventDefault();
            setDropTarget("__root__");
          }
        }}
        onDragLeave={(e) => {
          if (e.target === e.currentTarget && dropTarget === "__root__") {
            setDropTarget(null);
          }
        }}
        onDrop={(e) => {
          if (dragId && e.target === e.currentTarget) {
            e.preventDefault();
            void moveNode(dragId, null);
            setDragId(null);
            setDropTarget(null);
          }
        }}
      >
        {hosts.length === 0 ? (
          <EmptyState
            title={t("hosts.empty")}
            description={t("hosts.emptyHint")}
            action={
              <div className="tree-empty-actions">
                <button
                  type="button"
                  disabled={!isTauri}
                  onClick={() => setDialog({ kind: "host", parentId: null })}
                >
                  {t("hosts.newHost")}
                </button>
                <button
                  type="button"
                  disabled={!isTauri}
                  onClick={() => openImport(null)}
                >
                  {t("hosts.import")}
                </button>
              </div>
            }
          />
        ) : noMatches ? (
          <p className="tree-empty">{t("hosts.noMatches")}</p>
        ) : (
          renderNodes(null, 0)
        )}
      </div>

      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          items={menuItems()}
          onClose={() => setMenu(null)}
        />
      )}

      {dialog && (
        <HostDialog
          host={dialog.host}
          parentId={dialog.parentId}
          isFolder={dialog.kind === "folder"}
          onClose={() => setDialog(null)}
        />
      )}

      {connectTarget && (
        <ConnectHostDialog
          host={connectTarget}
          initialError={connectError ?? undefined}
          onClose={() => {
            setConnectTarget(null);
            setConnectError(null);
          }}
        />
      )}

      {tagManagerOpen && <TagManagerDialog onClose={() => setTagManagerOpen(false)} />}

      {importOpen && (
        <ImportDialog parentId={importParent} onClose={() => setImportOpen(false)} />
      )}

      {pendingDelete && (
        <Modal
          title={t("hosts.delete")}
          onClose={() => setPendingDelete(null)}
          footer={
            <>
              <button type="button" onClick={() => setPendingDelete(null)}>
                {t("common.cancel")}
              </button>
              <button
                type="button"
                className="danger"
                onClick={() => {
                  deleteHost(pendingDelete.id).catch((err) =>
                    setStatus(String(err), true),
                  );
                  setPendingDelete(null);
                }}
              >
                {t("common.delete")}
              </button>
            </>
          }
        >
          <p>
            {t(
              pendingDelete.isFolder
                ? "hosts.deleteFolderConfirm"
                : "hosts.deleteHostConfirm",
              { name: pendingDelete.name },
            )}
          </p>
        </Modal>
      )}
    </section>
  );
}
