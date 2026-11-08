import { useTranslation } from "react-i18next";
import { useAppStore } from "../store";

function formatTime(at: number): string {
  return new Date(at).toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

export function ExecutionLog() {
  const { t } = useTranslation();
  const open = useAppStore((s) => s.execLogOpen);
  const entries = useAppStore((s) => s.execLog);
  const toggleExecLog = useAppStore((s) => s.toggleExecLog);
  const clearExecLog = useAppStore((s) => s.clearExecLog);

  if (!open) return null;

  return (
    <section className="exec-log" aria-label={t("log.title")}>
      <header className="exec-log-head">
        <h3>{t("log.title")}</h3>
        <div className="exec-log-tools">
          <button
            type="button"
            className="link-btn"
            onClick={clearExecLog}
            disabled={entries.length === 0}
          >
            {t("log.clear")}
          </button>
          <button
            type="button"
            className="icon-btn"
            aria-label={t("common.close")}
            title={t("common.close")}
            onClick={toggleExecLog}
          >
            ×
          </button>
        </div>
      </header>
      {entries.length === 0 ? (
        <p className="hint exec-log-empty">{t("log.empty")}</p>
      ) : (
        <ul className="exec-log-list">
          {entries.map((entry) => (
            <li key={entry.id} className={`exec-log-row kind-${entry.kind}`}>
              <span className="exec-log-time">{formatTime(entry.at)}</span>
              {entry.sessionTitle && (
                <span className="exec-log-session">{entry.sessionTitle}</span>
              )}
              <span className="exec-log-text">{entry.text}</span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
