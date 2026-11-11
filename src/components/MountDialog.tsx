import { useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type RemoteMountInfo } from "../api";
import { Modal } from "./Modal";

interface MountDialogProps {
  mounts: RemoteMountInfo[];
  onClose: () => void;
  onChanged: () => Promise<void> | void;
}

export function MountDialog({ mounts, onClose, onChanged }: MountDialogProps) {
  const { t } = useTranslation();
  const [kind, setKind] = useState<"webdav" | "s3" | "ftp">("webdav");
  const [label, setLabel] = useState("");
  const [url, setUrl] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [endpoint, setEndpoint] = useState("");
  const [region, setRegion] = useState("us-east-1");
  const [bucket, setBucket] = useState("");
  const [accessKey, setAccessKey] = useState("");
  const [secretKey, setSecretKey] = useState("");
  const [prefix, setPrefix] = useState("");
  const [host, setHost] = useState("");
  const [port, setPort] = useState("21");
  const [rootPath, setRootPath] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const create = async () => {
    setBusy(true);
    setError("");
    try {
      const id = `m${Date.now().toString(36)}`;
      const parsedPort = port.trim() === "" ? null : Number(port.trim());
      if (kind === "ftp" && parsedPort !== null && !Number.isInteger(parsedPort)) {
        setError(t("transfer.mountPortInvalid"));
        return;
      }
      const config =
        kind === "webdav"
          ? { kind: "webdav" as const, url, username, password }
          : kind === "ftp"
            ? {
                kind: "ftp" as const,
                host: host.trim(),
                port: parsedPort,
                username,
                password,
                path: rootPath.trim(),
              }
            : {
                kind: "s3" as const,
                endpoint,
                region,
                bucket,
                accessKey,
                secretKey,
                prefix,
              };
      await api.remoteMountAdd(
        id,
        label.trim() ||
          (kind === "webdav"
            ? url
            : kind === "ftp"
              ? host.trim() || rootPath.trim()
              : bucket),
        config,
      );
      await onChanged();
      onClose();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const remove = async (id: string) => {
    setError("");
    try {
      await api.remoteMountRemove(id);
      await onChanged();
    } catch (err) {
      setError(String(err));
    }
  };

  return (
    <Modal
      title={t("transfer.mountTitle")}
      onClose={onClose}
      footer={
        <>
          <button type="button" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button type="button" onClick={() => void create()} disabled={busy}>
            {t("transfer.mountCreate")}
          </button>
        </>
      }
    >
      <div className="settings-form">
        <p className="hint">{t("transfer.mountHint")}</p>
        <label className="field">
          <span>{t("transfer.mountKind")}</span>
          <select
            value={kind}
            onChange={(e) => setKind(e.target.value as "webdav" | "s3" | "ftp")}
          >
            <option value="webdav">{t("transfer.mountWebdav")}</option>
            <option value="s3">{t("transfer.mountS3")}</option>
            <option value="ftp">{t("transfer.mountFtp")}</option>
          </select>
        </label>
        <label className="field">
          <span>{t("transfer.mountLabel")}</span>
          <input value={label} onChange={(e) => setLabel(e.target.value)} />
        </label>
        {kind === "webdav" ? (
          <>
            <label className="field">
              <span>{t("transfer.mountUrl")}</span>
              <input
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                placeholder="https://dav.example.com/files"
              />
            </label>
            <label className="field">
              <span>{t("transfer.mountUser")}</span>
              <input
                value={username}
                onChange={(e) => setUsername(e.target.value)}
              />
            </label>
            <label className="field">
              <span>{t("transfer.mountPassword")}</span>
              <input
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
              />
            </label>
          </>
        ) : kind === "s3" ? (
          <>
            <label className="field">
              <span>{t("transfer.mountEndpoint")}</span>
              <input
                value={endpoint}
                onChange={(e) => setEndpoint(e.target.value)}
                placeholder="https://s3.amazonaws.com"
              />
            </label>
            <label className="field">
              <span>{t("transfer.mountRegion")}</span>
              <input value={region} onChange={(e) => setRegion(e.target.value)} />
            </label>
            <label className="field">
              <span>{t("transfer.mountBucket")}</span>
              <input value={bucket} onChange={(e) => setBucket(e.target.value)} />
            </label>
            <label className="field">
              <span>{t("transfer.mountAccessKey")}</span>
              <input
                value={accessKey}
                onChange={(e) => setAccessKey(e.target.value)}
              />
            </label>
            <label className="field">
              <span>{t("transfer.mountSecretKey")}</span>
              <input
                type="password"
                value={secretKey}
                onChange={(e) => setSecretKey(e.target.value)}
              />
            </label>
            <label className="field">
              <span>{t("transfer.mountPrefix")}</span>
              <input value={prefix} onChange={(e) => setPrefix(e.target.value)} />
            </label>
          </>
        ) : (
          <>
            <label className="field">
              <span>{t("transfer.mountHost")}</span>
              <input
                value={host}
                onChange={(e) => setHost(e.target.value)}
                placeholder="ftp.example.com"
              />
            </label>
            <label className="field">
              <span>{t("transfer.mountPort")}</span>
              <input
                value={port}
                onChange={(e) => setPort(e.target.value)}
                inputMode="numeric"
                placeholder="21"
              />
            </label>
            <label className="field">
              <span>{t("transfer.mountUser")}</span>
              <input
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                placeholder="anonymous"
              />
            </label>
            <label className="field">
              <span>{t("transfer.mountPassword")}</span>
              <input
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
              />
            </label>
            <label className="field">
              <span>{t("transfer.mountRoot")}</span>
              <input
                value={rootPath}
                onChange={(e) => setRootPath(e.target.value)}
                placeholder="/"
              />
            </label>
            <p className="hint">{t("transfer.mountFtpHint")}</p>
          </>
        )}
        {error && <p className="error-text">{error}</p>}
        {mounts.length > 0 && (
          <div className="mount-list">
            <span className="hint">{t("transfer.mountExisting")}</span>
            <ul className="plugin-list">
              {mounts.map((mount) => (
                <li key={mount.id} className="plugin-row">
                  <span className="plugin-name">{mount.label}</span>
                  <span className="plugin-kind">{mount.kind}</span>
                  <button
                    type="button"
                    className="link-btn"
                    onClick={() => void remove(mount.id)}
                  >
                    {t("transfer.mountRemove")}
                  </button>
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </Modal>
  );
}
