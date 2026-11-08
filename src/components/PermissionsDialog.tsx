import { useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type VfsEntry } from "../api";
import { Modal } from "./Modal";

const groups = ["user", "group", "other"] as const;
const bits = ["read", "write", "execute"] as const;
const masks = [
  [0o400, 0o200, 0o100],
  [0o040, 0o020, 0o010],
  [0o004, 0o002, 0o001],
] as const;

export function PermissionsDialog({
  sessionId,
  entry,
  onClose,
  onSaved,
  onError,
}: {
  sessionId: string | null;
  entry: VfsEntry;
  onClose: () => void;
  onSaved: () => void;
  onError: (message: string) => void;
}) {
  const { t } = useTranslation();
  const [mode, setMode] = useState(entry.mode & 0o777);
  const [busy, setBusy] = useState(false);

  const toggle = (mask: number) =>
    setMode((current) => (current & mask ? current & ~mask : current | mask));

  const save = async () => {
    setBusy(true);
    try {
      await api.vfsChmod(sessionId, entry.path, mode);
      onSaved();
      onClose();
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      title={t("files.permissionsTitle", { name: entry.name })}
      onClose={onClose}
      footer={
        <>
          <button type="button" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button
            type="button"
            className="primary"
            disabled={busy}
            onClick={() => void save()}
          >
            {t("hostDialog.save")}
          </button>
        </>
      }
    >
      <p className="hint">{t("files.permissionsHint")}</p>
      <div className="perm-grid">
        <span />
        {bits.map((bit) => (
          <span key={bit} className="perm-head">
            {t(`files.perm_${bit}`)}
          </span>
        ))}
        {groups.map((group, row) => (
          <div key={group} className="perm-row">
            <span className="perm-group">{t(`files.perm_${group}`)}</span>
            {masks[row].map((mask, column) => (
              <label key={mask} className="checkbox perm-check">
                <input
                  type="checkbox"
                  checked={(mode & mask) !== 0}
                  onChange={() => toggle(mask)}
                />
                <span className="mono">{bits[column][0]}</span>
              </label>
            ))}
          </div>
        ))}
      </div>
      <p className="hint mono">
        {t("files.permissionsValue", {
          octal: mode.toString(8).padStart(3, "0"),
        })}
      </p>
    </Modal>
  );
}
