import { useState } from "react";
import { useTranslation } from "react-i18next";
import {
  comboFromEvent,
  displayCombo,
  findConflict,
  SHORTCUT_ACTIONS,
  SHORTCUT_DEFAULTS,
  type ShortcutAction,
} from "../keybindings";
import { useAppStore } from "../store";
import { Modal } from "./Modal";

export function ShortcutsDialog() {
  const { t } = useTranslation();
  const open = useAppStore((s) => s.shortcutsOpen);
  const setOpen = useAppStore((s) => s.setShortcutsOpen);
  const shortcuts = useAppStore((s) => s.shortcuts);
  const setShortcut = useAppStore((s) => s.setShortcut);
  const resetShortcuts = useAppStore((s) => s.resetShortcuts);
  const [editing, setEditing] = useState<ShortcutAction | null>(null);
  const [draft, setDraft] = useState("");
  const [error, setError] = useState("");

  if (!open) return null;

  const beginEdit = (action: ShortcutAction) => {
    setEditing(action);
    setDraft("");
    setError("");
  };

  const cancelEdit = () => {
    setEditing(null);
    setDraft("");
    setError("");
  };

  const capture = (action: ShortcutAction, event: React.KeyboardEvent) => {
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape") {
      cancelEdit();
      return;
    }
    if (event.key === "Backspace" || event.key === "Delete") {
      setDraft("");
      setError("");
      return;
    }
    const combo = comboFromEvent(event.nativeEvent);
    if (!combo) return;
    const conflict = findConflict(shortcuts, action, combo);
    setDraft(combo);
    setError(
      conflict
        ? t("shortcuts.conflict", {
            action: t(`shortcuts.${conflict}`),
          })
        : "",
    );
  };

  const save = (action: ShortcutAction) => {
    if (error) return;
    setShortcut(action, draft);
    cancelEdit();
  };

  return (
    <Modal
      title={t("shortcuts.title")}
      onClose={() => setOpen(false)}
      footer={
        <>
          <button type="button" onClick={resetShortcuts}>
            {t("shortcuts.resetAll")}
          </button>
          <button type="button" onClick={() => setOpen(false)}>
            {t("common.close")}
          </button>
        </>
      }
    >
      <p className="hint">{t("shortcuts.editHint")}</p>
      <dl className="shortcut-list">
        {SHORTCUT_ACTIONS.map((action) => {
          const combo = shortcuts[action];
          const isEditing = editing === action;
          return (
            <div key={action} className="shortcut-row">
              <dt>{t(`shortcuts.${action}`)}</dt>
              <dd>
                {isEditing ? (
                  <div className="shortcut-editor">
                    <button
                      type="button"
                      className="shortcut-capture"
                      autoFocus
                      onKeyDown={(event) => capture(action, event)}
                      onBlur={() => {
                        if (!draft && !error) cancelEdit();
                      }}
                    >
                      {draft
                        ? displayCombo(draft)
                        : t("shortcuts.pressKeys")}
                    </button>
                    <button
                      type="button"
                      className="link-btn"
                      disabled={Boolean(error)}
                      onMouseDown={(event) => event.preventDefault()}
                      onClick={() => save(action)}
                    >
                      {t("shortcuts.save")}
                    </button>
                    <button
                      type="button"
                      className="link-btn"
                      onMouseDown={(event) => event.preventDefault()}
                      onClick={cancelEdit}
                    >
                      {t("shortcuts.cancelEdit")}
                    </button>
                  </div>
                ) : (
                  <>
                    <kbd>{combo ? displayCombo(combo) : t("shortcuts.disabled")}</kbd>
                    <button
                      type="button"
                      className="link-btn"
                      onClick={() => beginEdit(action)}
                    >
                      {t("shortcuts.edit")}
                    </button>
                    {combo !== SHORTCUT_DEFAULTS[action] && (
                      <button
                        type="button"
                        className="link-btn"
                        onClick={() => setShortcut(action, SHORTCUT_DEFAULTS[action])}
                      >
                        {t("shortcuts.reset")}
                      </button>
                    )}
                  </>
                )}
              </dd>
              {isEditing && error && (
                <p className="error-text shortcut-error">{error}</p>
              )}
            </div>
          );
        })}
      </dl>
    </Modal>
  );
}
