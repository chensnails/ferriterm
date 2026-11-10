import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  api,
  isTauri,
  onUpdateProgress,
  type UpdateInfo,
  type UpdateProgress,
} from "../api";
import { formatBytes } from "../format";

export function UpdatePanel() {
  const { t } = useTranslation();
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [busy, setBusy] = useState<"check" | "install" | null>(null);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [progress, setProgress] = useState<UpdateProgress | null>(null);

  useEffect(() => {
    if (!isTauri) return;
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void onUpdateProgress(setProgress).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  if (!isTauri) return null;

  const check = async () => {
    setBusy("check");
    setError("");
    setMessage("");
    try {
      const result = await api.updateCheck();
      setInfo(result);
      if (!result.available) {
        setMessage(t("about.upToDate", { version: result.currentVersion }));
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(null);
    }
  };

  const install = async () => {
    setBusy("install");
    setError("");
    setMessage("");
    setProgress(null);
    try {
      const note = await api.updateInstall();
      setMessage(note || t("about.installedRelaunch"));
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="settings-update">
      <p className="hint">{t("about.updateHint")}</p>
      <div className="about-update-actions">
        <button type="button" onClick={() => void check()} disabled={busy !== null}>
          {busy === "check" ? t("about.checking") : t("about.checkUpdate")}
        </button>
        {info?.available && (
          <button
            type="button"
            className="primary"
            onClick={() => void install()}
            disabled={busy !== null}
          >
            {busy === "install" ? t("about.installing") : t("about.installUpdate")}
          </button>
        )}
      </div>
      {info?.available && (
        <p className="hint">
          {t("about.updateAvailable", { version: info.version })}
          {info.size ? ` (${formatBytes(info.size)})` : ""}
        </p>
      )}
      {info?.available && info.notes && <p className="hint">{info.notes}</p>}
      {busy === "install" && progress && (
        <p className="hint">
          {progress.total
            ? t("about.updateProgress", {
                received: formatBytes(progress.received),
                total: formatBytes(progress.total),
              })
            : t("about.updateProgressUnknown", {
                received: formatBytes(progress.received),
              })}
        </p>
      )}
      {message && <p className="hint sync-message">{message}</p>}
      {error && (
        <p className="error-text" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}