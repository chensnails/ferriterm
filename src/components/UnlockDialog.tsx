import { useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../api";
import { vaultErrorText } from "../vault-errors";
import { Modal } from "./Modal";

export type VaultAction = "unlock" | "enable" | "change" | "disable" | "reset";

export function UnlockDialog({
  mode,
  onClose,
  onDone,
}: {
  mode: VaultAction;
  onClose: () => void;
  onDone?: (detail?: string) => void;
}) {
  const { t } = useTranslation();
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [confirm, setConfirm] = useState("");
  const [understood, setUnderstood] = useState(false);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const needsCurrent = mode === "change" || mode === "disable";
  const needsNext = mode === "enable" || mode === "change";
  const needsConfirm = mode === "enable" || mode === "change";

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError("");
    if (needsNext && next.length < 8) {
      setError(t("vault.passwordTooShort"));
      return;
    }
    if (needsConfirm && next !== confirm) {
      setError(t("vault.passwordMismatch"));
      return;
    }
    if (mode === "reset" && !understood) {
      setError(t("vault.resetConfirmRequired"));
      return;
    }
    setBusy(true);
    try {
      if (mode === "unlock") {
        await api.vaultUnlock(current);
        onDone?.();
      } else if (mode === "enable") {
        const migrated = await api.vaultEnableMasterPassword(next);
        onDone?.(t("vault.enabledDetail", { count: migrated }));
      } else if (mode === "change") {
        await api.vaultChangeMasterPassword(current, next);
        onDone?.();
      } else if (mode === "disable") {
        await api.vaultDisableMasterPassword(current);
        onDone?.();
      } else {
        await api.vaultReset();
        onDone?.();
      }
      onClose();
    } catch (err) {
      setError(vaultErrorText(err, t));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      title={t(`vault.action.${mode}`)}
      onClose={onClose}
      footer={
        <>
          <button type="button" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button
            type="submit"
            form="vault-action-form"
            className={mode === "reset" ? "danger" : "primary"}
            disabled={busy}
          >
            {t(`vault.action.${mode}`)}
          </button>
        </>
      }
    >
      <form id="vault-action-form" className="form-grid" onSubmit={submit}>
        {mode === "reset" && (
          <p className="error-text">{t("vault.resetWarning")}</p>
        )}
        {(needsCurrent || mode === "unlock") && (
          <label className="field">
            <span>
              {mode === "change" || mode === "disable"
                ? t("vault.currentPassword")
                : t("vault.masterPassword")}
            </span>
            <input
              autoFocus
              type="password"
              value={current}
              autoComplete={mode === "unlock" ? "current-password" : "off"}
              onChange={(e) => setCurrent(e.target.value)}
            />
          </label>
        )}
        {needsNext && (
          <>
            <label className="field">
              <span>
                {mode === "change" ? t("vault.newPassword") : t("vault.masterPassword")}
              </span>
              <input
                autoFocus={mode === "enable"}
                type="password"
                value={next}
                autoComplete="new-password"
                onChange={(e) => setNext(e.target.value)}
              />
              <span className="hint">{t("vault.passwordHint")}</span>
            </label>
            <label className="field">
              <span>{t("vault.confirmPassword")}</span>
              <input
                type="password"
                value={confirm}
                autoComplete="new-password"
                onChange={(e) => setConfirm(e.target.value)}
              />
            </label>
          </>
        )}
        {mode === "reset" && (
          <label className="checkbox">
            <input
              type="checkbox"
              checked={understood}
              onChange={(e) => setUnderstood(e.target.checked)}
            />
            <span>{t("vault.resetUnderstood")}</span>
          </label>
        )}
        <p className="hint">{t(`vault.actionHint.${mode}`)}</p>
        {error && (
          <p className="error-text" role="alert">
            {error}
          </p>
        )}
      </form>
    </Modal>
  );
}
