import { Fragment, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type PasswordCredential } from "../api";
import { useAppStore } from "../store";
import { Modal } from "./Modal";

/** Named passwords stored in the vault. Hosts link one of these instead of
    asking for the password on every connect. */
export function PasswordManagerDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const [items, setItems] = useState<PasswordCredential[]>([]);
  const [name, setName] = useState("");
  const [username, setUsername] = useState("");
  const [secret, setSecret] = useState("");
  const [notes, setNotes] = useState("");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draftName, setDraftName] = useState("");
  const [draftUsername, setDraftUsername] = useState("");
  const [draftNotes, setDraftNotes] = useState("");
  const [draftSecret, setDraftSecret] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");

  const reload = async () => {
    try {
      setItems(await api.passwordCredentialList());
    } catch (err) {
      setError(String(err));
    }
  };

  useEffect(() => {
    void reload();
  }, []);

  const create = async () => {
    if (!secret) {
      setError(t("passwords.passwordRequired"));
      return;
    }
    setBusy(true);
    setError("");
    setMessage("");
    try {
      const credential = await api.passwordCredentialSave({
        id: crypto.randomUUID(),
        name: name.trim() || `${t("passwords.untitled")}`,
        username,
        notes,
        secret,
      });
      setMessage(t("passwords.saved", { name: credential.name }));
      setName("");
      setUsername("");
      setSecret("");
      setNotes("");
      await reload();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const startEdit = (credential: PasswordCredential) => {
    setEditingId(credential.id);
    setDraftName(credential.name);
    setDraftUsername(credential.username);
    setDraftNotes(credential.notes);
    setDraftSecret("");
    setError("");
  };

  const saveEdit = async (credential: PasswordCredential) => {
    setBusy(true);
    setError("");
    try {
      const updated = await api.passwordCredentialSave({
        id: credential.id,
        name: draftName,
        username: draftUsername,
        notes: draftNotes,
        secret: draftSecret || undefined,
      });
      setMessage(t("passwords.saved", { name: updated.name }));
      setEditingId(null);
      await reload();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const remove = async (credential: PasswordCredential) => {
    const confirmed = await useAppStore.getState().askConfirm({
      title: t("passwords.delete"),
      body: t("passwords.deleteConfirm", { name: credential.name }),
      confirmLabel: t("common.delete"),
      danger: true,
    });
    if (!confirmed) return;
    setError("");
    try {
      await api.passwordCredentialDelete(credential.id);
      setMessage(t("passwords.deleted", { name: credential.name }));
      await reload();
    } catch (err) {
      setError(String(err));
    }
  };

  return (
    <Modal
      title={t("passwords.title")}
      onClose={onClose}
      className="key-manager"
    >
      <fieldset className="settings-group">
        <legend>{t("passwords.addTitle")}</legend>
        <p className="hint">{t("passwords.addHint")}</p>
        <label className="field">
          <span>{t("passwords.name")}</span>
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={t("passwords.namePlaceholder")}
          />
        </label>
        <label className="field">
          <span>{t("passwords.username")}</span>
          <input
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            placeholder={t("passwords.usernamePlaceholder")}
          />
        </label>
        <label className="field">
          <span>{t("passwords.password")}</span>
          <input
            type="password"
            value={secret}
            onChange={(e) => setSecret(e.target.value)}
          />
        </label>
        <label className="field">
          <span>{t("passwords.notes")}</span>
          <textarea
            rows={2}
            value={notes}
            maxLength={2000}
            onChange={(e) => setNotes(e.target.value)}
          />
        </label>
        <div className="plugin-actions">
          <button
            type="button"
            className="primary"
            disabled={busy}
            onClick={() => void create()}
          >
            {t("passwords.add")}
          </button>
        </div>
      </fieldset>

      <fieldset className="settings-group">
        <legend>{t("passwords.libraryTitle")}</legend>
        {items.length === 0 ? (
          <p className="hint">{t("passwords.empty")}</p>
        ) : (
          <ul className="plugin-list">
            {items.map((credential) => (
              <Fragment key={credential.id}>
                <li className="plugin-row">
                  <span className="plugin-main">
                    <span className="plugin-name">{credential.name}</span>
                    {credential.username && (
                      <span className="hint plugin-desc">
                        {credential.username}
                      </span>
                    )}
                    {credential.notes && (
                      <span className="hint plugin-desc">
                        {credential.notes}
                      </span>
                    )}
                  </span>
                  <button
                    type="button"
                    className="link-btn"
                    onClick={() => startEdit(credential)}
                  >
                    {t("passwords.edit")}
                  </button>
                  <button
                    type="button"
                    className="link-btn danger"
                    onClick={() => void remove(credential)}
                  >
                    {t("common.delete")}
                  </button>
                </li>
                {editingId === credential.id && (
                  <li className="key-editor">
                    <label className="field">
                      <span>{t("passwords.name")}</span>
                      <input
                        value={draftName}
                        onChange={(e) => setDraftName(e.target.value)}
                      />
                    </label>
                    <label className="field">
                      <span>{t("passwords.username")}</span>
                      <input
                        value={draftUsername}
                        onChange={(e) => setDraftUsername(e.target.value)}
                      />
                    </label>
                    <label className="field">
                      <span>{t("passwords.newPassword")}</span>
                      <input
                        type="password"
                        value={draftSecret}
                        placeholder={t("passwords.newPasswordPlaceholder")}
                        onChange={(e) => setDraftSecret(e.target.value)}
                      />
                    </label>
                    <label className="field">
                      <span>{t("passwords.notes")}</span>
                      <textarea
                        rows={2}
                        value={draftNotes}
                        maxLength={2000}
                        onChange={(e) => setDraftNotes(e.target.value)}
                      />
                    </label>
                    <div className="plugin-actions">
                      <button
                        type="button"
                        className="primary"
                        disabled={busy}
                        onClick={() => void saveEdit(credential)}
                      >
                        {t("passwords.save")}
                      </button>
                      <button
                        type="button"
                        disabled={busy}
                        onClick={() => setEditingId(null)}
                      >
                        {t("common.cancel")}
                      </button>
                    </div>
                  </li>
                )}
              </Fragment>
            ))}
          </ul>
        )}
        <p className="hint">{t("passwords.libraryHint")}</p>
      </fieldset>

      {message && <p className="hint sync-message">{message}</p>}
      {error && (
        <p className="error-text" role="alert">
          {error}
        </p>
      )}
    </Modal>
  );
}
