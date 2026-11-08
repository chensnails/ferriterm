import { useTranslation } from "react-i18next";
import { useAppStore } from "../store";
import { TerminalPanel } from "./TerminalPanel";

export function TerminalArea() {
  const { t } = useTranslation();
  const sessions = useAppStore((s) => s.sessions);

  return (
    <div id="terminal-host">
      {sessions.length === 0 && (
        <div id="empty-hint">
          <p className="hint-mark">&gt;_</p>
          <p>{t("empty.hint")}</p>
          <p className="hint-sub">{t("empty.sub")}</p>
        </div>
      )}
      {sessions.map((s) => (
        <TerminalPanel key={s.id} session={s} />
      ))}
    </div>
  );
}