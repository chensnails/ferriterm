import { useEffect, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import {
  api,
  isTauri,
  type CredentialStatus,
  type HostEntry,
  type SessionKind,
  type SshKey,
  type VaultStatus,
} from "../api";
import { buildConnectArgs, type PendingConnect } from "../connect";
import { useAppStore } from "../store";
import { vaultErrorText } from "../vault-errors";
import { Modal } from "./Modal";

export function ConnectHostDialog({
  host,
  onClose,
  initialError,
}: {
  host: HostEntry;
  onClose: () => void;
  initialError?: string;
}) {
  const { t } = useTranslation();
  const hosts = useAppStore((s) => s.hosts);
  const keepaliveSecs = useAppStore((s) => s.keepaliveSecs);
  const openSession = useAppStore((s) => s.openSession);
  const setStatusMessage = useAppStore((s) => s.setStatus);
  const [status, setStatus] = useState<CredentialStatus | null>(null);
  const [vault, setVault] = useState<VaultStatus | null>(null);
  const [keys, setKeys] = useState<SshKey[]>([]);
  const [password, setPassword] = useState("");
  const [keyPath, setKeyPath] = useState(host.keyPath);
  const [passphrase, setPassphrase] = useState("");
  const [remember, setRemember] = useState(false);
  const [useSaved, setUseSaved] = useState(!initialError);
  const [unlockPassword, setUnlockPassword] = useState("");
  const [unlocking, setUnlocking] = useState(false);
  const [error, setError] = useState(initialError ?? "");
  const [busy, setBusy] = useState(false);

  const keyAuth = host.authMethod === "key";
  const keyboardAuth = host.authMethod === "keyboard-interactive";
  const agentAuth = host.authMethod === "agent";
  const needsSecret = !keyboardAuth && !agentAuth;
  const locked = vault?.mode === "master" && vault.locked;
  const jumpHost = hosts.find((item) => item.id === host.jumpHostId);
  const managedKey = keys.find((item) => item.id === host.keyId);
  const passphraseSaved = keyAuth
    ? Boolean(host.keyId ? status?.keyPassphrase : status?.passphrase)
    : false;
  const keyNeedsPassphrase = keyAuth && Boolean(managedKey?.hasPassphrase);

  useEffect(() => {
    if (!isTauri) return;
    let cancelled = false;
    void (async () => {
      try {
        const [nextVault, nextStatus, keyList] = await Promise.all([
          api.vaultStatus(),
          api.credentialStatus(host.id, host.keyId || undefined),
          keyAuth ? api.sshKeyList() : Promise.resolve([] as SshKey[]),
        ]);
        if (cancelled) return;
        setVault(nextVault);
        setStatus(nextStatus);
        setKeys(keyList);
        setUseSaved(initialError ? false : nextStatus.password);
      } catch (err) {
        if (!cancelled) setError(String(err));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [host.id, host.keyId, keyAuth]);

  const unlock = async () => {
    if (!unlockPassword) return;
    setUnlocking(true);
    setError("");
    try {
      await api.vaultUnlock(unlockPassword);
      const [nextVault, nextStatus] = await Promise.all([
        api.vaultStatus(),
        api.credentialStatus(host.id, host.keyId || undefined),
      ]);
      setVault(nextVault);
      setStatus(nextStatus);
      setUseSaved(initialError ? false : nextStatus.password);
      setUnlockPassword("");
    } catch (err) {
      setError(vaultErrorText(err, t));
    } finally {
      setUnlocking(false);
    }
  };

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (locked) {
      setError(t("connectHost.vaultLocked"));
      return;
    }
    if (keyAuth && !host.keyId && !keyPath.trim()) {
      setError(t("connectHost.keyMissing"));
      return;
    }
    if (keyAuth && keyNeedsPassphrase && !passphraseSaved && !passphrase) {
      setError(t("connectHost.passphraseRequired"));
      return;
    }
    if (!keyAuth && needsSecret && !useSaved && !password) {
      setError(t("connectHost.passwordRequired"));
      return;
    }
    setError("");
    setBusy(true);
    try {
      const args = buildConnectArgs(host, hosts, keepaliveSecs);
      if (keyAuth) {
        if (!host.keyId) args.keyPath = keyPath.trim();
        if (passphrase) {
          args.keyPassphrase = passphrase;
          if (remember) {
            if (host.keyId) {
              await api.saveSecret(`keypp:${host.keyId}`, passphrase);
            } else {
              await api.saveHostCredential(host.id, null, passphrase);
            }
          }
        }
      } else if (needsSecret && !useSaved) {
        args.password = password;
        if (remember && password) {
          await api.saveHostCredential(host.id, password, null);
        }
      }
      setStatusMessage(t("connect.connecting"));
      // An SFTP host opens the file browser tab; both share the SSH transport.
      const kind: SessionKind = host.protocol === "sftp" ? "sftp" : "ssh";
      const pending: PendingConnect =
        kind === "sftp" ? { type: "sftp", args } : { type: "ssh", args };
      openSession(
        kind,
        host.name || `${host.username}@${host.host}`,
        pending,
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
          <button
            type="submit"
            form="connect-host-form"
            className="primary"
            data-testid="connect-submit"
            disabled={busy}
          >
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

        {locked && (
          <fieldset className="settings-group vault-unlock">
            <legend>{t("connectHost.vaultLocked")}</legend>
            <label className="field">
              <span>{t("vault.masterPassword")}</span>
              <input
                autoFocus
                type="password"
                value={unlockPassword}
                onChange={(e) => setUnlockPassword(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    void unlock();
                  }
                }}
              />
            </label>
            <div className="plugin-actions">
              <button
                type="button"
                onClick={() => void unlock()}
                disabled={unlocking}
              >
                {t("vault.unlock")}
              </button>
            </div>
          </fieldset>
        )}

        {!locked && agentAuth && (
          <p className="hint">{t("connectHost.agentHint")}</p>
        )}
        {!locked && keyboardAuth && (
          <p className="hint">{t("connectHost.keyboardInteractiveHint")}</p>
        )}
        {!locked && keyAuth && (
          <>
            {managedKey ? (
              <div className="saved-secret">
                <span className="badge">{t("connectHost.managedKey")}</span>
                <span className="saved-secret-name">{managedKey.name}</span>
                <span className="hint">{managedKey.fingerprint}</span>
                {passphraseSaved && (
                  <span className="badge">
                    {t("connectHost.passphraseSaved")}
                  </span>
                )}
              </div>
            ) : (
              <label className="field">
                <span>{t("connectHost.keyPath")}</span>
                <input
                  autoFocus
                  value={keyPath}
                  onChange={(e) => setKeyPath(e.target.value)}
                  placeholder="~/.ssh/id_ed25519"
                />
              </label>
            )}
            <label className="field">
              <span>
                {t("connectHost.passphrase")}
                {keyNeedsPassphrase ? "" : ` (${t("common.optional")})`}
              </span>
              <input
                type="password"
                value={passphrase}
                onChange={(e) => setPassphrase(e.target.value)}
                placeholder={t("connectHost.passphrasePlaceholder")}
              />
            </label>
            {passphraseSaved && (
              <p className="hint">{t("connectHost.passphraseSavedHint")}</p>
            )}
            <label className="checkbox">
              <input
                type="checkbox"
                checked={remember}
                onChange={(e) => setRemember(e.target.checked)}
              />
              <span>{t("connectHost.rememberPassphrase")}</span>
            </label>
          </>
        )}
        {!locked && !keyAuth && needsSecret && (
          <>
            {status?.password ? (
              <div className="saved-secret">
                <span className="badge">{t("connectHost.passwordSaved")}</span>
                <label className="checkbox">
                  <input
                    type="checkbox"
                    checked={useSaved}
                    onChange={(e) => setUseSaved(e.target.checked)}
                  />
                  <span>{t("connectHost.useSaved")}</span>
                </label>
              </div>
            ) : null}
            {!useSaved || !status?.password ? (
              <label className="field">
                <span>{t("connectHost.password")}</span>
                <input
                  autoFocus
                  type="password"
                  data-testid="connect-password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  placeholder={t("connectHost.passwordPlaceholder")}
                />
              </label>
            ) : null}
            {(!status?.password || !useSaved) && (
              <label className="checkbox">
                <input
                  type="checkbox"
                  checked={remember}
                  onChange={(e) => setRemember(e.target.checked)}
                />
                <span>{t("connectHost.remember")}</span>
              </label>
            )}
          </>
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
