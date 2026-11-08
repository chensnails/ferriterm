import { useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type SyncPlan } from "../api";
import { Modal } from "./Modal";

export type SyncDirection = "upload" | "download";

interface SyncDialogProps {
  direction: SyncDirection;
  localPath: string;
  remoteSession: string;
  remotePath: string;
  onClose: () => void;
  onApplied: () => void;
  onError: (message: string) => void;
}

function formatBytes(bytes: number): string {
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

export function SyncDialog({
  direction,
  localPath,
  remoteSession,
  remotePath,
  onClose,
  onApplied,
  onError,
}: SyncDialogProps) {
  const { t } = useTranslation();
  const [deleteExtra, setDeleteExtra] = useState(false);
  const [plan, setPlan] = useState<SyncPlan | null>(null);
  const [busy, setBusy] = useState(false);

  const upload = direction === "upload";
  const sourceSession = upload ? null : remoteSession;
  const sourcePath = upload ? localPath : remotePath;
  const targetSession = upload ? remoteSession : null;
  const targetPath = upload ? remotePath : localPath;

  const run = async (apply: boolean) => {
    setBusy(true);
    try {
      const result = apply
        ? await api.syncApply(
            sourceSession,
            sourcePath,
            targetSession,
            targetPath,
            deleteExtra,
          )
        : await api.syncPreview(
            sourceSession,
            sourcePath,
            targetSession,
            targetPath,
            deleteExtra,
          );
      setPlan(result);
      if (apply) {
        onApplied();
        onClose();
      }
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const directionLabel = upload
    ? t("sync.uploadDirection", { source: localPath, target: remotePath })
    : t("sync.downloadDirection", { source: remotePath, target: localPath });

  return (
    <Modal
      title={t("sync.directoryTitle")}
      onClose={onClose}
      footer={
        <>
          <button type="button" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={() => void run(false)}
          >
            {t("sync.previewButton")}
          </button>
          <button
            type="button"
            className="primary"
            disabled={busy || !plan || plan.actions.length === 0}
            onClick={() => void run(true)}
          >
            {t("sync.applyButton")}
          </button>
        </>
      }
    >
      <p className="hint">{directionLabel}</p>
      <label className="checkbox">
        <input
          type="checkbox"
          checked={deleteExtra}
          onChange={(e) => {
            setDeleteExtra(e.target.checked);
            setPlan(null);
          }}
        />
        <span>{t("sync.deleteExtra")}</span>
      </label>
      <p className="hint">{t("sync.deleteExtraHint")}</p>
      {plan && (
        <div className="sync-plan">
          <p className="sync-summary">
            {t("sync.summary", {
              dirs: plan.createDirs,
              files: plan.copyFiles,
              bytes: formatBytes(plan.uploadBytes),
              deletes: plan.deleteEntries,
            })}
          </p>
          {plan.actions.length === 0 ? (
            <p className="hint">{t("sync.upToDate")}</p>
          ) : (
            <div className="sync-actions-list">
              {plan.actions.slice(0, 200).map((action, index) => (
                <div
                  key={`${action.kind}-${action.path}-${index}`}
                  className={
                    action.kind.startsWith("delete")
                      ? "sync-action danger"
                      : "sync-action"
                  }
                >
                  <span className="sync-kind">
                    {t(`sync.kind_${action.kind}`)}
                  </span>
                  <span className="sync-path">{action.path}</span>
                  {action.size > 0 && (
                    <span className="sync-size">{formatBytes(action.size)}</span>
                  )}
                </div>
              ))}
              {plan.actions.length > 200 && (
                <p className="hint">
                  {t("sync.more", { count: plan.actions.length - 200 })}
                </p>
              )}
            </div>
          )}
        </div>
      )}
    </Modal>
  );
}
