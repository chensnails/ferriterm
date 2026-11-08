import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useAppStore } from "../store";
import { Modal } from "./Modal";

export function BroadcastDialog() {
  const { t } = useTranslation();
  const sessions = useAppStore((s) => s.sessions);
  const active = useAppStore((s) => s.broadcastTargets);
  const startBroadcast = useAppStore((s) => s.startBroadcast);
  const cancelBroadcast = useAppStore((s) => s.cancelBroadcast);

  const openSessions = sessions.filter((session) => !session.closed);
  const [selected, setSelected] = useState<string[]>(() =>
    active.length > 0 ? active : openSessions.map((session) => session.id),
  );

  const toggle = (id: string) =>
    setSelected((ids) =>
      ids.includes(id) ? ids.filter((value) => value !== id) : [...ids, id],
    );

  const productionCount = sessions.filter(
    (session) => session.production && selected.includes(session.id),
  ).length;

  return (
    <Modal
      title={t("broadcast.title")}
      onClose={cancelBroadcast}
      footer={
        <>
          <button type="button" onClick={cancelBroadcast}>
            {t("broadcast.cancel")}
          </button>
          <button
            type="button"
            className="primary"
            disabled={selected.length === 0}
            onClick={() => startBroadcast(selected)}
          >
            {t("broadcast.start", { count: selected.length })}
          </button>
        </>
      }
    >
      <div className="broadcast-body">
        <p className="hint">{t("broadcast.hint")}</p>
        {productionCount > 0 && (
          <p className="error-text" role="alert">
            {t("broadcast.productionWarning", { count: productionCount })}
          </p>
        )}
        {openSessions.length === 0 ? (
          <p className="hint">{t("broadcast.none")}</p>
        ) : (
          <div className="broadcast-list">
            {openSessions.map((session) => (
              <label key={session.id} className="checkbox">
                <input
                  type="checkbox"
                  checked={selected.includes(session.id)}
                  onChange={() => toggle(session.id)}
                />
                <span>
                  {session.title}
                  {session.production && (
                    <span className="broadcast-prod">
                      {" "}
                      · {t("hosts.production")}
                    </span>
                  )}
                </span>
              </label>
            ))}
          </div>
        )}
      </div>
    </Modal>
  );
}
