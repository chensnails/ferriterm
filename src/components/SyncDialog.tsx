import { useState } from "react";
import { useTranslation } from "react-i18next";
import {
  api,
  type SyncAction,
  type SyncConflict,
  type SyncPlan,
} from "../api";
import { Modal } from "./Modal";

export type SyncDirection = "upload" | "download" | "twoway";

type ConflictChoice = "newer" | "local" | "remote" | "skip";

type Stage = "plan" | "confirm";

const ACTION_LIMIT = 200;

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

function isDelete(action: SyncAction): boolean {
  return action.kind === "deleteFile" || action.kind === "deleteDir";
}

/** Turns one conflict plus the user's choice into the action that settles it. */
function resolveConflict(
  conflict: SyncConflict,
  choice: ConflictChoice,
): SyncAction | null {
  // A file on one side and a folder on the other can only be fixed by hand.
  if (conflict.reason === "typeMismatch") {
    return null;
  }
  if (choice === "newer") {
    choice = conflict.targetMtime > conflict.sourceMtime ? "remote" : "local";
  }
  if (choice === "local") {
    return {
      kind: "copyFile",
      side: "target",
      path: conflict.path,
      size: conflict.sourceSize,
    };
  }
  if (choice === "remote") {
    return {
      kind: "copyFile",
      side: "source",
      path: conflict.path,
      size: conflict.targetSize,
    };
  }
  return null;
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
  const twoWay = direction === "twoway";
  const [deleteExtra, setDeleteExtra] = useState(false);
  const [plan, setPlan] = useState<SyncPlan | null>(null);
  const [busy, setBusy] = useState(false);
  const [stage, setStage] = useState<Stage>("plan");
  const [choices, setChoices] = useState<Record<string, ConflictChoice>>({});
  const [keptDeletes, setKeptDeletes] = useState<string[]>([]);
  const [showAll, setShowAll] = useState(false);

  // A two-way run starts from the local side; "target" is then the remote.
  const upload = direction !== "download";
  const sourceSession = upload ? null : remoteSession;
  const sourcePath = upload ? localPath : remotePath;
  const targetSession = upload ? remoteSession : null;
  const targetPath = upload ? remotePath : localPath;

  const buildPlan = (): SyncPlan => {
    if (!plan) throw new Error("preview first");
    const kept = plan.actions.filter(
      (action) => !isDelete(action) || keptDeletes.includes(action.path),
    );
    const settled: SyncAction[] = [];
    if (twoWay) {
      for (const conflict of plan.conflicts) {
        const action = resolveConflict(
          conflict,
          choices[conflict.path] ?? "newer",
        );
        if (action) settled.push(action);
      }
    }
    return { ...plan, actions: [...kept, ...settled] };
  };

  const run = async (apply: boolean) => {
    setBusy(true);
    try {
      if (apply) {
        await api.syncApply(sourceSession, targetSession, buildPlan());
        onApplied();
        onClose();
        return;
      }
      const result = await api.syncPreview(
        sourceSession,
        sourcePath,
        targetSession,
        targetPath,
        twoWay,
        twoWay ? false : deleteExtra,
      );
      setPlan(result);
      setStage("plan");
      setShowAll(false);
      setChoices({});
      setKeptDeletes(result.actions.filter(isDelete).map((a) => a.path));
    } catch (err) {
      onError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const deletions = plan ? plan.actions.filter(isDelete) : [];
  const conflictCount = plan ? plan.conflicts.length : 0;
  const pendingConflicts = plan
    ? plan.conflicts.filter(
        (conflict) =>
          conflict.reason !== "typeMismatch" &&
          (choices[conflict.path] ?? "newer") !== "skip",
      ).length
    : 0;
  const plannedActions = plan
    ? plan.actions.filter((action) => !isDelete(action)).length + pendingConflicts
    : 0;

  const directionLabel = twoWay
    ? t("sync.twoWayDirection", { local: localPath, remote: remotePath })
    : upload
      ? t("sync.uploadDirection", { source: localPath, target: remotePath })
      : t("sync.downloadDirection", { source: remotePath, target: localPath });

  const visibleActions = plan
    ? showAll
      ? plan.actions
      : plan.actions.slice(0, ACTION_LIMIT)
    : [];

  const footer =
    stage === "confirm" ? (
      <>
        <button type="button" onClick={() => setStage("plan")}>
          {t("sync.back")}
        </button>
        <button
          type="button"
          className="danger"
          disabled={busy || (keptDeletes.length === 0 && plannedActions === 0)}
          onClick={() => void run(true)}
        >
          {keptDeletes.length > 0
            ? t("sync.deleteConfirmButton", { count: keptDeletes.length })
            : t("sync.continueButton")}
        </button>
      </>
    ) : (
      <>
        <button type="button" onClick={onClose}>
          {t("common.cancel")}
        </button>
        <button type="button" disabled={busy} onClick={() => void run(false)}>
          {t("sync.previewButton")}
        </button>
        <button
          type="button"
          className="primary"
          disabled={busy || !plan || (plannedActions === 0 && deletions.length === 0)}
          onClick={() => {
            if (deletions.length > 0) {
              setStage("confirm");
              return;
            }
            void run(true);
          }}
        >
          {t("sync.applyButton")}
        </button>
      </>
    );

  return (
    <Modal title={t("sync.directoryTitle")} onClose={onClose} footer={footer}>
      <p className="hint">{directionLabel}</p>
      {stage === "confirm" ? (
        <div className="sync-plan">
          <p className="sync-summary">
            {t("sync.deleteConfirmTitle", { count: deletions.length })}
          </p>
          <p className="hint">{t("sync.deleteConfirmHint")}</p>
          <div className="sync-bulk">
            <button
              type="button"
              onClick={() => setKeptDeletes(deletions.map((a) => a.path))}
            >
              {t("sync.selectAll")}
            </button>
            <button type="button" onClick={() => setKeptDeletes([])}>
              {t("sync.selectNone")}
            </button>
          </div>
          <div className="sync-actions-list">
            {deletions.map((action) => (
              <label key={action.path} className="sync-action danger">
                <input
                  type="checkbox"
                  checked={keptDeletes.includes(action.path)}
                  onChange={(e) =>
                    setKeptDeletes((previous) =>
                      e.target.checked
                        ? [...previous, action.path]
                        : previous.filter((path) => path !== action.path),
                    )
                  }
                />
                <span className="sync-kind">{t(`sync.kind_${action.kind}`)}</span>
                <span className="sync-path">{action.path}</span>
                {action.size > 0 && (
                  <span className="sync-size">{formatBytes(action.size)}</span>
                )}
              </label>
            ))}
          </div>
        </div>
      ) : (
        <>
          {!twoWay && (
            <>
              <label className="checkbox">
                <input
                  type="checkbox"
                  checked={deleteExtra}
                  onChange={(e) => {
                    setDeleteExtra(e.target.checked);
                    setPlan(null);
                    setStage("plan");
                  }}
                />
                <span>{t("sync.deleteExtra")}</span>
              </label>
              <p className="hint">{t("sync.deleteExtraHint")}</p>
            </>
          )}
          {twoWay && <p className="hint">{t("sync.twoWayHint")}</p>}
          {plan && (
            <div className="sync-plan">
              <p className="sync-summary">
                {twoWay
                  ? t("sync.summaryTwoWay", {
                      up: plan.upFiles,
                      down: plan.downFiles,
                      bytes: formatBytes(plan.upBytes + plan.downBytes),
                      conflicts: conflictCount,
                    })
                  : t("sync.summary", {
                      dirs: plan.createDirs,
                      files: plan.upFiles,
                      bytes: formatBytes(plan.upBytes),
                      deletes: plan.deleteEntries,
                    })}
              </p>
              {conflictCount > 0 && (
                <div className="sync-conflicts">
                  <p className="sync-summary">
                    {t("sync.conflictsTitle", { count: conflictCount })}
                  </p>
                  <p className="hint">{t("sync.conflictHint")}</p>
                  <div className="sync-actions-list">
                    {plan.conflicts.map((conflict) => (
                      <div
                        key={conflict.path}
                        className="sync-action sync-conflict"
                      >
                        <span className="sync-path">{conflict.path}</span>
                        <span className="sync-size">
                          {t("sync.conflictSides", {
                            local: formatBytes(conflict.sourceSize),
                            remote: formatBytes(conflict.targetSize),
                          })}
                        </span>
                        {conflict.reason === "typeMismatch" ? (
                          <span className="sync-kind">
                            {t("sync.conflictManual")}
                          </span>
                        ) : (
                          <select
                            value={choices[conflict.path] ?? "newer"}
                            onChange={(e) =>
                              setChoices((previous) => ({
                                ...previous,
                                [conflict.path]: e.target.value as ConflictChoice,
                              }))
                            }
                          >
                            <option value="newer">{t("sync.keepNewer")}</option>
                            <option value="local">{t("sync.keepLocal")}</option>
                            <option value="remote">{t("sync.keepRemote")}</option>
                            <option value="skip">{t("sync.skip")}</option>
                          </select>
                        )}
                      </div>
                    ))}
                  </div>
                </div>
              )}
              {plan.actions.length === 0 ? (
                conflictCount === 0 && <p className="hint">{t("sync.upToDate")}</p>
              ) : (
                <div className="sync-actions-list">
                  {visibleActions.map((action, index) => (
                    <div
                      key={`${action.side}-${action.kind}-${action.path}-${index}`}
                      className={
                        isDelete(action) ? "sync-action danger" : "sync-action"
                      }
                    >
                      <span className="sync-kind">
                        {t(`sync.kind_${action.kind}`)}
                      </span>
                      {twoWay && (
                        <span className="sync-side">
                          {action.side === "target"
                            ? t("sync.sideRemote")
                            : t("sync.sideLocal")}
                        </span>
                      )}
                      <span className="sync-path">{action.path}</span>
                      {action.size > 0 && (
                        <span className="sync-size">
                          {formatBytes(action.size)}
                        </span>
                      )}
                    </div>
                  ))}
                  {plan.actions.length > ACTION_LIMIT && (
                    <div className="sync-bulk">
                      <button
                        type="button"
                        onClick={() => setShowAll((value) => !value)}
                      >
                        {showAll
                          ? t("sync.showLess")
                          : t("sync.more", {
                              count: plan.actions.length - ACTION_LIMIT,
                            })}
                      </button>
                    </div>
                  )}
                </div>
              )}
            </div>
          )}
        </>
      )}
    </Modal>
  );
}
