import { useTranslation } from "react-i18next";
import { useAppStore } from "../store";

export function TabBar() {
  const { t } = useTranslation();
  const sessions = useAppStore((s) => s.sessions);
  const activeId = useAppStore((s) => s.activeId);
  const activate = useAppStore((s) => s.activate);
  const closeSession = useAppStore((s) => s.closeSession);

  if (sessions.length === 0) return null;

  return (
    <div id="tabbar">
      {sessions.map((s) => {
        const cls = ["tab"];
        if (s.id === activeId) cls.push("active");
        if (s.closed) cls.push("closed");
        return (
          <div key={s.id} className={cls.join(" ")} onClick={() => activate(s.id)}>
            <span className="tab-label">{s.title}</span>
            <button
              className="tab-close"
              title={t("sessions.closeTab")}
              onClick={(e) => {
                e.stopPropagation();
                closeSession(s.id);
              }}
            >
              ×
            </button>
          </div>
        );
      })}
    </div>
  );
}