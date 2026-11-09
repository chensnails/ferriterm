import { useTranslation } from "react-i18next";
import { useAppStore } from "../store";
import { Modal } from "./Modal";
import { ShortcutEditor } from "./ShortcutEditor";

export function ShortcutsDialog() {
  const { t } = useTranslation();
  const open = useAppStore((s) => s.shortcutsOpen);
  const setOpen = useAppStore((s) => s.setShortcutsOpen);
  const resetShortcuts = useAppStore((s) => s.resetShortcuts);

  if (!open) return null;

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
      <ShortcutEditor />
    </Modal>
  );
}
