import { useTranslation } from "react-i18next";
import { api, type TransferJob } from "../api";
import { formatBytes, formatEta } from "../format";

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

  const running = jobs.filter((job) => job.status === "running");
  const waiting = jobs.filter(
    (job) => job.status === "pending" || job.status === "paused",
  );
  const speed = running.reduce((total, job) => total + job.speed, 0);

  return (
    <section className="transfer-queue">
      <header className="transfer-queue-head">
        <div className="transfer-queue-stats">
          <span className="transfer-queue-title">
            {t("transfer.queue", { count: jobs.length })}
          </span>
          {running.length > 0 && (
            <span className="transfer-stat running">
              {t("transfer.statActive", { n: running.length })}
            </span>
          )}
          {waiting.length > 0 && (
            <span className="transfer-stat">
              {t("transfer.statWaiting", { n: waiting.length })}
            </span>
          )}
          {speed > 0 && (
            <span className="transfer-stat mono">
              {formatBytes(speed)}
              {t("transfer.perSecond")}
            </span>
          )}
        </div>
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
        <div className="transfer-table-wrap">
          <table className="transfer-table">
            <thead>
              <tr>
                <th className="col-name">{t("transfer.colName")}</th>
                <th className="col-route">{t("transfer.colRoute")}</th>
                <th className="col-progress">{t("transfer.colProgress")}</th>
                <th className="col-status">{t("transfer.colStatus")}</th>
                <th className="col-actions">{t("transfer.colActions")}</th>
              </tr>
            </thead>
            <tbody>
              {jobs.map((job) => {
                const percent =
                  job.size === 0
                    ? job.status === "done"
                      ? 100
                      : 0
                    : Math.min(
                        100,
                        Math.round((job.transferred / job.size) * 100),
                      );
                return (
                  <tr key={job.id} className={job.status}>
                    <td className="transfer-name" title={job.name}>
                      {job.isDirectory ? "▸ " : ""}
                      {job.name}
                    </td>
                    <td className="transfer-route">
                      <span title={job.sourcePath}>{job.sourcePath}</span>
                      <span className="transfer-route-arrow">→</span>
                      <span title={job.targetPath}>{job.targetPath}</span>
                    </td>
                    <td className="transfer-cell-progress">
                      <div className="transfer-bar">
                        <div
                          className={`transfer-bar-fill ${job.status}`}
                          style={{ width: `${percent}%` }}
                        />
                      </div>
                      <span className="transfer-progress-text">
                        {percent}% · {formatBytes(job.transferred)} /{" "}
                        {formatBytes(job.size)}
                      </span>
                    </td>
                    <td className="transfer-cell-status">
                      {job.relay && (
                        <span
                          className="transfer-relay"
                          title={t("transfer.relayHint")}
                        >
                          {t("transfer.relay")}
                        </span>
                      )}
                      <span
                        className={`transfer-status ${job.status}`}
                        title={job.error ?? undefined}
                      >
                        {t(`transfer.status_${job.status}`)}
                      </span>
                      {job.skipped > 0 && (
                        <span
                          className="transfer-skipped"
                          title={t("transfer.skippedHint")}
                        >
                          {t("transfer.skipped", { count: job.skipped })}
                        </span>
                      )}
                      {job.status === "running" && job.speed > 0 && (
                        <span className="transfer-speed">
                          {formatBytes(job.speed)}
                          {t("transfer.perSecond")}
                          {job.etaSeconds !== null
                            ? ` · ${formatEta(job.etaSeconds)}`
                            : ""}
                        </span>
                      )}
                    </td>
                    <td className="transfer-actions">
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
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
