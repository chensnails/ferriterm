import { useEffect, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { api, isTauri, type CommandGroup } from "../api";
import { templateVariables } from "../commands";
import { useAppStore } from "../store";
import { terminalHandle } from "../terminal-registry";
import { CommandVarsDialog } from "./CommandVarsDialog";

export function CommandRail() {
  const { t } = useTranslation();
  const activeId = useAppStore((s) => s.activeId);
  const focusedSessionId = useAppStore((s) => s.focusedSessionId);
  const commandHistory = useAppStore((s) => s.commandHistory);
  const clearCommandHistory = useAppStore((s) => s.clearCommandHistory);
  const commandLibraryVersion = useAppStore((s) => s.commandLibraryVersion);
  const setStatus = useAppStore((s) => s.setStatus);
  const [groups, setGroups] = useState<CommandGroup[]>([]);
  const [groupName, setGroupName] = useState("");
  const [name, setName] = useState("");
  const [template, setTemplate] = useState("");
  const [groupId, setGroupId] = useState("");
  const [pending, setPending] = useState<{
    name: string;
    template: string;
  } | null>(null);
  const [editing, setEditing] = useState<
    | { type: "group"; id: string }
    | { type: "command"; id: string; groupId: string }
    | null
  >(null);

  const reload = async () => {
    if (!isTauri) return;
    try {
      setGroups(await api.listCommandGroups());
    } catch (err) {
      setStatus(String(err), true);
    }
  };

  useEffect(() => {
    void reload();
  }, [commandLibraryVersion]);

  useEffect(() => {
    if (!groupId && groups.length > 0) setGroupId(groups[0].id);
  }, [groups, groupId]);

  const clearEdit = () => {
    setEditing(null);
    setGroupName("");
    setName("");
    setTemplate("");
  };

  const addGroup = async (e: FormEvent) => {
    e.preventDefault();
    const trimmed = groupName.trim();
    if (!trimmed) return;
    try {
      if (editing?.type === "group") {
        await api.updateCommandGroup(editing.id, trimmed);
      } else {
        await api.createCommandGroup(crypto.randomUUID(), trimmed);
      }
      clearEdit();
      await reload();
    } catch (err) {
      setStatus(String(err), true);
    }
  };

  const addCommand = async (e: FormEvent) => {
    e.preventDefault();
    const itemName = name.trim();
    const itemTemplate = template.trim();
    if (!itemName || !itemTemplate || !groupId) return;
    try {
      if (editing?.type === "command") {
        await api.updateCommand(editing.id, itemName, itemTemplate);
      } else {
        await api.createCommand(
          crypto.randomUUID(),
          groupId,
          itemName,
          itemTemplate,
        );
      }
      clearEdit();
      await reload();
    } catch (err) {
      setStatus(String(err), true);
    }
  };

  const removeCommand = async (id: string) => {
    try {
      await api.deleteCommand(id);
      if (editing?.type === "command" && editing.id === id) clearEdit();
      await reload();
    } catch (err) {
      setStatus(String(err), true);
    }
  };

  const removeGroup = async (id: string) => {
    try {
      await api.deleteCommandGroup(id);
      if (editing && (editing.id === id || ("groupId" in editing && editing.groupId === id))) {
        clearEdit();
      }
      await reload();
    } catch (err) {
      setStatus(String(err), true);
    }
  };

  const insert = (text: string) => {
    const target = focusedSessionId ?? activeId;
    const view = target ? terminalHandle(target) : null;
    if (!view) {
      setStatus(t("commands.noSession"), true);
      return;
    }
    view.insert(text);
  };

  const runCommand = (name: string, template: string) => {
    if (templateVariables(template).length > 0) {
      setPending({ name, template });
      return;
    }
    insert(template);
  };

  return (
    <div className="command-rail">
      <div className="rail-body">
        <section className="command-group command-history">
          <header className="command-group-head">
            <span>{t("commands.history")}</span>
            {commandHistory.length > 0 && (
              <button
                className="rail-mini"
                title={t("commands.clearHistory")}
                aria-label={t("commands.clearHistory")}
                onClick={clearCommandHistory}
              >
                ⌫
              </button>
            )}
          </header>
          {commandHistory.length === 0 ? (
            <p className="hint command-history-empty">
              {t("commands.historyEmpty")}
            </p>
          ) : (
            commandHistory.slice(0, 12).map((entry) => (
              <button
                key={entry.id}
                type="button"
                className="command-history-item"
                title={entry.text}
                onClick={() => insert(entry.text)}
              >
                <span className="command-history-text">{entry.text}</span>
                <span className="command-history-meta">
                  {entry.sessionTitle || t("commands.historyLocal")} ·{" "}
                  {new Date(entry.at).toLocaleTimeString()}
                </span>
              </button>
            ))
          )}
        </section>
        {groups.length === 0 && <p className="hint">{t("commands.empty")}</p>}
        {groups.map((group) => (
          <section key={group.id} className="command-group">
            <header className="command-group-head">
              <span>{group.name}</span>
              <span className="rail-tools">
                <button
                  className="rail-mini"
                  title={t("commands.editGroup")}
                  aria-label={t("commands.editGroup")}
                  onClick={() => {
                    setEditing({ type: "group", id: group.id });
                    setGroupName(group.name);
                    setName("");
                    setTemplate("");
                  }}
                >
                  ✎
                </button>
                <button
                  className="rail-mini"
                  title={t("commands.deleteGroup")}
                  aria-label={t("commands.deleteGroup")}
                  onClick={() => removeGroup(group.id)}
                >
                  ×
                </button>
              </span>
            </header>
            {group.commands.map((item) => (
              <div key={item.id} className="command-item">
                <button
                  className="command-insert"
                  title={item.template}
                  onClick={() => runCommand(item.name, item.template)}
                >
                  {item.name}
                </button>
                <button
                  className="rail-mini"
                  title={t("commands.editCommand")}
                  aria-label={t("commands.editCommand")}
                  onClick={() => {
                    setEditing({
                      type: "command",
                      id: item.id,
                      groupId: group.id,
                    });
                    setGroupId(group.id);
                    setName(item.name);
                    setTemplate(item.template);
                    setGroupName("");
                  }}
                >
                  ✎
                </button>
                <button
                  className="rail-mini"
                  title={t("commands.deleteCommand")}
                  aria-label={t("commands.deleteCommand")}
                  onClick={() => removeCommand(item.id)}
                >
                  ×
                </button>
              </div>
            ))}
          </section>
        ))}
      </div>

      <footer className="rail-foot">
        {editing?.type !== "group" && (
          <form className="rail-form" onSubmit={addCommand}>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={t("commands.name")}
            />
            <input
              value={template}
              onChange={(e) => setTemplate(e.target.value)}
              placeholder={t("commands.template")}
            />
            <select
              value={groupId}
              disabled={editing?.type === "command"}
              onChange={(e) => setGroupId(e.target.value)}
              aria-label={t("commands.group")}
            >
              {groups.map((group) => (
                <option key={group.id} value={group.id}>
                  {group.name}
                </option>
              ))}
            </select>
            <button
              type="submit"
              className="primary"
              disabled={!groupId || !name.trim() || !template.trim()}
            >
              {editing?.type === "command"
                ? t("commands.saveCommand")
                : t("commands.addCommand")}
            </button>
            {editing?.type === "command" && (
              <button type="button" onClick={clearEdit}>
                {t("common.cancel")}
              </button>
            )}
          </form>
        )}
        {editing?.type !== "command" && (
          <form className="rail-form" onSubmit={addGroup}>
            <input
              value={groupName}
              onChange={(e) => setGroupName(e.target.value)}
              placeholder={t("commands.newGroup")}
            />
            <button type="submit" disabled={!groupName.trim()}>
              {editing?.type === "group"
                ? t("commands.saveGroup")
                : t("commands.addGroup")}
            </button>
            {editing?.type === "group" && (
              <button type="button" onClick={clearEdit}>
                {t("common.cancel")}
              </button>
            )}
          </form>
        )}
      </footer>
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
    </div>
  );
}
