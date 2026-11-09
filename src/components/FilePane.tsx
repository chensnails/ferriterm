import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type VfsEntry } from "../api";
import { isRootPath, joinPath, parentPath } from "../paths";
import { useAppStore } from "../store";
import { ContextMenu, type MenuItem } from "./ContextMenu";

interface FilePaneProps {
  title: string;
  sessionId: string | null;
  path: string;
  onPathChange: (path: string) => void;
  showHidden: boolean;
  onToggleHidden: () => void;
  bookmarks: string[];
  onAddBookmark: (path: string) => void;
  onRemoveBookmark: (path: string) => void;
  onSend?: (entry: VfsEntry) => void;
  sendLabel?: string;
  onEdit: (entry: VfsEntry) => void;
  onPermissions: (entry: VfsEntry) => void;
  onError: (message: string) => void;
  refreshToken: number;
}

/** The local pane sees Windows paths and the remote pane sees POSIX ones, so
    joins and parent lookups live in `paths.ts` and accept both separators. */
function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(1)} ${units[unit]}`;
}

function formatTime(seconds: number): string {
  if (!seconds) return "—";
  const date = new Date(seconds * 1000);
  if (Number.isNaN(date.getTime())) return "—";
  return date.toLocaleString(undefined, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function permissionString(mode: number): string {
  const flags = [
    [0o400, "r"],
    [0o200, "w"],
    [0o100, "x"],
    [0o040, "r"],
    [0o020, "w"],
    [0o010, "x"],
    [0o004, "r"],
    [0o002, "w"],
    [0o001, "x"],
  ] as const;
  const bits = flags.map(([mask, char]) => (mode & mask ? char : "-"));
  return bits.join("");
}

export function FilePane({
  title,
  sessionId,
  path,
  onPathChange,
  showHidden,
  onToggleHidden,
  bookmarks,
  onAddBookmark,
  onRemoveBookmark,
  onSend,
  sendLabel,
  onEdit,
  onPermissions,
  onError,
  refreshToken,
}: FilePaneProps) {
  const { t } = useTranslation();
  const [entries, setEntries] = useState<VfsEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [input, setInput] = useState(path);
  const [selected, setSelected] = useState<string | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; entry: VfsEntry } | null>(
    null,
  );

  useEffect(() => {
    setInput(path);
  }, [path]);

  useEffect(() => {
    if (path) return;
    let cancelled = false;
    api
      .vfsHome(sessionId)
      .then((home) => {
        if (!cancelled && home) onPathChange(home);
      })
      .catch((err) => onError(String(err)));
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId, path]);

  const load = useCallback(async () => {
    if (!path) return;
    setLoading(true);
    try {
      const list = await api.vfsList(sessionId, path, showHidden);
      setEntries(list);
      setSelected((current) =>
        list.some((entry) => entry.path === current) ? current : null,
      );
    } catch (err) {
      onError(String(err));
      setEntries([]);
    } finally {
      setLoading(false);
    }
  }, [sessionId, path, showHidden, onError]);

  useEffect(() => {
    void load();
  }, [load, refreshToken]);

  const navigate = (target: string) => {
    onPathChange(target);
    setSelected(null);
  };

  const goUp = () => {
    if (isRootPath(path)) return;
    navigate(parentPath(path));
  };

  const refresh = () => void load();

  const makeFolder = async () => {
    const name = await useAppStore.getState().askText({
      title: t("files.newFolder"),
      placeholder: t("files.name"),
    });
    if (!name) return;
    try {
      await api.vfsMkdir(sessionId, joinPath(path, name));
      await load();
    } catch (err) {
      onError(String(err));
    }
  };

  const rename = async (entry: VfsEntry) => {
    const name = await useAppStore.getState().askText({
      title: t("files.rename"),
      placeholder: t("files.name"),
      value: entry.name,
    });
    if (!name || name === entry.name) return;
    const target = joinPath(parentPath(entry.path), name);
    try {
      await api.vfsRename(sessionId, entry.path, target);
      await load();
    } catch (err) {
      onError(String(err));
    }
  };

  const remove = async (entry: VfsEntry) => {
    const confirmed = await useAppStore.getState().askConfirm({
      title: t("files.deleteTitle"),
      body: entry.isDir
        ? t("files.deleteDirConfirm", { name: entry.name })
        : t("files.deleteFileConfirm", { name: entry.name }),
      confirmLabel: t("files.delete"),
      danger: true,
    });
    if (!confirmed) return;
    try {
      await api.vfsRemove(sessionId, entry.path, entry.isDir);
      await load();
    } catch (err) {
      onError(String(err));
    }
  };

  const fastDelete = async (entry: VfsEntry) => {
    const confirmed = await useAppStore.getState().askConfirm({
      title: t("files.fastDeleteTitle"),
      body: t("files.fastDeleteConfirm", { name: entry.name }),
      confirmLabel: t("files.fastDelete"),
      danger: true,
    });
    if (!confirmed) return;
    try {
      await api.vfsRemove(sessionId, entry.path, true);
      await load();
    } catch (err) {
      onError(String(err));
    }
  };

  const copyPath = (entry: VfsEntry) => {
    navigator.clipboard
      .writeText(entry.path)
      .then(() => useAppStore.getState().setStatus(t("files.copied")))
      .catch((err) => onError(String(err)));
  };

  const makeFile = async () => {
    const name = await useAppStore.getState().askText({
      title: t("files.newFile"),
      placeholder: t("files.name"),
    });
    if (!name) return;
    try {
      await api.vfsWriteText(
        sessionId,
        joinPath(path, name),
        "",
        null,
      );
      await load();
    } catch (err) {
      onError(String(err));
    }
  };

  const open = (entry: VfsEntry) => {
    if (entry.isDir) {
      navigate(entry.path);
    } else {
      onEdit(entry);
    }
  };

  const menuItems = useMemo<MenuItem[]>(() => {
    if (!menu) return [];
    const entry = menu.entry;
    const items: MenuItem[] = [];
    if (onSend) {
      items.push({
        label: sendLabel ?? t("files.send", { target: "" }),
        onSelect: () => onSend(entry),
      });
    }
    items.push(
      {
        label: entry.isDir ? t("files.openFolder") : t("files.edit"),
        onSelect: () => open(entry),
      },
      {
        label: t("files.copyPath"),
        onSelect: () => copyPath(entry),
      },
      {
        label: t("files.rename"),
        onSelect: () => void rename(entry),
      },
      {
        label: t("files.delete"),
        danger: true,
        onSelect: () => void remove(entry),
      },
      {
        label: t("files.fastDelete"),
        danger: true,
        onSelect: () => void fastDelete(entry),
      },
      {
        label: t("files.permissions"),
        onSelect: () => onPermissions(entry),
      },
      {
        label: t("files.refresh"),
        onSelect: refresh,
      },
      {
        label: t("files.new"),
        children: [
          { label: t("files.newFile"), onSelect: () => void makeFile() },
          { label: t("files.newFolder"), onSelect: () => void makeFolder() },
        ],
      },
    );
    return items;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [menu, sendLabel, onSend, t]);

  const current = entries.find((entry) => entry.path === selected);

  return (
    <div className="file-pane">
      <div className="file-pane-head">
        <span className="file-pane-title">{title}</span>
        <div className="file-pane-tools">
          <button type="button" className="icon-btn" title={t("files.up")} onClick={goUp}>
            ↑
          </button>
          <button
            type="button"
            className="icon-btn"
            title={t("files.refresh")}
            onClick={refresh}
            disabled={loading}
          >
            ↻
          </button>
          <button
            type="button"
            className={showHidden ? "icon-btn active" : "icon-btn"}
            title={t("files.toggleHidden")}
            aria-pressed={showHidden}
            onClick={onToggleHidden}
          >
            ◉
          </button>
          <button
            type="button"
            className="icon-btn"
            title={t("files.newFolder")}
            onClick={() => void makeFolder()}
          >
            ＋
          </button>
          {onSend && (
            <button
              type="button"
              className="icon-btn"
              title={t("files.send", { target: sendLabel ?? "" })}
              disabled={!current}
              onClick={() => current && onSend(current)}
            >
              ⇄
            </button>
          )}
        </div>
      </div>
      <form
        className="file-path-row"
        onSubmit={(e) => {
          e.preventDefault();
          navigate(input.trim());
        }}
      >
        <input
          className="file-path"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          spellCheck={false}
        />
        <select
          className="file-bookmarks"
          value=""
          onChange={(e) => {
            if (e.target.value) navigate(e.target.value);
          }}
        >
          <option value="">{t("files.bookmarks")}</option>
          {bookmarks.map((bookmark) => (
            <option key={bookmark} value={bookmark}>
              {bookmark}
            </option>
          ))}
        </select>
        <button
          type="button"
          className="icon-btn"
          title={t("files.addBookmark")}
          onClick={() => onAddBookmark(path)}
        >
          ☆
        </button>
        <button
          type="button"
          className="icon-btn"
          title={t("files.removeBookmark")}
          disabled={!bookmarks.includes(path)}
          onClick={() => onRemoveBookmark(path)}
        >
          ★
        </button>
      </form>
      <div className="file-table-wrap">
        <table className="file-table">
          <thead>
            <tr>
              <th>{t("files.name")}</th>
              <th>{t("files.type")}</th>
              <th>{t("files.size")}</th>
              <th>{t("files.modified")}</th>
              <th>{t("files.permissions")}</th>
              <th>{t("files.owner")}</th>
            </tr>
          </thead>
          <tbody>
            {entries.map((entry) => (
              <tr
                key={entry.path}
                className={selected === entry.path ? "selected" : undefined}
                onClick={() => setSelected(entry.path)}
                onDoubleClick={() => open(entry)}
                onContextMenu={(e) => {
                  e.preventDefault();
                  setSelected(entry.path);
                  setMenu({ x: e.clientX, y: e.clientY, entry });
                }}
              >
                <td className="file-name" title={entry.name}>
                  <span className="file-kind">{entry.isDir ? "▸" : "·"}</span>
                  {entry.name}
                  {entry.isSymlink && <span className="file-link">@</span>}
                </td>
                <td>{entry.isDir ? t("files.folder") : t("files.file")}</td>
                <td>{entry.isDir ? "—" : formatSize(entry.size)}</td>
                <td>{formatTime(entry.mtime)}</td>
                <td className="mono">{permissionString(entry.mode)}</td>
                <td className="mono" title={`${entry.uid}:${entry.gid}`}>
                  {entry.owner ?? entry.uid}:{entry.group ?? entry.gid}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {loading && <p className="hint file-loading">{t("files.loading")}</p>}
      </div>
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          items={menuItems}
          onClose={() => setMenu(null)}
        />
      )}
    </div>
  );
}
