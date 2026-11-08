import { useEffect, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { api, isTauri, type CommandGroup } from "../api";
import { useAppStore } from "../store";
import { terminalView } from "../terminal";

export function CommandRail() {
  const { t } = useTranslation();
  const open = useAppStore((s) => s.commandsOpen);
  const activeId = useAppStore((s) => s.activeId);
  const toggleCommands = useAppStore((s) => s.toggleCommands);
  const setStatus = useAppStore((s) => s.setStatus);
  const [groups, setGroups] = useState<CommandGroup[]>([]);
  const [groupName, setGroupName] = useState("");
  const [name, setName] = useState("");
  const [template, setTemplate] = useState("");
  const [groupId, setGroupId] = useState("");

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
  }, []);

  useEffect(() => {
    if (!groupId && groups.length > 0) setGroupId(groups[0].id);
  }, [groups, groupId]);

  if (!open) return null;

  const addGroup = async (e: FormEvent) => {
    e.preventDefault();
    const trimmed = groupName.trim();
    if (!trimmed) return;
    try {
      await api.createCommandGroup(crypto.randomUUID(), trimmed);
      setGroupName("");
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
      await api.createCommand(
        crypto.randomUUID(),
        groupId,
        itemName,
        itemTemplate,
      );
      setName("");
      setTemplate("");
      await reload();
    } catch (err) {
      setStatus(String(err), true);
    }
  };

  const removeCommand = async (id: string) => {
    try {
      await api.deleteCommand(id);
      await reload();
    } catch (err) {
      setStatus(String(err), true);
    }
  };

  const removeGroup = async (id: string) => {
    try {
      await api.deleteCommandGroup(id);
      await reload();
    } catch (err) {
      setStatus(String(err), true);
    }
  };

  const insert = (text: string) => {
    const view = activeId ? terminalView(activeId) : null;
    if (!view) {
      setStatus(t("commands.noSession"), true);
      return;
    }
    view.insert(text);
  };

  return (
    <aside id="command-rail" aria-label={t("commands.title")}>
      <header className="rail-head">
        <h3>{t("commands.title")}</h3>
        <button
          className="icon-btn rail-close"
          aria-label={t("common.close")}
          onClick={toggleCommands}
        >
          ×
        </button>
      </header>

      <div className="rail-body">
        {groups.length === 0 && <p className="hint">{t("commands.empty")}</p>}
        {groups.map((group) => (
          <section key={group.id} className="command-group">
            <header className="command-group-head">
              <span>{group.name}</span>
              <button
                className="rail-mini"
                title={t("commands.deleteGroup")}
                aria-label={t("commands.deleteGroup")}
                onClick={() => removeGroup(group.id)}
              >
                ×
              </button>
            </header>
            {group.commands.map((item) => (
              <div key={item.id} className="command-item">
                <button
                  className="command-insert"
                  title={item.template}
                  onClick={() => insert(item.template)}
                >
                  {item.name}
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
            {t("commands.addCommand")}
          </button>
        </form>
        <form className="rail-form" onSubmit={addGroup}>
          <input
            value={groupName}
            onChange={(e) => setGroupName(e.target.value)}
            placeholder={t("commands.newGroup")}
          />
          <button type="submit" disabled={!groupName.trim()}>
            {t("commands.addGroup")}
          </button>
        </form>
      </footer>
    </aside>
  );
}
