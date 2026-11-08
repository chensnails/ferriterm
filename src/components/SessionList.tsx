import { useTranslation } from "react-i18next";
import { useAppStore } from "../store";

export function SessionList() {
  const { t } = useTranslation();
  const sessions = useAppStore((s) => s.sessions);
  const activeId = useAppStore((s) => s.activeId);
  const activate = useAppStore((s) => s.activate);
  const activeCount = sessions.filter((s) => !s.closed).length;

  return (
    <div className="session-section">
      <h2>{t("sessions.title")}</h2>
      <p className="session-count">{t("sessions.count", { count: activeCount })}</p>
      <ul id="session-list">
        {sessions.map((s) => {
          const cls = [];
          if (s.id === activeId) cls.push("active");
          if (s.closed) cls.push("closed");
          return (
            <li key={s.id} className={cls.join(" ")} onClick={() => activate(s.id)}>
              {s.title}
              {s.closed && <span className="closed-suffix">{t("sessions.closed")}</span>}
            </li>
          );
        })}
      </ul>
    </div>
  );
}