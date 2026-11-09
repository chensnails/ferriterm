import { useTranslation } from "react-i18next";
import { useAppStore } from "../store";
import { Modal } from "./Modal";

export function CloseConfirmDialog() {
  const { t } = useTranslation();
  const pendingCloseId = useAppStore((s) => s.pendingCloseId);
  const sessions = useAppStore((s) => s.sessions);
  const cancelCloseSession = useAppStore((s) => s.cancelCloseSession);
  const confirmCloseSession = useAppStore((s) => s.confirmCloseSession);

  if (!pendingCloseId) return null;
  const session = sessions.find((s) => s.id === pendingCloseId);
  if (!session) return null;

  return (
    <Modal
      title={t("sessions.closeConfirmTitle")}
      onClose={cancelCloseSession}
      footer={
        <>
          <button type="button" onClick={cancelCloseSession}>
            {t("common.cancel")}
          </button>
          <button type="button" className="danger" onClick={confirmCloseSession}>
            {t("sessions.closeTab")}
          </button>
        </>
      }
    >
      <p>{t("sessions.closeConfirmBody", { title: session.title })}</p>
    </Modal>
  );
}