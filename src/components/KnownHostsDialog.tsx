import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type KnownHostEntry } from "../api";
import { Modal } from "./Modal";

export function KnownHostsDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const [entries, setEntries] = useState<KnownHostEntry[]>([]);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [removing, setRemoving] = useState<string | null>(null);

  const load = async () => {
    setBusy(true);
    try {
      setEntries(await api.knownHostsList());
      setError("");
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const key = (entry: KnownHostEntry) =>
    `${entry.host}:${entry.port}:${entry.algorithm}`;

  const remove = async (entry: KnownHostEntry) => {
    setRemoving(key(entry));
    try {
      await api.knownHostsRemove(entry.host, entry.port);
      await load();
    } catch (err) {
      setError(String(err));
    } finally {
      setRemoving(null);
    }
  };

  const target = (entry: KnownHostEntry) =>
    entry.port === 22 ? entry.host : `[${entry.host}]:${entry.port}`;

  return (
    <Modal title={t("knownHosts.title")} onClose={onClose} className="known-hosts">
      <p className="hint">{t("knownHosts.hint")}</p>
      {busy && entries.length === 0 && <p className="hint">{t("common.loading")}</p>}
      {!busy && entries.length === 0 && (
        <p className="hint">{t("knownHosts.empty")}</p>
      )}
      {entries.length > 0 && (
        <ul className="known-host-list">
          {entries.map((entry) => (
            <li key={key(entry)} className="known-host-row">
              <div className="known-host-main">
                <span className="known-host-target">{target(entry)}</span>
                <span className="hint">{entry.algorithm}</span>
              </div>
              <code className="known-host-fingerprint" title={entry.fingerprint}>
                {entry.fingerprint}
              </code>
              <button
                type="button"
                className="link-btn danger"
                disabled={removing === key(entry)}
                onClick={() => void remove(entry)}
              >
                {t("knownHosts.remove")}
              </button>
            </li>
          ))}
        </ul>
      )}
      {error && (
        <p className="error-text" role="alert">
          {error}
        </p>
      )}
      <div className="plugin-actions">
        <button type="button" onClick={() => void load()} disabled={busy}>
          {t("knownHosts.refresh")}
        </button>
      </div>
    </Modal>
  );
}
