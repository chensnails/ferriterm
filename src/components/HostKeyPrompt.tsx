import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { api, type HostKeyChallenge } from "../api";
import { useAppStore } from "../store";
import { Modal } from "./Modal";

export function HostKeyPrompt() {
  const { t } = useTranslation();
  const [request, setRequest] = useState<HostKeyChallenge | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<HostKeyChallenge>("hostkey://verify", (event) => {
      if (disposed) return;
      // The event is broadcast to every window; only the one owning the
      // session may answer it.
      const owns = useAppStore
        .getState()
        .sessions.some((session) => session.id === event.payload.sessionId);
      if (!owns) return;
      setError("");
      setRequest(event.payload);
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const respond = async (accept: boolean, remember: boolean) => {
    const current = request;
    if (!current) return;
    setRequest(null);
    setError("");
    try {
      await api.sshHostKeyRespond(current.requestId, accept, remember);
    } catch (err) {
      setError(String(err));
    }
  };

  if (!request) return null;

  const target = `${request.host}:${request.port}`;

  return (
    <Modal
      title={request.changed ? t("hostKey.changedTitle") : t("hostKey.title")}
      onClose={() => void respond(false, false)}
      footer={
        <>
          <button type="button" onClick={() => void respond(false, false)}>
            {t("hostKey.reject")}
          </button>
          <button type="button" onClick={() => void respond(true, false)}>
            {t("hostKey.acceptOnce")}
          </button>
          <button
            type="button"
            className="primary"
            data-testid="hostkey-accept"
            onClick={() => void respond(true, true)}
          >
            {t("hostKey.accept")}
          </button>
        </>
      }
    >
      <div className="hostkey-body">
        {request.changed && (
          <p className="error-text" role="alert">
            {t("hostKey.changedWarning")}
          </p>
        )}
        <p>{t("hostKey.message", { host: target })}</p>
        <dl className="hostkey-details">
          <dt>{t("hostKey.algorithm")}</dt>
          <dd>{request.algorithm}</dd>
          <dt>{t("hostKey.fingerprint")}</dt>
          <dd className="hostkey-fingerprint">{request.fingerprint}</dd>
        </dl>
        {error && (
          <p className="error-text" role="alert">
            {error}
          </p>
        )}
      </div>
    </Modal>
  );
}
