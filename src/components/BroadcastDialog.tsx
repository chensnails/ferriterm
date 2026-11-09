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
  const logEvent = useAppStore((s) => s.logEvent);

  // SFTP tabs have no terminal to type into, so they cannot be targets.
  const openSessions = sessions.filter(
    (session) => !session.closed && session.kind !== "sftp",
  );
  const [selected, setSelected] = useState<string[]>(() =>
    active.length > 0 ? active : openSessions.map((session) => session.id),
  );
  const [acknowledged, setAcknowledged] = useState(false);

  const toggle = (id: string) => {
    setAcknowledged(false);
    setSelected((ids) =>
      ids.includes(id) ? ids.filter((value) => value !== id) : [...ids, id],
    );
  };

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
            disabled={
              selected.length === 0 ||
              (productionCount > 0 && !acknowledged)
            }
            onClick={() => {
              logEvent(t("log.broadcastStarted", { count: selected.length }));
              startBroadcast(selected);
            }}
          >
            {t("broadcast.start", { count: selected.length })}
          </button>
        </>
      }
    >
      <div className="broadcast-body">
        <p className="hint">{t("broadcast.hint")}</p>
        {productionCount > 0 && (
          <>
            <p className="error-text" role="alert">
              {t("broadcast.productionWarning", { count: productionCount })}
            </p>
            <label className="checkbox broadcast-ack">
              <input
                type="checkbox"
                checked={acknowledged}
                onChange={(event) => setAcknowledged(event.target.checked)}
              />
              <span>{t("broadcast.confirmProduction")}</span>
            </label>
          </>
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
