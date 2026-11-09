import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../api";
import { joinPath, parentPath } from "../paths";
import { useAppStore } from "../store";
import { Modal } from "./Modal";

interface FileEditorProps {
  sessionId: string | null;
  path: string;
  onClose: () => void;
  onSaved: () => void;
}

/** Idle time before an auto-saving editor writes back. */
const AUTO_SAVE_DELAY_MS = 1500;

/** `notes.txt` → `notes.copy.txt`, so the copy sorts next to the original. */
function copyName(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot > 0
    ? `${name.slice(0, dot)}.copy${name.slice(dot)}`
    : `${name}.copy`;
}

export function FileEditor({ sessionId, path, onClose, onSaved }: FileEditorProps) {
  const { t } = useTranslation();
  const setStatus = useAppStore((s) => s.setStatus);
  const autoSave = useAppStore((s) => s.editorAutoSave);
  const [content, setContent] = useState("");
  const [original, setOriginal] = useState("");
  const [mtime, setMtime] = useState<number | null>(null);
  const [truncated, setTruncated] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [conflict, setConflict] = useState(false);
  /** Non-null while the "save a copy" field is open. */
  const [saveAsName, setSaveAsName] = useState<string | null>(null);
  const name = path.split("/").pop() || path;

  const load = useCallback(async () => {
    setLoading(true);
    setError("");
    setConflict(false);
    setSaveAsName(null);
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

  /**
   * `expectedMtime` of null is what "overwrite anyway" means: the backend skips
   * the stamp check, so the user's buffer wins over whatever landed on disk.
   */
  const write = useCallback(
    async (expectedMtime: number | null, silent: boolean) => {
      if (truncated) return false;
      setSaving(true);
      if (!silent) setError("");
      try {
        const next = await api.vfsWriteText(sessionId, path, content, expectedMtime);
        setMtime(next);
        setOriginal(content);
        setConflict(false);
        onSaved();
        return true;
      } catch (err) {
        const message = String(err);
        if (message.includes("changed since")) {
          // The conflict bar carries the explanation and the three ways out,
          // so a second copy of the backend message would only add noise.
          setConflict(true);
        } else {
          setError(message);
        }
        return false;
      } finally {
        setSaving(false);
      }
    },
    [content, onSaved, path, sessionId, truncated],
  );

  const save = () => {
    if (mtime === null) return;
    void write(mtime, false);
  };

  const saveCopy = async () => {
    const next = (saveAsName ?? "").trim();
    if (!next) return;
    if (next.includes("/") || next.includes("\\")) {
      setError(t("editor.copyNameInvalid"));
      return;
    }
    const target = joinPath(parentPath(path) ?? "", next);
    setSaving(true);
    setError("");
    try {
      // A copy must never land on top of a file the user already has; the
      // default name is a suggestion, so asking for another one is cheap.
      const exists = await api
        .vfsStat(sessionId, target)
        .then(() => true)
        .catch(() => false);
      if (exists) {
        setError(t("editor.copyExists", { name: next }));
        return;
      }
      await api.vfsWriteText(sessionId, target, content, null);
      setSaveAsName(null);
      setConflict(false);
      onSaved();
      setStatus(t("editor.copySaved", { name: next }));
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  };

  // Auto-save is off by default: a remote write per pause in typing is a
  // promise the user has to make deliberately. A detected conflict parks it
  // until they pick one of the three ways out.
  useEffect(() => {
    if (!autoSave || loading || truncated || conflict || saving) return;
    if (mtime === null || saveAsName !== null || !dirty) return;
    const id = window.setTimeout(() => void write(mtime, true), AUTO_SAVE_DELAY_MS);
    return () => window.clearTimeout(id);
  }, [
    autoSave,
    conflict,
    dirty,
    loading,
    mtime,
    saveAsName,
    saving,
    truncated,
    write,
  ]);

  return (
    <Modal
      wide
      title={t("editor.title", { name })}
      onClose={onClose}
      footer={
        <>
          {autoSave && (
            <span className="editor-autosave">{t("editor.autoSaveOn")}</span>
          )}
          <button type="button" onClick={onClose}>
            {t("editor.close")}
          </button>
          <button
            type="button"
            className="primary"
            disabled={loading || saving || truncated || !dirty}
            onClick={save}
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
            {saveAsName === null ? (
              <div className="editor-conflict-actions">
                <button type="button" onClick={() => void load()}>
                  {t("editor.reload")}
                </button>
                <button
                  type="button"
                  className="danger"
                  disabled={saving}
                  onClick={() => void write(null, false)}
                >
                  {t("editor.overwrite")}
                </button>
                <button
                  type="button"
                  disabled={saving}
                  onClick={() => setSaveAsName(copyName(name))}
                >
                  {t("editor.saveCopy")}
                </button>
              </div>
            ) : (
              <div className="editor-conflict-actions">
                <input
                  className="prompt-input"
                  value={saveAsName}
                  onChange={(e) => setSaveAsName(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      void saveCopy();
                    }
                    if (e.key === "Escape") {
                      e.preventDefault();
                      e.stopPropagation();
                      setSaveAsName(null);
                    }
                  }}
                  aria-label={t("editor.copyName")}
                  placeholder={t("editor.copyName")}
                  autoFocus
                />
                <button
                  type="button"
                  className="primary"
                  disabled={saving || saveAsName.trim().length === 0}
                  onClick={() => void saveCopy()}
                >
                  {t("editor.copyConfirm")}
                </button>
                <button type="button" onClick={() => setSaveAsName(null)}>
                  {t("common.cancel")}
                </button>
              </div>
            )}
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
