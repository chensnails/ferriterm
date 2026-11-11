import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, isTauri, type CommandGroup, type CommandItem } from "../api";
import { templateVariables } from "../commands";
import { useAppStore } from "../store";
import { terminalHandle } from "../terminal-registry";
import { CommandManagerDialog } from "./CommandManagerDialog";
import { CommandVarsDialog } from "./CommandVarsDialog";
import { EmptyState } from "./ui";
import { PlusIcon, SearchIcon, SlidersIcon, TrashIcon } from "./icons";

interface ManagerTarget {
  groupId?: string;
  commandId?: string;
  startNew?: boolean;
}

/**
 * Launching half of the quick-command feature: history, grouped commands and
 * one-click insertion into the focused terminal. Editing moved to
 * CommandManagerDialog so the dock stays a compact, always-visible launcher
 * instead of doubling as a form.
 */
export function CommandRail() {
  const { t } = useTranslation();
  const activeId = useAppStore((s) => s.activeId);
  const focusedSessionId = useAppStore((s) => s.focusedSessionId);
  const commandHistory = useAppStore((s) => s.commandHistory);
  const clearCommandHistory = useAppStore((s) => s.clearCommandHistory);
  const commandLibraryVersion = useAppStore((s) => s.commandLibraryVersion);
  const setStatus = useAppStore((s) => s.setStatus);
  const [groups, setGroups] = useState<CommandGroup[]>([]);
  const [query, setQuery] = useState("");
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [manager, setManager] = useState<ManagerTarget | null>(null);
  const [pending, setPending] = useState<{
    name: string;
    template: string;
  } | null>(null);
  const [insertedId, setInsertedId] = useState<string | null>(null);

  useEffect(() => {
    if (!isTauri) return;
    void (async () => {
      try {
        setGroups(await api.listCommandGroups());
      } catch (err) {
        setStatus(String(err), true);
      }
    })();
  }, [commandLibraryVersion, setStatus]);

  const total = groups.reduce((sum, group) => sum + group.commands.length, 0);

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return groups;
    return groups
      .map((group) => {
        if (group.name.toLowerCase().includes(needle)) return group;
        return {
          ...group,
          commands: group.commands.filter(
            (item) =>
              item.name.toLowerCase().includes(needle) ||
              item.template.toLowerCase().includes(needle),
          ),
        };
      })
      .filter(
        (group) =>
          group.commands.length > 0 ||
          group.name.toLowerCase().includes(needle),
      );
  }, [groups, query]);

  const toggleSection = (key: string) => {
    setCollapsed((current) => {
      const next = new Set(current);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  const insert = (text: string, id?: string) => {
    const target = focusedSessionId ?? activeId;
    const view = target ? terminalHandle(target) : null;
    if (!view) {
      setStatus(t("commands.noSession"), true);
      return;
    }
    view.insert(text);
    if (id) {
      setInsertedId(id);
      window.setTimeout(() => setInsertedId(null), 900);
    }
  };

  const runCommand = (item: CommandItem) => {
    if (templateVariables(item.template).length > 0) {
      setPending({ name: item.name, template: item.template });
      return;
    }
    insert(item.template, item.id);
  };

  const historyExpanded = !collapsed.has("history");

  return (
    <div className="command-rail">
      <div className="command-toolbar">
        <label className="command-search">
          <SearchIcon size={13} />
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t("commands.search")}
            aria-label={t("commands.search")}
            autoComplete="off"
            spellCheck={false}
          />
        </label>
        <button
          type="button"
          className="command-tool-btn primary"
          onClick={() => setManager({ startNew: true })}
          title={t("commands.newCommand")}
        >
          <PlusIcon size={13} />
          <span>{t("commands.newCommand")}</span>
        </button>
        <button
          type="button"
          className="command-tool-btn icon-only"
          onClick={() => setManager({})}
          title={t("commands.manage")}
          aria-label={t("commands.manage")}
        >
          <SlidersIcon size={13} />
        </button>
      </div>

      <div className="rail-body">
        {!query && (
          <section className="command-group command-history">
            <header className="command-group-head">
              <button
                type="button"
                className="command-group-toggle"
                aria-expanded={historyExpanded}
                onClick={() => toggleSection("history")}
                title={t("commands.toggleGroup")}
              >
                <span className="command-caret" aria-hidden>
                  ▸
                </span>
                <span>{t("commands.history")}</span>
                <span className="command-count">{commandHistory.length}</span>
              </button>
              {commandHistory.length > 0 && (
                <button
                  type="button"
                  className="rail-mini"
                  title={t("commands.clearHistory")}
                  aria-label={t("commands.clearHistory")}
                  onClick={clearCommandHistory}
                >
                  <TrashIcon size={11} />
                </button>
              )}
            </header>
            {historyExpanded &&
              (commandHistory.length === 0 ? (
                <EmptyState
                  className="command-history-empty"
                  title={t("commands.historyEmpty")}
                />
              ) : (
                commandHistory.slice(0, 12).map((entry) => (
                  <button
                    key={entry.id}
                    type="button"
                    className={`command-history-item ${
                      insertedId === entry.id ? "inserted" : ""
                    }`}
                    title={entry.text}
                    onClick={() => insert(entry.text, entry.id)}
                  >
                    <span className="command-history-text">{entry.text}</span>
                    <span className="command-history-meta">
                      {entry.sessionTitle || t("commands.historyLocal")} ·{" "}
                      {new Date(entry.at).toLocaleTimeString()}
                    </span>
                  </button>
                ))
              ))}
          </section>
        )}

        <div className="command-library-head">
          <span>{t("commands.library")}</span>
          <span className="command-count">{total}</span>
        </div>

        {groups.length === 0 ? (
          <EmptyState
            className="command-empty"
            title={t("commands.empty")}
            description={t("commands.emptyHint")}
            action={
              <button type="button" onClick={() => setManager({ startNew: true })}>
                {t("commands.newCommand")}
              </button>
            }
          />
        ) : filtered.length === 0 ? (
          <EmptyState className="command-empty" title={t("commands.noMatch")} />
        ) : (
          filtered.map((group) => {
            const expanded = query.length > 0 || !collapsed.has(group.id);
            return (
              <section key={group.id} className="command-group">
                <header className="command-group-head">
                  <button
                    type="button"
                    className="command-group-toggle"
                    aria-expanded={expanded}
                    onClick={() => toggleSection(group.id)}
                    title={t("commands.toggleGroup")}
                  >
                    <span className="command-caret" aria-hidden>
                      ▸
                    </span>
                    <span>{group.name}</span>
                    <span className="command-count">
                      {group.commands.length}
                    </span>
                  </button>
                  <button
                    type="button"
                    className="rail-mini"
                    title={t("commands.manage")}
                    aria-label={t("commands.manage")}
                    onClick={() => setManager({ groupId: group.id })}
                  >
                    ✎
                  </button>
                </header>
                {expanded &&
                  group.commands.map((item) => {
                    const variables = templateVariables(item.template).length;
                    return (
                      <div
                        key={item.id}
                        className={`command-item ${
                          insertedId === item.id ? "inserted" : ""
                        }`}
                      >
                        <button
                          type="button"
                          className="command-insert"
                          title={item.template}
                          onClick={() => runCommand(item)}
                        >
                          <span className="command-item-name">{item.name}</span>
                          <span className="command-item-preview">
                            {item.template}
                          </span>
                        </button>
                        {variables > 0 && (
                          <span
                            className="command-var-badge"
                            title={t("commands.varCount", { count: variables })}
                          >
                            {`{${variables}}`}
                          </span>
                        )}
                        <button
                          type="button"
                          className="rail-mini"
                          title={t("commands.editCommand")}
                          aria-label={t("commands.editCommand")}
                          onClick={() =>
                            setManager({
                              groupId: group.id,
                              commandId: item.id,
                            })
                          }
                        >
                          ✎
                        </button>
                      </div>
                    );
                  })}
              </section>
            );
          })
        )}
      </div>

      {pending && (
        <CommandVarsDialog
          name={pending.name}
          template={pending.template}
          onCancel={() => setPending(null)}
          onInsert={(command) => {
            setPending(null);
            insert(command);
          }}
        />
      )}
      {manager && (
        <CommandManagerDialog
          onClose={() => setManager(null)}
          initialGroupId={manager.groupId}
          initialCommandId={manager.commandId}
          startNew={manager.startNew}
        />
      )}
    </div>
  );
}
