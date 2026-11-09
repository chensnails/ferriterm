import { useRef, useState, type ChangeEvent } from "react";
import { useTranslation } from "react-i18next";
import {
  api,
  type ImportFormat,
  type ImportPreview,
  type ImportedHost,
} from "../api";
import { useAppStore } from "../store";
import { Modal } from "./Modal";

type FormatChoice = ImportFormat | "auto";

function decodeFile(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer);
  if (bytes[0] === 0xff && bytes[1] === 0xfe) {
    return new TextDecoder("utf-16le").decode(bytes.subarray(2));
  }
  if (bytes[0] === 0xfe && bytes[1] === 0xff) {
    return new TextDecoder("utf-16be").decode(bytes.subarray(2));
  }
  return new TextDecoder("utf-8").decode(bytes);
}

export function ImportDialog({
  parentId,
  onClose,
}: {
  parentId: string | null;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const loadHosts = useAppStore((s) => s.loadHosts);
  const setStatus = useAppStore((s) => s.setStatus);
  const fileRef = useRef<HTMLInputElement>(null);

  const [format, setFormat] = useState<FormatChoice>("auto");
  const [fileName, setFileName] = useState("");
  const [content, setContent] = useState("");
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const formatLabel = (value: ImportFormat) =>
    value === "ssh_config"
      ? t("import.formatSshConfig")
      : value === "xshell"
        ? t("import.formatXshell")
        : t("import.formatFinalShell");

  const onFile = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;
    setFileName(file.name);
    setError("");
    setPreview(null);
    try {
      setContent(decodeFile(await file.arrayBuffer()));
    } catch (err) {
      setError(String(err));
    }
  };

  const runPreview = async () => {
    setError("");
    setPreview(null);
    setBusy(true);
    try {
      setPreview(await api.importPreview(format, content));
    } catch (err) {
      setError(t("import.error", { error: String(err) }));
    } finally {
      setBusy(false);
    }
  };

  const runImport = async () => {
    if (!preview) return;
    setBusy(true);
    setError("");
    try {
      for (const host of preview.hosts) {
        await api.createHost(toHostInput(host, parentId));
      }
      await loadHosts();
      setStatus(t("import.imported", { count: preview.hosts.length }));
      onClose();
    } catch (err) {
      setError(t("import.error", { error: String(err) }));
    } finally {
      setBusy(false);
    }
  };

  const canPreview = content.trim().length > 0 && !busy;

  return (
    <Modal
      title={t("import.title")}
      onClose={onClose}
      footer={
        <>
          <button type="button" onClick={onClose}>
            {t("common.cancel")}
          </button>
          {preview ? (
            <button
              type="button"
              className="primary"
              disabled={busy || preview.hosts.length === 0}
              onClick={runImport}
            >
              {t("import.import", { count: preview.hosts.length })}
            </button>
          ) : (
            <button
              type="button"
              className="primary"
              disabled={!canPreview}
              onClick={runPreview}
            >
              {busy ? t("import.parsing") : t("import.preview")}
            </button>
          )}
        </>
      }
    >
      <div className="form-grid">
        <label className="field">
          <span>{t("import.format")}</span>
          <select
            value={format}
            onChange={(e) => {
              setFormat(e.target.value as FormatChoice);
              setPreview(null);
            }}
          >
            <option value="auto">{t("import.formatAuto")}</option>
            <option value="ssh_config">{t("import.formatSshConfig")}</option>
            <option value="xshell">{t("import.formatXshell")}</option>
            <option value="finalshell">{t("import.formatFinalShell")}</option>
          </select>
        </label>

        <div className="import-file">
          <button type="button" onClick={() => fileRef.current?.click()}>
            {t("import.chooseFile")}
          </button>
          <span className="hint">{fileName || t("import.noFile")}</span>
          <input
            ref={fileRef}
            type="file"
            accept=".xsh,.json,.config,.txt,text/plain"
            hidden
            onChange={onFile}
          />
        </div>

        <label className="field">
          <span>{t("import.orPaste")}</span>
          <textarea
            rows={7}
            value={content}
            onChange={(e) => {
              setContent(e.target.value);
              setPreview(null);
            }}
            placeholder={t("import.placeholder")}
            spellCheck={false}
          />
        </label>

        {preview && (
          <div className="import-preview">
            <p className="hint">
              {t("import.detected", { format: formatLabel(preview.format) })}
            </p>
            <p className="hint">
              {t("import.found", { count: preview.hosts.length })}
            </p>
            <ul className="import-list">
              {preview.hosts.map((host, index) => (
                <li key={`${host.host}:${host.port}:${index}`}>
                  <span className="import-name">{host.name}</span>
                  <span className="import-target">
                    {host.username ? `${host.username}@` : ""}
                    {host.host}:{host.port}
                  </span>
                </li>
              ))}
            </ul>
          </div>
        )}

        {error && (
          <p className="error-text" role="alert">
            {error}
          </p>
        )}
      </div>
    </Modal>
  );
}

function toHostInput(host: ImportedHost, parentId: string | null) {
  return {
    id: crypto.randomUUID(),
    name: host.name,
    host: host.host,
    port: host.port,
    username: host.username,
    authMethod: "password",
    keyPath: "",
    keyId: "",
    jumpHostId: "",
    x11: false,
    parentId,
    isFolder: false,
    production: false,
    notes: "",
    tagIds: [] as string[],
  };
}
