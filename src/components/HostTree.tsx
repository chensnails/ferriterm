import { useEffect, useMemo, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { isTauri, type HostEntry, type HostInput } from "../api";
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
  FolderIcon,
  FolderPlusIcon,
  PlusIcon,
  SearchIcon,
  TagIcon,
} from "./icons";

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
    jumpHostId: node.jumpHostId,
    x11: node.x11,
    parentId,
    isFolder: node.isFolder,
    production: node.production,
    notes: node.notes,
    tagIds: node.tags.map((tag) => tag.id),
  };
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
  const clearConnectRequest = useAppStore((s) => s.clearConnectRequest);
  const clearImportRequest = useAppStore((s) => s.clearImportRequest);
  const clearTagsRequest = useAppStore((s) => s.clearTagsRequest);
  const updateHost = useAppStore((s) => s.updateHost);
  const deleteHost = useAppStore((s) => s.deleteHost);
  const setStatus = useAppStore((s) => s.setStatus);
  const setHostFilter = useAppStore((s) => s.setHostFilter);
  const toggleFolder = useAppStore((s) => s.toggleFolder);
  const selectHost = useAppStore((s) => s.selectHost);
  const clearNewWindowHost = useAppStore((s) => s.clearNewWindowHost);

  const [dialog, setDialog] = useState<DialogState | null>(null);
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [connectTarget, setConnectTarget] = useState<HostEntry | null>(null);
  const [tagManagerOpen, setTagManagerOpen] = useState(false);
  const [importParent, setImportParent] = useState<string | null>(null);
  const [importOpen, setImportOpen] = useState(false);
  const [pendingDelete, setPendingDelete] = useState<HostEntry | null>(null);
  const [dragId, setDragId] = useState<string | null>(null);
  const [dropTarget, setDropTarget] = useState<string | null>(null);

  useEffect(() => {
    if (!newWindowHostId) return;
    const host = hosts.find((entry) => entry.id === newWindowHostId && !entry.isFolder);
    if (host) {
      selectHost(host.id);
      setConnectTarget(host);
      clearNewWindowHost();
    } else if (hosts.length > 0) {
      clearNewWindowHost();
    }
  }, [newWindowHostId, hosts, selectHost, clearNewWindowHost]);

  useEffect(() => {
    if (!connectRequestId) return;
    const host = hosts.find(
      (entry) => entry.id === connectRequestId && !entry.isFolder,
    );
    if (host) {
      selectHost(host.id);
      setConnectTarget(host);
    }
    clearConnectRequest();
  }, [connectRequestId, hosts, selectHost, clearConnectRequest]);

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
    if (!node) {
      return [
        { label: t("hosts.newHost"), onSelect: () => setDialog({ kind: "host", parentId: null }) },
        {
          label: t("hosts.newFolder"),
          onSelect: () => setDialog({ kind: "folder", parentId: null }),
        },
        { label: t("hosts.import"), onSelect: () => openImport(null) },
        { label: t("hosts.manageTags"), onSelect: () => setTagManagerOpen(true) },
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
        {
          label: t("hosts.delete"),
          danger: true,
          onSelect: () => setPendingDelete(node),
        },
      ];
    }
    return [
      { label: t("hosts.connect"), onSelect: () => setConnectTarget(node) },
      { label: t("hosts.newWindow"), onSelect: () => void openInNewWindow(node) },
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
              aria-expanded={node.isFolder ? open : undefined}
              aria-selected={selected}
              tabIndex={0}
              style={{
                paddingLeft: `calc(${depth} * var(--space-4) + var(--space-2))`,
              }}
              title={
                node.isFolder
                  ? node.name
                  : `${node.username}@${node.host}:${node.port}`
              }
              onClick={() => {
                selectHost(node.id);
                if (node.isFolder) toggleFolder(node.id);
              }}
              onDoubleClick={() => {
                if (!node.isFolder) setConnectTarget(node);
              }}
              onContextMenu={(e) => {
                e.preventDefault();
                selectHost(node.id);
                setMenu({ x: e.clientX, y: e.clientY, node });
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  if (node.isFolder) toggleFolder(node.id);
                  else setConnectTarget(node);
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
              {node.isFolder ? (
                <Chevron open={open} />
              ) : (
                <span className="tree-caret-spacer" />
              )}
              {node.isFolder ? (
                <FolderIcon />
              ) : (
                <span className="tree-icon-spacer" />
              )}
              {node.production && !node.isFolder && (
                <span className="prod-dot" title={t("hosts.production")} />
              )}
              <span className="tree-name">
                <Highlight text={node.name} query={query} />
              </span>
              <span className="tree-tags">
                {node.tags.map((tag) => (
                  <TagChip key={tag.id} tag={tag} small />
                ))}
              </span>
            </div>
            {node.isFolder && open && renderNodes(node.id, depth + 1)}
          </div>
        );
      });

  const noMatches = visible !== null && hosts.length > 0 && ![...visible.values()].some(Boolean);

  return (
    <section className="host-section">
      <header className="section-head">
        <h2>{t("hosts.title")}</h2>
        <div className="section-actions">
          <button
            className="icon-btn"
            title={t("hosts.newHost")}
            aria-label={t("hosts.newHost")}
            disabled={!isTauri}
            onClick={() => setDialog({ kind: "host", parentId: null })}
          >
            <PlusIcon />
          </button>
          <button
            className="icon-btn"
            title={t("hosts.newFolder")}
            aria-label={t("hosts.newFolder")}
            disabled={!isTauri}
            onClick={() => setDialog({ kind: "folder", parentId: null })}
          >
            <FolderPlusIcon />
          </button>
          <button
            className="icon-btn"
            title={t("hosts.manageTags")}
            aria-label={t("hosts.manageTags")}
            disabled={!isTauri}
            onClick={() => setTagManagerOpen(true)}
          >
            <TagIcon />
          </button>
        </div>
      </header>

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

      <div
        className={`tree-scroll${dropTarget === "__root__" ? " drop-root" : ""}`}
        role="tree"
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
          <p className="tree-empty">
            {t("hosts.empty")}
            <br />
            <span>{t("hosts.emptyHint")}</span>
          </p>
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
          onClose={() => setConnectTarget(null)}
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
