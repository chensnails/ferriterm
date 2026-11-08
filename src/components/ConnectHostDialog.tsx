import { useEffect, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { api, type HostEntry } from "../api";
import { buildConnectArgs } from "../connect";
import { useAppStore } from "../store";
import { Modal } from "./Modal";

export function ConnectHostDialog({
  host,
  onClose,
}: {
  host: HostEntry;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const hosts = useAppStore((s) => s.hosts);
  const keepaliveSecs = useAppStore((s) => s.keepaliveSecs);
  const openSession = useAppStore((s) => s.openSession);
  const setStatus = useAppStore((s) => s.setStatus);
  const [password, setPassword] = useState("");
  const [keyPath, setKeyPath] = useState(host.keyPath);
  const [passphrase, setPassphrase] = useState("");
  const [remember, setRemember] = useState(false);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const keyAuth = host.authMethod === "key";
  const keyboardAuth = host.authMethod === "keyboard-interactive";
  const agentAuth = host.authMethod === "agent";
  const jumpHost = hosts.find((item) => item.id === host.jumpHostId);

  useEffect(() => {
    if (keyboardAuth || agentAuth) return;
    let cancelled = false;
    api
      .getCredential(host.id)
      .then((secret) => {
        if (cancelled || !secret) return;
        if (keyAuth) setPassphrase(secret);
        else setPassword(secret);
        setRemember(true);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [host.id, keyAuth, keyboardAuth, agentAuth]);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (keyAuth ? !keyPath.trim() : !keyboardAuth && !agentAuth && !password) {
      return;
    }
    setError("");
    setBusy(true);
    try {
      const args = await buildConnectArgs(
        host,
        hosts,
        {
          password: keyAuth ? undefined : password,
          keyPath: keyPath.trim(),
          passphrase: passphrase || undefined,
        },
        keepaliveSecs,
      );
      if (!keyboardAuth && !agentAuth) {
        const secret = keyAuth ? passphrase : password;
        if (remember && secret) {
          await api
            .saveCredential(host.id, secret)
            .catch((err) => setStatus(String(err), true));
        } else {
          await api.deleteCredential(host.id).catch(() => {});
        }
      }
      setStatus(t("connect.connecting"));
      openSession(
        "ssh",
        host.name || `${host.username}@${host.host}`,
        { type: "ssh", args },
        host.production,
        host.id,
      );
      onClose();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      title={t("connectHost.title", { name: host.name })}
      onClose={onClose}
      footer={
        <>
          <button type="button" onClick={onClose}>
            {t("connectHost.cancel")}
          </button>
          <button type="submit" form="connect-host-form" className="primary">
            {t("connectHost.submit")}
          </button>
        </>
      }
    >
      <form id="connect-host-form" className="form-grid" onSubmit={submit}>
        <p className="hint">
          {t("connectHost.target", {
            username: host.username,
            host: host.host,
            port: host.port,
          })}
        </p>
        {jumpHost && (
          <p className="hint">
            {t("connectHost.viaJump", {
              name: jumpHost.name || `${jumpHost.username}@${jumpHost.host}`,
            })}
          </p>
        )}
        {host.x11 && <p className="hint">{t("connectHost.x11Enabled")}</p>}
        {agentAuth ? (
          <p className="hint">{t("connectHost.agentHint")}</p>
        ) : keyboardAuth ? (
          <p className="hint">{t("connectHost.keyboardInteractiveHint")}</p>
        ) : keyAuth ? (
          <>
            <label className="field">
              <span>{t("connectHost.keyPath")}</span>
              <input
                autoFocus
                value={keyPath}
                onChange={(e) => setKeyPath(e.target.value)}
                placeholder="~/.ssh/id_ed25519"
              />
            </label>
            <label className="field">
              <span>{t("connectHost.passphrase")}</span>
              <input
                type="password"
                value={passphrase}
                onChange={(e) => setPassphrase(e.target.value)}
                placeholder={t("connectHost.passphrasePlaceholder")}
              />
            </label>
          </>
        ) : (
          <label className="field">
            <span>{t("connectHost.password")}</span>
            <input
              autoFocus
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder={t("connectHost.passwordPlaceholder")}
            />
          </label>
        )}
        {!keyboardAuth && !agentAuth && (
          <label className="checkbox">
            <input
              type="checkbox"
              checked={remember}
              onChange={(e) => setRemember(e.target.checked)}
            />
            <span>{t("connectHost.remember")}</span>
          </label>
        )}
        {error && (
          <p className="error-text" role="alert">
            {error}
          </p>
        )}
        {host.notes && <p className="hint">{host.notes}</p>}
      </form>
    </Modal>
  );
}
