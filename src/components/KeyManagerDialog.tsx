import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type KeyCandidate, type SshKey } from "../api";
import { Modal } from "./Modal";

export function KeyManagerDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const [keys, setKeys] = useState<SshKey[]>([]);
  const [candidates, setCandidates] = useState<KeyCandidate[]>([]);
  const [tab, setTab] = useState<"library" | "import" | "generate">("library");
  const [name, setName] = useState("");
  const [path, setPath] = useState("");
  const [pasted, setPasted] = useState("");
  const [passphrase, setPassphrase] = useState("");
  const [algorithm, setAlgorithm] = useState("ed25519");
  const [comment, setComment] = useState("");
  const [generated, setGenerated] = useState<SshKey | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");

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

  const copy = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setMessage(t("keys.copied"));
    } catch (err) {
      setError(String(err));
    }
  };

  return (
    <Modal title={t("keys.title")} onClose={onClose} className="key-manager">
      <div className="settings-tabs">
        {(["library", "import", "generate"] as const).map((item) => (
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
              {keys.map((key) => (
                <li key={key.id} className="plugin-row key-row">
                  <span className="plugin-main">
                    <span className="plugin-name">
                      {key.name}
                      {key.hasPassphrase && (
                        <span className="badge">{t("keys.encrypted")}</span>
                      )}
                    </span>
                    <span className="hint plugin-desc">
                      {key.algorithm} · {key.fingerprint}
                    </span>
                    {key.comment && (
                      <span className="hint plugin-desc">{key.comment}</span>
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
                    className="link-btn danger"
                    onClick={() => void remove(key)}
                  >
                    {t("common.delete")}
                  </button>
                </li>
              ))}
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
            <legend>{t("keys.manualTitle")}</legend>
            <label className="field">
              <span>{t("keys.name")}</span>
              <input value={name} onChange={(e) => setName(e.target.value)} />
            </label>
            <label className="field">
              <span>{t("keys.path")}</span>
              <input
                value={path}
                onChange={(e) => setPath(e.target.value)}
                placeholder="~/.ssh/id_ed25519"
              />
            </label>
            <label className="field">
              <span>{t("keys.paste")}</span>
              <textarea
                rows={4}
                value={pasted}
                onChange={(e) => setPasted(e.target.value)}
                placeholder="-----BEGIN OPENSSH PRIVATE KEY-----"
              />
            </label>
            <div className="plugin-actions">
              <button
                type="button"
                disabled={busy || (!path.trim() && !pasted.trim())}
                onClick={() =>
                  void importKey({
                    path: path.trim() || undefined,
                    text: pasted.trim() || undefined,
                    name: name.trim(),
                    passphrase,
                  })
                }
              >
                {t("keys.import")}
              </button>
            </div>
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

      {message && <p className="hint sync-message">{message}</p>}
      {error && (
        <p className="error-text" role="alert">
          {error}
        </p>
      )}
    </Modal>
  );
}
