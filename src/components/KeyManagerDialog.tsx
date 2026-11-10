import { Fragment, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, isTauri, type KeyCandidate, type SshKey } from "../api";
import { useAppStore } from "../store";
import { Modal } from "./Modal";

interface KeyPreview {
  algorithm: string;
  fingerprint: string;
  comment: string;
  encrypted: boolean;
}

/** How close a managed key is to its operator-set expiry date. */
function expiryInfo(expiresAt: string): {
  state: "none" | "ok" | "soon" | "expired";
  days: number;
} {
  const trimmed = expiresAt.trim();
  if (!/^\d{4}-\d{2}-\d{2}$/.test(trimmed)) return { state: "none", days: 0 };
  const target = new Date(`${trimmed}T00:00:00`).getTime();
  if (Number.isNaN(target)) return { state: "none", days: 0 };
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const days = Math.round((target - today.getTime()) / 86_400_000);
  if (days < 0) return { state: "expired", days: -days };
  if (days <= 30) return { state: "soon", days };
  return { state: "ok", days };
}

export function KeyManagerDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const [keys, setKeys] = useState<SshKey[]>([]);
  const [candidates, setCandidates] = useState<KeyCandidate[]>([]);
  const [tab, setTab] = useState<
    "library" | "import" | "generate" | "backup"
  >("library");
  const [name, setName] = useState("");
  const [path, setPath] = useState("");
  const [pasted, setPasted] = useState("");
  const [preview, setPreview] = useState<KeyPreview | null>(null);
  const [previewError, setPreviewError] = useState("");
  const [nameTouched, setNameTouched] = useState(false);
  const previewRequest = useRef(0);
  const [passphrase, setPassphrase] = useState("");
  const [algorithm, setAlgorithm] = useState("ed25519");
  const [comment, setComment] = useState("");
  const [generated, setGenerated] = useState<SshKey | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draftNote, setDraftNote] = useState("");
  const [draftExpiry, setDraftExpiry] = useState("");
  const [rotatePassphrase, setRotatePassphrase] = useState("");
  const [backupPassphrase, setBackupPassphrase] = useState("");
  const [publicPayload, setPublicPayload] = useState("");
  const [bundlePayload, setBundlePayload] = useState("");
  const [bundleInput, setBundleInput] = useState("");

  const reload = async () => {
    try {
      const [list, scan] = await Promise.all([
        api.sshKeyList(),
        api.sshKeyScan().catch(() => [] as KeyCandidate[]),
      ]);
      setKeys(list);
      setCandidates(scan);
    } catch (err) {
      setError(String(err));
    }
  };

  useEffect(() => {
    void reload();
  }, []);

  // Inspect pasted key text so the user sees the algorithm, fingerprint and
  // encryption state before the key is written to the vault.
  useEffect(() => {
    const text = pasted.trim();
    if (!text || !isTauri) {
      setPreview(null);
      setPreviewError("");
      return;
    }
    const request = previewRequest.current + 1;
    previewRequest.current = request;
    const timer = window.setTimeout(async () => {
      try {
        const info = await api.sshKeyInspect(text);
        if (previewRequest.current !== request) return;
        setPreview(info);
        setPreviewError("");
        if (!nameTouched) {
          setName(
            info.comment.trim() ||
              `${info.algorithm} ${info.fingerprint.slice(0, 19)}`,
          );
        }
      } catch (err) {
        if (previewRequest.current !== request) return;
        setPreview(null);
        setPreviewError(String(err));
      }
    }, 350);
    return () => window.clearTimeout(timer);
  }, [pasted, nameTouched]);

  const importKey = async (source: {
    path?: string;
    text?: string;
    name: string;
    passphrase?: string;
  }) => {
    setBusy(true);
    setError("");
    setMessage("");
    try {
      const key = await api.sshKeyImport({
        id: crypto.randomUUID(),
        name: source.name,
        path: source.path ?? null,
        text: source.text ?? null,
        passphrase: source.passphrase || null,
      });
      setMessage(t("keys.imported", { name: key.name }));
      setPath("");
      setPasted("");
      setPreview(null);
      setPreviewError("");
      setName("");
      setNameTouched(false);
      setPassphrase("");
      await reload();
      setTab("library");
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const generate = async () => {
    setBusy(true);
    setError("");
    setMessage("");
    try {
      const key = await api.sshKeyGenerate({
        id: crypto.randomUUID(),
        name,
        algorithm,
        passphrase: passphrase || null,
        comment,
      });
      setGenerated(key);
      setMessage(t("keys.generated", { name: key.name }));
      setName("");
      setPassphrase("");
      await reload();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const remove = async (key: SshKey) => {
    setError("");
    try {
      await api.sshKeyDelete(key.id);
      setMessage(t("keys.deleted", { name: key.name }));
      await reload();
    } catch (err) {
      setError(String(err));
    }
  };

  const pasteFromClipboard = async () => {
    setError("");
    try {
      const text = await navigator.clipboard.readText();
      if (!text.trim()) {
        setMessage(t("keys.clipboardEmpty"));
        return;
      }
      setNameTouched(false);
      setPasted(text.trim());
    } catch {
      setError(t("keys.clipboardError"));
    }
  };

  const copy = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setMessage(t("keys.copied"));
    } catch (err) {
      setError(String(err));
    }
  };

  const startEdit = (key: SshKey) => {
    setEditingId(key.id);
    setDraftNote(key.notes);
    setDraftExpiry(key.expiresAt);
    setRotatePassphrase("");
    setError("");
  };

  const saveMetadata = async (key: SshKey) => {
    setBusy(true);
    setError("");
    try {
      const updated = await api.sshKeyUpdateMetadata(
        key.id,
        draftNote,
        draftExpiry,
      );
      setKeys((list) =>
        list.map((item) => (item.id === updated.id ? updated : item)),
      );
      setEditingId(null);
      setMessage(t("keys.metadataSaved", { name: updated.name }));
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const rotate = async (key: SshKey) => {
    const confirmed = await useAppStore.getState().askConfirm({
      title: t("keys.rotate"),
      body: t("keys.rotateConfirm", { name: key.name }),
      confirmLabel: t("keys.rotate"),
      danger: true,
    });
    if (!confirmed) return;
    setBusy(true);
    setError("");
    try {
      const updated = await api.sshKeyRotate(key.id, rotatePassphrase, "");
      setKeys((list) =>
        list.map((item) => (item.id === updated.id ? updated : item)),
      );
      setEditingId(null);
      setRotatePassphrase("");
      setMessage(t("keys.rotated", { name: updated.name }));
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const exportPublic = async () => {
    setBusy(true);
    setError("");
    try {
      setPublicPayload(await api.sshKeyExportPublic());
      setMessage(t("keys.publicReady"));
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const exportBundle = async () => {
    if (!backupPassphrase.trim()) return;
    setBusy(true);
    setError("");
    try {
      setBundlePayload(await api.sshKeyExportBundle(backupPassphrase));
      setMessage(t("keys.bundleReady"));
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const importBundle = async () => {
    const payload = bundleInput.trim();
    if (!payload || !backupPassphrase.trim()) return;
    setBusy(true);
    setError("");
    try {
      const stats = await api.sshKeyImportBundle(payload, backupPassphrase);
      setMessage(
        t("keys.bundleImported", {
          imported: stats.imported,
          skipped: stats.skipped,
        }),
      );
      setBundleInput("");
      await reload();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal title={t("keys.title")} onClose={onClose} className="key-manager">
      <div className="settings-tabs">
        {(["library", "import", "generate", "backup"] as const).map((item) => (
          <button
            key={item}
            type="button"
            className={`settings-tab${tab === item ? " active" : ""}`}
            aria-current={tab === item ? "page" : undefined}
            onClick={() => setTab(item)}
          >
            {t(`keys.tab.${item}`)}
          </button>
        ))}
      </div>

      {tab === "library" && (
        <fieldset className="settings-group">
          {keys.length === 0 ? (
            <p className="hint">{t("keys.empty")}</p>
          ) : (
            <ul className="plugin-list">
              {keys.map((key) => {
                const expiry = expiryInfo(key.expiresAt);
                return (
                  <Fragment key={key.id}>
                    <li className="plugin-row key-row">
                      <span className="plugin-main">
                        <span className="plugin-name">
                          {key.name}
                          {key.hasPassphrase && (
                            <span className="badge">{t("keys.encrypted")}</span>
                          )}
                          {expiry.state === "expired" && (
                            <span className="badge danger">
                              {t("keys.expired", { days: expiry.days })}
                            </span>
                          )}
                          {expiry.state === "soon" && (
                            <span className="badge warn">
                              {t("keys.expiresSoon", { days: expiry.days })}
                            </span>
                          )}
                        </span>
                        <span className="hint plugin-desc">
                          {key.algorithm} · {key.fingerprint}
                        </span>
                        {key.comment && (
                          <span className="hint plugin-desc">
                            {key.comment}
                          </span>
                        )}
                        {key.notes && (
                          <span className="hint plugin-desc">{key.notes}</span>
                        )}
                        {key.expiresAt && (
                          <span className="hint plugin-desc">
                            {t("keys.expiresOn", { date: key.expiresAt })}
                          </span>
                        )}
                      </span>
                      <button
                        type="button"
                        className="link-btn"
                        onClick={() => void copy(key.publicKey)}
                      >
                        {t("keys.copyPublic")}
                      </button>
                      <button
                        type="button"
                        className="link-btn"
                        onClick={() => startEdit(key)}
                      >
                        {t("keys.edit")}
                      </button>
                      <button
                        type="button"
                        className="link-btn danger"
                        onClick={() => void remove(key)}
                      >
                        {t("common.delete")}
                      </button>
                    </li>
                    {editingId === key.id && (
                      <li className="key-editor">
                        <label className="field">
                          <span>{t("keys.note")}</span>
                          <textarea
                            rows={2}
                            value={draftNote}
                            maxLength={2000}
                            placeholder={t("keys.notePlaceholder")}
                            onChange={(e) => setDraftNote(e.target.value)}
                          />
                        </label>
                        <label className="field">
                          <span>{t("keys.expiresAt")}</span>
                          <input
                            type="date"
                            value={draftExpiry}
                            onChange={(e) => setDraftExpiry(e.target.value)}
                          />
                        </label>
                        <span className="hint">{t("keys.expiresHint")}</span>
                        <div className="plugin-actions">
                          <button
                            type="button"
                            className="primary"
                            disabled={busy}
                            onClick={() => void saveMetadata(key)}
                          >
                            {t("keys.save")}
                          </button>
                          <button
                            type="button"
                            disabled={busy}
                            onClick={() => setEditingId(null)}
                          >
                            {t("common.cancel")}
                          </button>
                        </div>
                        <div className="key-rotate">
                          <span className="hint">{t("keys.rotateHint")}</span>
                          <div className="field-inline">
                            <input
                              type="password"
                              value={rotatePassphrase}
                              placeholder={t("keys.rotatePassphrase")}
                              onChange={(e) =>
                                setRotatePassphrase(e.target.value)
                              }
                            />
                            <button
                              type="button"
                              className="link-btn danger"
                              disabled={busy}
                              onClick={() => void rotate(key)}
                            >
                              {t("keys.rotate")}
                            </button>
                          </div>
                        </div>
                      </li>
                    )}
                  </Fragment>
                );
              })}
            </ul>
          )}
          <p className="hint">{t("keys.libraryHint")}</p>
        </fieldset>
      )}

      {tab === "import" && (
        <>
          <fieldset className="settings-group">
            <legend>{t("keys.scanTitle")}</legend>
            <p className="hint">{t("keys.scanHint")}</p>
            {candidates.length === 0 ? (
              <p className="hint">{t("keys.scanEmpty")}</p>
            ) : (
              <ul className="plugin-list">
                {candidates.map((candidate) => (
                  <li key={candidate.path} className="plugin-row">
                    <span className="plugin-main">
                      <span className="plugin-name">
                        {candidate.name}
                        {candidate.encrypted && (
                          <span className="badge">{t("keys.encrypted")}</span>
                        )}
                      </span>
                      <span className="hint plugin-desc">
                        {candidate.algorithm} · {candidate.fingerprint}
                      </span>
                    </span>
                    <button
                      type="button"
                      className="link-btn"
                      disabled={busy}
                      onClick={() =>
                        void importKey({
                          path: candidate.path,
                          name: candidate.name,
                          passphrase,
                        })
                      }
                    >
                      {t("keys.import")}
                    </button>
                  </li>
                ))}
              </ul>
            )}
            <label className="field">
              <span>{t("keys.passphraseForImport")}</span>
              <input
                type="password"
                value={passphrase}
                onChange={(e) => setPassphrase(e.target.value)}
                placeholder={t("keys.passphraseOptional")}
              />
            </label>
          </fieldset>
          <fieldset className="settings-group">
            <legend>{t("keys.pasteTitle")}</legend>
            <p className="hint">{t("keys.pasteHint")}</p>
            <label className="field">
              <span>{t("keys.paste")}</span>
              <textarea
                rows={6}
                className="key-paste"
                spellCheck={false}
                value={pasted}
                onChange={(e) => setPasted(e.target.value)}
                placeholder="-----BEGIN OPENSSH PRIVATE KEY-----"
              />
            </label>
            <div className="plugin-actions">
              <button type="button" onClick={() => void pasteFromClipboard()}>
                {t("keys.pasteFromClipboard")}
              </button>
              {pasted.trim() && (
                <button
                  type="button"
                  className="link-btn"
                  onClick={() => {
                    setPasted("");
                    setPreview(null);
                    setPreviewError("");
                  }}
                >
                  {t("common.clear")}
                </button>
              )}
            </div>
            {preview && (
              <div className="key-preview">
                <span className="plugin-name">
                  {t("keys.previewOk")}
                  {preview.encrypted && (
                    <span className="badge warn">{t("keys.encrypted")}</span>
                  )}
                </span>
                <span className="hint plugin-desc">
                  {preview.algorithm} · {preview.fingerprint}
                </span>
                {preview.comment && (
                  <span className="hint plugin-desc">{preview.comment}</span>
                )}
                {preview.encrypted && !passphrase && (
                  <span className="hint">{t("keys.passphraseNeeded")}</span>
                )}
              </div>
            )}
            {previewError && (
              <p className="hint error-text">{t("keys.previewError")}</p>
            )}
            <div className="settings-inline">
              <label className="field">
                <span>{t("keys.name")}</span>
                <input
                  value={name}
                  onChange={(e) => {
                    setNameTouched(true);
                    setName(e.target.value);
                  }}
                  placeholder={t("keys.namePlaceholder")}
                />
              </label>
              <label className="field">
                <span>{t("keys.path")}</span>
                <input
                  value={path}
                  onChange={(e) => setPath(e.target.value)}
                  placeholder="~/.ssh/id_ed25519"
                />
              </label>
            </div>
            <div className="plugin-actions">
              <button
                type="button"
                className="primary"
                disabled={busy || !pasted.trim() || !preview}
                onClick={() =>
                  void importKey({
                    text: pasted.trim(),
                    name: name.trim(),
                    passphrase,
                  })
                }
              >
                {t("keys.import")}
              </button>
            </div>
            {path.trim() && (
              <div className="plugin-actions">
                <button
                  type="button"
                  disabled={busy}
                  onClick={() =>
                    void importKey({
                      path: path.trim(),
                      name: name.trim(),
                      passphrase,
                    })
                  }
                >
                  {t("keys.importFromPath")}
                </button>
              </div>
            )}
          </fieldset>
        </>
      )}

      {tab === "generate" && (
        <fieldset className="settings-group">
          <label className="field">
            <span>{t("keys.name")}</span>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={t("keys.namePlaceholder")}
            />
          </label>
          <label className="field">
            <span>{t("keys.algorithm")}</span>
            <select
              value={algorithm}
              onChange={(e) => setAlgorithm(e.target.value)}
            >
              <option value="ed25519">Ed25519</option>
              <option value="rsa">RSA 2048</option>
              <option value="ecdsa">ECDSA P-256</option>
            </select>
          </label>
          <label className="field">
            <span>{t("keys.comment")}</span>
            <input
              value={comment}
              onChange={(e) => setComment(e.target.value)}
              placeholder={t("keys.commentPlaceholder")}
            />
          </label>
          <label className="field">
            <span>{t("keys.passphrase")}</span>
            <input
              type="password"
              value={passphrase}
              onChange={(e) => setPassphrase(e.target.value)}
              placeholder={t("keys.passphraseOptional")}
            />
          </label>
          <div className="plugin-actions">
            <button type="button" disabled={busy} onClick={() => void generate()}>
              {t("keys.generate")}
            </button>
          </div>
          {generated && (
            <div className="generated-key">
              <span className="hint">{t("keys.publicKey")}</span>
              <textarea readOnly rows={3} value={generated.publicKey} />
              <button
                type="button"
                className="link-btn"
                onClick={() => void copy(generated.publicKey)}
              >
                {t("keys.copyPublic")}
              </button>
            </div>
          )}
        </fieldset>
      )}

      {tab === "backup" && (
        <>
          <fieldset className="settings-group">
            <legend>{t("keys.backupTitle")}</legend>
            <p className="hint">{t("keys.backupHint")}</p>
            <label className="field">
              <span>{t("keys.bundlePassphrase")}</span>
              <input
                type="password"
                value={backupPassphrase}
                onChange={(e) => setBackupPassphrase(e.target.value)}
                placeholder={t("keys.bundlePassphrasePlaceholder")}
              />
            </label>
            <div className="plugin-actions">
              <button
                type="button"
                disabled={busy || keys.length === 0}
                onClick={() => void exportPublic()}
              >
                {t("keys.exportPublic")}
              </button>
              <button
                type="button"
                disabled={busy || keys.length === 0 || !backupPassphrase.trim()}
                onClick={() => void exportBundle()}
              >
                {t("keys.exportBundle")}
              </button>
            </div>
            {publicPayload && (
              <div className="key-export">
                <span className="plugin-name">
                  {t("keys.publicReady")}
                  <button
                    type="button"
                    className="link-btn"
                    onClick={() => void copy(publicPayload)}
                  >
                    {t("keys.copyPayload")}
                  </button>
                </span>
                <textarea readOnly rows={4} value={publicPayload} />
              </div>
            )}
            {bundlePayload && (
              <div className="key-export">
                <span className="plugin-name">
                  {t("keys.bundleReady")}
                  <button
                    type="button"
                    className="link-btn"
                    onClick={() => void copy(bundlePayload)}
                  >
                    {t("keys.copyPayload")}
                  </button>
                </span>
                <textarea readOnly rows={4} value={bundlePayload} />
              </div>
            )}
          </fieldset>
          <fieldset className="settings-group">
            <legend>{t("keys.importBundle")}</legend>
            <p className="hint">{t("keys.importBundleHint")}</p>
            <label className="field">
              <span>{t("keys.bundlePayload")}</span>
              <textarea
                rows={4}
                spellCheck={false}
                value={bundleInput}
                placeholder={t("keys.bundlePayloadPlaceholder")}
                onChange={(e) => setBundleInput(e.target.value)}
              />
            </label>
            <div className="plugin-actions">
              <button
                type="button"
                className="primary"
                disabled={
                  busy || !bundleInput.trim() || !backupPassphrase.trim()
                }
                onClick={() => void importBundle()}
              >
                {t("keys.importBundle")}
              </button>
            </div>
          </fieldset>
        </>
      )}

      {message && <p className="hint sync-message">{message}</p>}
      {error && (
        <p className="error-text" role="alert">
          {error}
        </p>
      )}
    </Modal>
  );
}
