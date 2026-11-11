import { useEffect, useRef, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import {
  api,
  isTauri,
  type CommandGroup,
  type CommandItem,
} from "../api";
import { templateVariables } from "../commands";
import { useAppStore } from "../store";
import { Modal } from "./Modal";
import { EmptyState } from "./ui";
import { PlusIcon, TrashIcon } from "./icons";

interface CommandManagerDialogProps {
  onClose: () => void;
  initialGroupId?: string;
  initialCommandId?: string;
  startNew?: boolean;
}

/**
 * Editing half of the quick-command feature. The dock stays a one-click
 * launcher; everything that changes the library lives here instead of being
 * squeezed into a form at the bottom of the dock. Mirrors termora's
 * "list on the left, editor on the right" snippet manager, but keeps the
 * existing explicit save / flat group model of this app.
 */
export function CommandManagerDialog({
  onClose,
  initialGroupId,
  initialCommandId,
  startNew,
}: CommandManagerDialogProps) {
  const { t } = useTranslation();
  const bumpCommandLibrary = useAppStore((s) => s.bumpCommandLibrary);
  const setStatus = useAppStore((s) => s.setStatus);
  const [groups, setGroups] = useState<CommandGroup[]>([]);
  const [activeGroupId, setActiveGroupId] = useState("");
  const [activeCommandId, setActiveCommandId] = useState<string | null>(null);
  const [mode, setMode] = useState<"new" | "edit">("new");
  const [name, setName] = useState("");
  const [template, setTemplate] = useState("");
  const [groupId, setGroupId] = useState("");
  const [groupName, setGroupName] = useState("");
  const [creatingGroup, setCreatingGroup] = useState(false);
  const [notice, setNotice] = useState("");
  const noticeTimer = useRef<number | null>(null);

  useEffect(
    () => () => {
      if (noticeTimer.current) window.clearTimeout(noticeTimer.current);
    },
    [],
  );

  const flash = (text: string) => {
    setNotice(text);
    if (noticeTimer.current) window.clearTimeout(noticeTimer.current);
    noticeTimer.current = window.setTimeout(() => setNotice(""), 2200);
  };

  const reload = async (): Promise<CommandGroup[]> => {
    if (!isTauri) return [];
    try {
      const list = await api.listCommandGroups();
      setGroups(list);
      return list;
    } catch (err) {
      setStatus(String(err), true);
      return [];
    }
  };

  const selectCommand = (item: CommandItem, group: string) => {
    setMode("edit");
    setActiveCommandId(item.id);
    setActiveGroupId(group);
    setGroupId(group);
    setName(item.name);
    setTemplate(item.template);
  };

  const startCommand = (group?: string) => {
    const gid = group ?? activeGroupId ?? groups[0]?.id ?? "";
    setMode("new");
    setActiveCommandId(null);
    setActiveGroupId(gid);
    setGroupId(gid);
    setName("");
    setTemplate("");
  };

  useEffect(() => {
    void (async () => {
      const list = await reload();
      if (list.length === 0) {
        setCreatingGroup(true);
        return;
      }
      const gid =
        initialGroupId && list.some((group) => group.id === initialGroupId)
          ? initialGroupId
          : list[0].id;
      setActiveGroupId(gid);
      setGroupId(gid);
      if (initialCommandId) {
        const match = list
          .flatMap((group) =>
            group.commands.map((item) => ({ item, group: group.id })),
          )
          .find((entry) => entry.item.id === initialCommandId);
        if (match) {
          selectCommand(match.item, match.group);
          return;
        }
      }
      if (startNew) return;
      const first = list.find((group) => group.id === gid)?.commands[0];
      if (first) selectCommand(first, gid);
    })();
    // Initial selection is resolved once; later CRUD keeps the current editor.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const saveCommand = async (event: FormEvent) => {
    event.preventDefault();
    const itemName = name.trim();
    const itemTemplate = template.trim();
    if (!itemName || !itemTemplate || !groupId) return;
    try {
      if (mode === "edit" && activeCommandId) {
        await api.updateCommand(activeCommandId, itemName, itemTemplate);
      } else {
        const id = crypto.randomUUID();
        await api.createCommand(id, groupId, itemName, itemTemplate);
        setActiveCommandId(id);
        setMode("edit");
      }
      bumpCommandLibrary();
      await reload();
      flash(t("commands.saved"));
    } catch (err) {
      setStatus(String(err), true);
    }
  };

  const createGroup = async (event: FormEvent) => {
    event.preventDefault();
    const trimmed = groupName.trim();
    if (!trimmed) return;
    try {
      const id = crypto.randomUUID();
      await api.createCommandGroup(id, trimmed);
      bumpCommandLibrary();
      await reload();
      setGroupName("");
      setCreatingGroup(false);
      setActiveGroupId(id);
      setGroupId(id);
      startCommand(id);
      flash(t("commands.saved"));
    } catch (err) {
      setStatus(String(err), true);
    }
  };

  const renameGroup = async (group: CommandGroup) => {
    const next = await useAppStore.getState().askText({
      title: t("commands.editGroup"),
      placeholder: t("commands.newGroup"),
      value: group.name,
    });
    if (!next || next.trim() === group.name) return;
    try {
      await api.updateCommandGroup(group.id, next.trim());
      bumpCommandLibrary();
      await reload();
      flash(t("commands.saved"));
    } catch (err) {
      setStatus(String(err), true);
    }
  };

  const removeGroup = async (group: CommandGroup) => {
    const confirmed = await useAppStore.getState().askConfirm({
      title: t("commands.deleteGroup"),
      body: t("commands.deleteGroupConfirm", { name: group.name }),
      confirmLabel: t("common.delete"),
      danger: true,
    });
    if (!confirmed) return;
    try {
      await api.deleteCommandGroup(group.id);
      bumpCommandLibrary();
      const list = await reload();
      const next = list[0]?.id ?? "";
      setActiveGroupId(next);
      setGroupId(next);
      startCommand(next);
      if (list.length === 0) setCreatingGroup(true);
    } catch (err) {
      setStatus(String(err), true);
    }
  };

  const removeCommand = async () => {
    if (!activeCommandId) return;
    const confirmed = await useAppStore.getState().askConfirm({
      title: t("commands.deleteCommand"),
      body: t("commands.deleteCommandConfirm", { name: name.trim() }),
      confirmLabel: t("common.delete"),
      danger: true,
    });
    if (!confirmed) return;
    try {
      await api.deleteCommand(activeCommandId);
      bumpCommandLibrary();
      await reload();
      startCommand(activeGroupId);
    } catch (err) {
      setStatus(String(err), true);
    }
  };

  const variables = templateVariables(template);

  return (
    <Modal
      title={t("commands.manageTitle")}
      wide
      onClose={onClose}
      footer={
        <>
          {notice && (
            <span className="command-manager-notice" role="status">
              {notice}
            </span>
          )}
          <button type="button" onClick={onClose}>
            {t("common.close")}
          </button>
          <button
            type="submit"
            form="command-manager-form"
            className="primary"
            disabled={!groupId || !name.trim() || !template.trim()}
          >
            {mode === "new"
              ? t("commands.addCommand")
              : t("commands.saveCommand")}
          </button>
        </>
      }
    >
      <div className="command-manager">
        <aside className="command-manager-side" aria-label={t("commands.library")}>
          <div className="command-manager-side-head">
            <span>{t("commands.library")}</span>
            <button
              type="button"
              className="command-manager-add"
              onClick={() => startCommand()}
              title={t("commands.newCommand")}
              aria-label={t("commands.newCommand")}
            >
              <PlusIcon size={13} />
            </button>
          </div>

          {creatingGroup && (
            <form
              className="command-manager-group-create"
              onSubmit={createGroup}
            >
              <input
                value={groupName}
                onChange={(event) => setGroupName(event.target.value)}
                placeholder={t("commands.newGroup")}
                aria-label={t("commands.newGroup")}
                autoFocus
              />
              <button type="submit" disabled={!groupName.trim()}>
                {t("commands.addGroup")}
              </button>
              {groups.length > 0 && (
                <button
                  type="button"
                  onClick={() => {
                    setCreatingGroup(false);
                    setGroupName("");
                  }}
                >
                  {t("common.cancel")}
                </button>
              )}
            </form>
          )}

          <div className="command-manager-groups">
            {groups.length === 0 ? (
              <EmptyState
                title={t("commands.noGroups")}
                description={t("commands.noGroupsHint")}
              />
            ) : (
              groups.map((group) => (
                <div className="command-manager-group" key={group.id}>
                  <div
                    className={`command-manager-group-head ${
                      activeGroupId === group.id ? "active" : ""
                    }`}
                  >
                    <button
                      type="button"
                      className="command-manager-group-name"
                      onClick={() => {
                        setActiveGroupId(group.id);
                        setGroupId(group.id);
                      }}
                    >
                      <span>{group.name}</span>
                      <span className="command-manager-count">
                        {group.commands.length}
                      </span>
                    </button>
                    <span className="command-manager-group-actions">
                      <button
                        type="button"
                        className="rail-mini"
                        title={t("commands.editGroup")}
                        aria-label={t("commands.editGroup")}
                        onClick={() => void renameGroup(group)}
                      >
                        ✎
                      </button>
                      <button
                        type="button"
                        className="rail-mini"
                        title={t("commands.deleteGroup")}
                        aria-label={t("commands.deleteGroup")}
                        onClick={() => void removeGroup(group)}
                      >
                        <TrashIcon size={11} />
                      </button>
                    </span>
                  </div>
                  {activeGroupId === group.id && (
                    <div className="command-manager-commands">
                      {group.commands.length === 0 && (
                        <p className="hint">{t("commands.groupEmpty")}</p>
                      )}
                      {group.commands.map((item) => (
                        <button
                          type="button"
                          key={item.id}
                          className={`command-manager-command ${
                            activeCommandId === item.id ? "active" : ""
                          }`}
                          onClick={() => selectCommand(item, group.id)}
                          title={item.template}
                        >
                          <span className="command-manager-cmd-mark" aria-hidden>
                            {"<>"}
                          </span>
                          <span className="command-manager-cmd-name">
                            {item.name}
                          </span>
                        </button>
                      ))}
                    </div>
                  )}
                </div>
              ))
            )}
          </div>

          {!creatingGroup && (
            <button
              type="button"
              className="command-manager-new-group"
              onClick={() => setCreatingGroup(true)}
            >
              <PlusIcon size={12} />
              {t("commands.newGroup")}
            </button>
          )}
        </aside>

        <div className="command-manager-editor">
          <header className="command-manager-editor-head">
            <h3>
              {mode === "new"
                ? t("commands.newCommand")
                : t("commands.editCommand")}
            </h3>
            {mode === "edit" && (
              <button
                type="button"
                className="link-btn danger"
                onClick={() => void removeCommand()}
              >
                {t("common.delete")}
              </button>
            )}
          </header>

          <form
            id="command-manager-form"
            className="settings-form command-manager-form"
            onSubmit={saveCommand}
          >
            <label className="field">
              <span>{t("commands.name")}</span>
              <input
                value={name}
                onChange={(event) => setName(event.target.value)}
                placeholder={t("commands.name")}
                autoComplete="off"
              />
            </label>
            <label className="field">
              <span>{t("commands.template")}</span>
              <textarea
                value={template}
                onChange={(event) => setTemplate(event.target.value)}
                placeholder={t("commands.template")}
                spellCheck={false}
                rows={7}
              />
            </label>
            <label className="field">
              <span>{t("commands.group")}</span>
              <select
                value={groupId}
                disabled={mode === "edit"}
                onChange={(event) => setGroupId(event.target.value)}
                aria-label={t("commands.group")}
              >
                {groups.map((group) => (
                  <option key={group.id} value={group.id}>
                    {group.name}
                  </option>
                ))}
              </select>
              {mode === "edit" && (
                <span className="hint">{t("commands.groupLocked")}</span>
              )}
            </label>
            <div className="command-manager-preview">
              <span className="command-manager-preview-label">
                {t("commands.preview")}
              </span>
              <code className="command-preview">
                {template.trim() || t("commands.previewEmpty")}
              </code>
            </div>
          </form>

          <div className="command-manager-tips">
            <span>{t("commands.variableSyntax")}</span>
            {variables.length > 0 && (
              <span className="command-manager-vars">
                {variables.map((variable) => (
                  <code key={variable}>{`{{${variable}}}`}</code>
                ))}
              </span>
            )}
            <span className="hint">{t("commands.variablesHint")}</span>
          </div>
        </div>
      </div>
    </Modal>
  );
}
