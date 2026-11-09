import { useTranslation } from "react-i18next";
import { useAppStore } from "../store";
import { EmptyState } from "./ui";

function formatTime(at: number): string {
  return new Date(at).toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

/** Body only: the dock card supplies the frame and the clear action. */
export function ExecutionLog() {
  const { t } = useTranslation();
  const entries = useAppStore((s) => s.execLog);

  if (entries.length === 0) {
    return <EmptyState className="exec-log-empty" title={t("log.empty")} />;
  }

  return (
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
  );
}
