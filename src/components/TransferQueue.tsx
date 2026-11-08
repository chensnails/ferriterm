import { useTranslation } from "react-i18next";
import { api, type TransferJob } from "../api";

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

function formatEta(seconds: number | null): string {
  if (seconds === null) return "";
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ${seconds % 60}s`;
  return `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
}

export function TransferQueue({
  jobs,
  onChanged,
  onError,
}: {
  jobs: TransferJob[];
  onChanged: () => void;
  onError: (message: string) => void;
}) {
  const { t } = useTranslation();

  const act = async (
    action: (id: string) => Promise<void>,
    id: string,
  ) => {
    try {
      await action(id);
      onChanged();
    } catch (err) {
      onError(String(err));
    }
  };

  const clear = async () => {
    try {
      await api.transferClearFinished();
      onChanged();
    } catch (err) {
      onError(String(err));
    }
  };

  return (
    <section className="transfer-queue">
      <header className="transfer-queue-head">
        <span>{t("transfer.queue", { count: jobs.length })}</span>
        <button
          type="button"
          className="link-btn"
          onClick={() => void clear()}
          disabled={jobs.length === 0}
        >
          {t("transfer.clearFinished")}
        </button>
      </header>
      {jobs.length === 0 ? (
        <p className="hint">{t("transfer.empty")}</p>
      ) : (
        <div className="transfer-jobs">
          {jobs.map((job) => {
            const percent =
              job.size === 0
                ? job.status === "done"
                  ? 100
                  : 0
                : Math.min(100, Math.round((job.transferred / job.size) * 100));
            return (
              <div key={job.id} className="transfer-job">
                <div className="transfer-job-main">
                  <span className="transfer-name" title={job.targetPath}>
                    {job.isDirectory ? "▸ " : ""}
                    {job.name}
                  </span>
                  <span className={`transfer-status ${job.status}`}>
                    {t(`transfer.status_${job.status}`)}
                  </span>
                  <span className="transfer-progress-text">
                    {formatBytes(job.transferred)} / {formatBytes(job.size)}
                  </span>
                  {job.status === "running" && (
                    <span className="transfer-speed">
                      {formatBytes(job.speed)}/s
                      {job.etaSeconds !== null
                        ? ` · ${formatEta(job.etaSeconds)}`
                        : ""}
                    </span>
                  )}
                  <div className="transfer-actions">
                    {job.status === "running" && (
                      <button
                        type="button"
                        className="icon-btn"
                        title={t("transfer.pause")}
                        onClick={() => void act(api.transferPause, job.id)}
                      >
                        ⏸
                      </button>
                    )}
                    {job.status === "paused" && (
                      <button
                        type="button"
                        className="icon-btn"
                        title={t("transfer.resume")}
                        onClick={() => void act(api.transferResume, job.id)}
                      >
                        ▶
                      </button>
                    )}
                    {(job.status === "running" ||
                      job.status === "paused" ||
                      job.status === "pending") && (
                      <button
                        type="button"
                        className="icon-btn"
                        title={t("transfer.cancel")}
                        onClick={() => void act(api.transferCancel, job.id)}
                      >
                        ×
                      </button>
                    )}
                    {(job.status === "failed" ||
                      job.status === "cancelled") && (
                      <button
                        type="button"
                        className="icon-btn"
                        title={t("transfer.retry")}
                        onClick={() => void act(api.transferRetry, job.id)}
                      >
                        ↻
                      </button>
                    )}
                    {job.status !== "running" && (
                      <button
                        type="button"
                        className="icon-btn"
                        title={t("transfer.remove")}
                        onClick={() => void act(api.transferRemove, job.id)}
                      >
                        ⌫
                      </button>
                    )}
                  </div>
                </div>
                <div className="transfer-bar">
                  <div
                    className={`transfer-bar-fill ${job.status}`}
                    style={{ width: `${percent}%` }}
                  />
                </div>
                {job.error && job.status === "failed" && (
                  <p className="transfer-error">{job.error}</p>
                )}
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
}
