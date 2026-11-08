import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../api";
import { Modal } from "./Modal";

interface FileEditorProps {
  sessionId: string | null;
  path: string;
  onClose: () => void;
  onSaved: () => void;
}

export function FileEditor({ sessionId, path, onClose, onSaved }: FileEditorProps) {
  const { t } = useTranslation();
  const [content, setContent] = useState("");
  const [original, setOriginal] = useState("");
  const [mtime, setMtime] = useState<number | null>(null);
  const [truncated, setTruncated] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [conflict, setConflict] = useState(false);
  const name = path.split("/").pop() || path;

  const load = useCallback(async () => {
    setLoading(true);
    setError("");
    setConflict(false);
    try {
      const file = await api.vfsReadText(sessionId, path);
      setContent(file.content);
      setOriginal(file.content);
      setMtime(file.mtime);
      setTruncated(file.truncated);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, [sessionId, path]);

  useEffect(() => {
    void load();
  }, [load]);

  const dirty = content !== original;

  const save = async () => {
    if (truncated || mtime === null) return;
    setSaving(true);
    setError("");
    try {
      const next = await api.vfsWriteText(sessionId, path, content, mtime);
      setMtime(next);
      setOriginal(content);
      setConflict(false);
      onSaved();
    } catch (err) {
      const message = String(err);
      setError(message);
      if (message.includes("changed since")) setConflict(true);
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      wide
      title={t("editor.title", { name })}
      onClose={onClose}
      footer={
        <>
          <button type="button" onClick={onClose}>
            {t("editor.close")}
          </button>
          <button
            type="button"
            className="primary"
            disabled={loading || saving || truncated || !dirty}
            onClick={() => void save()}
          >
            {saving ? t("editor.saving") : t("editor.save")}
          </button>
        </>
      }
    >
      <div className="file-editor-body">
        {loading ? (
          <p className="hint">{t("editor.loading")}</p>
        ) : (
          <textarea
            className="file-editor"
            value={content}
            spellCheck={false}
            onChange={(e) => setContent(e.target.value)}
            readOnly={truncated}
          />
        )}
        {truncated && <p className="hint">{t("editor.tooLarge")}</p>}
        {conflict && (
          <div className="editor-conflict" role="alert">
            <p>{t("editor.conflict")}</p>
            <button type="button" onClick={() => void load()}>
              {t("editor.reload")}
            </button>
          </div>
        )}
        {error && !conflict && (
          <p className="error-text" role="alert">
            {error}
          </p>
        )}
      </div>
    </Modal>
  );
}
