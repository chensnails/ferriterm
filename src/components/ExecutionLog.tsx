import { useTranslation } from "react-i18next";
import { useAppStore } from "../store";
import { EmptyState, IconButton, Island, PanelHeader } from "./ui";

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
    <Island className="exec-log" plain aria-label={t("log.title")}>
      <PanelHeader
        title={t("log.title")}
        actions={
          <>
            <button
              type="button"
              className="link-btn"
              onClick={clearExecLog}
              disabled={entries.length === 0}
            >
              {t("log.clear")}
            </button>
            <IconButton
              label={t("common.close")}
              onClick={toggleExecLog}
              size="sm"
            >
              ×
            </IconButton>
          </>
        }
      />
      {entries.length === 0 ? (
        <EmptyState className="exec-log-empty" title={t("log.empty")} />
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
    </Island>
  );
}
