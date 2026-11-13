import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type CastFile, type RecordingMeta, type SessionLogMeta } from "../api";
import { CastPlayer, type PlayerState } from "../cast";
import { useAppStore } from "../store";
import { Modal } from "./Modal";
import { EmptyState, ErrorState, LoadingState, Segmented } from "./ui";

function formatTime(seconds: number) {
  const total = Math.max(0, Math.floor(seconds));
  const minutes = Math.floor(total / 60);
  return `${minutes}:${String(total % 60).padStart(2, "0")}`;
}

function formatSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

type Tab = "replay" | "log";

export function RecordingsDialog() {
  const { t, i18n } = useTranslation();
  const open = useAppStore((s) => s.recordingsOpen);
  const setOpen = useAppStore((s) => s.setRecordingsOpen);
  const [tab, setTab] = useState<Tab>("log");
  const [items, setItems] = useState<RecordingMeta[] | null>(null);
  const [logs, setLogs] = useState<SessionLogMeta[] | null>(null);
  const [error, setError] = useState("");
  const [active, setActive] = useState<string | null>(null);
  const [cast, setCast] = useState<CastFile | null>(null);
  const [logText, setLogText] = useState<string | null>(null);
  const [state, setState] = useState<PlayerState>({
    position: 0,
    duration: 0,
    playing: false,
  });
  const [speed, setSpeed] = useState(1);
  const playerRef = useRef<CastPlayer | null>(null);
  const hostRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    setItems(null);
    setLogs(null);
    setError("");
    setActive(null);
    setCast(null);
    setLogText(null);
    // Logs are the default capture, so the dialog opens on them.
    api
      .sessionLogList()
      .then(setLogs)
      .catch((err) => setError(String(err)));
    api
      .recordingList()
      .then(setItems)
      .catch((err) => setError(String(err)));
  }, [open]);

  useEffect(() => {
    const host = hostRef.current;
    if (!cast || !host) return;
    host.replaceChildren();
    const player = new CastPlayer(
      host,
      cast.header.width || 80,
      cast.header.height || 24,
      cast.events,
      cast.duration,
      setState,
    );
    playerRef.current = player;
    return () => {
      player.dispose();
      playerRef.current = null;
    };
  }, [cast]);

  if (!open) return null;

  const selectReplay = (name: string) => {
    setActive(name);
    setCast(null);
    setError("");
    api
      .recordingRead(name)
      .then(setCast)
      .catch((err) => setError(String(err)));
  };

  const selectLog = (name: string) => {
    setActive(name);
    setLogText(null);
    setError("");
    api
      .sessionLogRead(name)
      .then(setLogText)
      .catch((err) => setError(String(err)));
  };

  const dateLabel = (timestamp: number) =>
    timestamp ? new Date(timestamp * 1000).toLocaleString(i18n.language) : "";

  return (
    <Modal
      wide
      className="recordings"
      title={t("recordings.title")}
      onClose={() => setOpen(false)}
    >
      <Segmented
        className="recordings-tabs"
        value={tab}
        onChange={(value) => {
          setTab(value as Tab);
          setActive(null);
          setCast(null);
          setLogText(null);
          setError("");
        }}
        options={[
          { value: "log", label: t("recordings.tabLog") },
          { value: "replay", label: t("recordings.tabReplay") },
        ]}
      />
      <div className="recordings-layout">
        <div className="recordings-list">
          {tab === "log" ? (
            <>
              {logs === null && !error && (
                <LoadingState label={t("recordings.loading")} />
              )}
              {logs?.length === 0 && (
                <EmptyState
                  className="recordings-list-empty"
                  title={t("common.emptyTitle")}
                  description={t("recordings.logsEmpty")}
                />
              )}
              {logs?.map((meta) => (
                <button
                  key={meta.name}
                  type="button"
                  className={
                    active === meta.name
                      ? "recordings-item active"
                      : "recordings-item"
                  }
                  onClick={() => selectLog(meta.name)}
                >
                  <span className="recordings-item-title">
                    {meta.title || meta.name}
                  </span>
                  <span className="recordings-item-meta">
                    {dateLabel(meta.timestamp)} ·{" "}
                    {t("recordings.logLines", { count: meta.lines })} ·{" "}
                    {formatSize(meta.size)}
                  </span>
                </button>
              ))}
            </>
          ) : (
            <>
              {items === null && !error && (
                <LoadingState label={t("recordings.loading")} />
              )}
              {items?.length === 0 && (
                <EmptyState
                  className="recordings-list-empty"
                  title={t("common.emptyTitle")}
                  description={t("recordings.empty")}
                />
              )}
              {items?.map((meta) => (
                <button
                  key={meta.name}
                  type="button"
                  className={
                    active === meta.name
                      ? "recordings-item active"
                      : "recordings-item"
                  }
                  onClick={() => selectReplay(meta.name)}
                >
                  <span className="recordings-item-title">
                    {meta.title || meta.name}
                  </span>
                  <span className="recordings-item-meta">
                    {dateLabel(meta.timestamp)} · {formatTime(meta.duration)} ·{" "}
                    {formatSize(meta.size)}
                  </span>
                </button>
              ))}
            </>
          )}
        </div>
        <div className="recordings-stage">
          {tab === "log" ? (
            logText === null ? (
              <EmptyState
                className="recordings-stage-empty"
                title={t("recordings.selectLog")}
              />
            ) : (
              <pre className="recordings-log" aria-live="polite">
                {logText}
              </pre>
            )
          ) : (
            <>
              {!cast && (
                <EmptyState
                  className="recordings-stage-empty"
                  title={t("recordings.select")}
                />
              )}
              <div
                ref={hostRef}
                className="recordings-player"
                hidden={!cast}
              />
              {cast && (
                <div className="recordings-controls">
                  <button
                    type="button"
                    onClick={() => playerRef.current?.toggle()}
                  >
                    {state.playing ? t("recordings.pause") : t("recordings.play")}
                  </button>
                  <button
                    type="button"
                    onClick={() => playerRef.current?.restart()}
                  >
                    {t("recordings.restart")}
                  </button>
                  <input
                    type="range"
                    min={0}
                    max={Math.max(1, Math.round(cast.duration * 10))}
                    step={1}
                    value={Math.round(state.position * 10)}
                    aria-label={t("recordings.seek")}
                    onChange={(e) =>
                      playerRef.current?.seek(Number(e.target.value) / 10)
                    }
                  />
                  <span className="recordings-time">
                    {formatTime(state.position)} / {formatTime(cast.duration)}
                  </span>
                  <select
                    value={speed}
                    aria-label={t("recordings.speed")}
                    onChange={(e) => {
                      const next = Number(e.target.value);
                      setSpeed(next);
                      playerRef.current?.setSpeed(next);
                    }}
                  >
                    <option value={1}>1×</option>
                    <option value={2}>2×</option>
                    <option value={4}>4×</option>
                  </select>
                </div>
              )}
            </>
          )}
        </div>
      </div>
      {error && (
        <ErrorState
          className="recordings-error"
          title={t("common.errorTitle")}
          detail={error}
        />
      )}
    </Modal>
  );
}
