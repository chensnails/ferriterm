import { useTranslation } from "react-i18next";
import { useAppStore } from "../store";
import { Modal } from "./Modal";

const rows: { key: string; combo: string }[] = [
  { key: "palette", combo: "Ctrl/⌘ K" },
  { key: "search", combo: "Ctrl/⌘ F" },
  { key: "newTab", combo: "Ctrl/⌘ T" },
  { key: "closeTab", combo: "Ctrl/⌘ W" },
  { key: "nextTab", combo: "Ctrl/⌘ Tab · Shift" },
  { key: "split", combo: "Ctrl/⌘ Shift D" },
  { key: "settings", combo: "Ctrl/⌘ ," },
  { key: "help", combo: "Ctrl/⌘ /" },
];

export function ShortcutsDialog() {
  const { t } = useTranslation();
  const open = useAppStore((s) => s.shortcutsOpen);
  const setOpen = useAppStore((s) => s.setShortcutsOpen);

  if (!open) return null;

  return (
    <Modal
      title={t("shortcuts.title")}
      onClose={() => setOpen(false)}
      footer={
        <button type="button" onClick={() => setOpen(false)}>
          {t("common.close")}
        </button>
      }
    >
      <dl className="shortcut-list">
        {rows.map((row) => (
          <div key={row.key} className="shortcut-row">
            <dt>{t(`shortcuts.${row.key}`)}</dt>
            <dd>
              <kbd>{row.combo}</kbd>
            </dd>
          </div>
        ))}
      </dl>
    </Modal>
  );
}
