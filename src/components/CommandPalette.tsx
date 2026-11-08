import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useAppStore } from "../store";

interface PaletteAction {
  id: string;
  label: string;
  hint?: string;
  run: () => void;
}

export function CommandPalette() {
  const { t } = useTranslation();
  const open = useAppStore((s) => s.paletteOpen);
  const closePalette = useAppStore((s) => s.closePalette);
  const hosts = useAppStore((s) => s.hosts);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        useAppStore.getState().togglePalette();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  useEffect(() => {
    if (!open) return;
    setQuery("");
    setActive(0);
    requestAnimationFrame(() => inputRef.current?.focus());
  }, [open]);

  useEffect(() => {
    listRef.current
      ?.querySelector(`[data-index="${active}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [active, open]);

  const actions = useMemo<PaletteAction[]>(() => {
    const store = useAppStore.getState();
    return [
      {
        id: "local",
        label: t("palette.newLocal"),
        run: () =>
          store.openSession("pty", t("session.localTitle"), { type: "pty" }),
      },
      ...hosts
        .filter((host) => !host.isFolder)
        .map((host) => ({
          id: `host-${host.id}`,
          label: host.name || `${host.username}@${host.host}`,
          hint: `${host.username}@${host.host}:${host.port}`,
          run: () => store.requestConnect(host.id),
        })),
      {
        id: "settings",
        label: t("palette.settings"),
        run: () => store.setSettingsOpen(true),
      },
      {
        id: "split",
        label: t("palette.split"),
        run: () => (store.splitId ? store.closeSplit() : store.splitActive()),
      },
      {
        id: "split-direction",
        label: t("palette.splitDirection"),
        run: () =>
          store.setSplitDirection(
            store.splitDirection === "horizontal" ? "vertical" : "horizontal",
          ),
      },
      {
        id: "shortcuts",
        label: t("palette.shortcuts"),
        run: () => store.setShortcutsOpen(true),
      },
      {
        id: "import",
        label: t("palette.import"),
        run: () => store.requestImport(),
      },
      {
        id: "tags",
        label: t("palette.tags"),
        run: () => store.requestTags(),
      },
      {
        id: "theme",
        label: t("palette.theme"),
        run: () => store.toggleTheme(),
      },
      {
        id: "monitor",
        label: t("palette.monitor"),
        run: () => store.toggleMonitor(),
      },
      {
        id: "forward",
        label: t("palette.forward"),
        run: () => store.toggleForward(),
      },
      {
        id: "commands",
        label: t("palette.commands"),
        run: () => store.toggleCommands(),
      },
      {
        id: "files",
        label: t("palette.files"),
        run: () => store.openTransfer(),
      },
      {
        id: "log",
        label: t("palette.log"),
        run: () => store.toggleExecLog(),
      },
    ];
  }, [hosts, t]);

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return actions;
    return actions.filter((action) =>
      `${action.label} ${action.hint ?? ""}`.toLowerCase().includes(needle),
    );
  }, [actions, query]);

  useEffect(() => {
    setActive(0);
  }, [query]);

  if (!open) return null;

  const run = (action?: PaletteAction) => {
    if (!action) return;
    closePalette();
    action.run();
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Escape") {
      e.preventDefault();
      closePalette();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((index) => Math.min(index + 1, filtered.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((index) => Math.max(index - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      run(filtered[active]);
    }
  };

  return (
    <div
      className="palette-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) closePalette();
      }}
    >
      <div
        className="palette"
        role="dialog"
        aria-modal="true"
        aria-label={t("palette.title")}
      >
        <input
          ref={inputRef}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={onKeyDown}
          placeholder={t("palette.placeholder")}
          aria-label={t("palette.placeholder")}
        />
        <div className="palette-list" ref={listRef} role="listbox">
          {filtered.length === 0 && (
            <p className="hint palette-empty">{t("palette.empty")}</p>
          )}
          {filtered.map((action, index) => (
            <button
              key={action.id}
              type="button"
              data-index={index}
              role="option"
              aria-selected={index === active}
              className={
                index === active ? "palette-item active" : "palette-item"
              }
              onMouseEnter={() => setActive(index)}
              onClick={() => run(action)}
            >
              <span className="palette-label">{action.label}</span>
              {action.hint && (
                <span className="palette-hint">{action.hint}</span>
              )}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
