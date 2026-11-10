import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  api,
  isTauri,
  type GpuInfo,
  type MonitorSnapshot,
  type RemoteSnapshot,
} from "../api";
import { useAppStore } from "../store";

const LOCAL_TARGET = "local";

/** Local and remote samples share the same rows, so one view keeps the
    render code single-path. Remote hosts have no GPU probe. */
interface MonitorView {
  cpuUsage: number;
  cpuBrand: string;
  cpuCores: number;
  memUsed: number;
  memTotal: number;
  swapUsed: number;
  swapTotal: number;
  load1: number;
  load5: number;
  load15: number;
  uptimeSecs: number;
  hostName: string;
  osName: string;
  kernelVersion: string;
  gpus: GpuInfo[];
  diskUsed?: number;
  diskTotal?: number;
}

function fromLocal(snapshot: MonitorSnapshot): MonitorView {
  return snapshot;
}

function fromRemote(snapshot: RemoteSnapshot): MonitorView {
  return { ...snapshot, cpuBrand: "", gpus: [] };
}

function formatBytes(bytes: number): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`;
}

function formatUptime(secs: number): string {
  const days = Math.floor(secs / 86_400);
  const hours = Math.floor((secs % 86_400) / 3600);
  const minutes = Math.floor((secs % 3600) / 60);
  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${minutes}m`;
  return `${minutes}m`;
}

function usage(used: number, total: number): number {
  return total > 0 ? Math.min(100, (used / total) * 100) : 0;
}

export function MonitorPanel() {
  const { t } = useTranslation();
  const setStatus = useAppStore((s) => s.setStatus);
  const sessions = useAppStore((s) => s.sessions);
  const targets = useMemo(
    () =>
      sessions.filter(
        (session) =>
          (session.kind === "ssh" || session.kind === "sftp") &&
          !session.closed,
      ),
    [sessions],
  );
  // Target lives in the store: the status-bar readout can point the card at
  // the active session, and both surfaces always agree on the host.
  const target = useAppStore((s) => s.monitorTarget);
  const setTarget = useAppStore((s) => s.setMonitorTarget);
  const [view, setView] = useState<MonitorView | null>(null);
  const busy = useRef(false);

  // A tab that closed while selected must not keep polling its dead id.
  useEffect(() => {
    if (
      target !== LOCAL_TARGET &&
      !targets.some((session) => session.id === target)
    ) {
      setTarget(LOCAL_TARGET);
    }
  }, [target, targets]);

  useEffect(() => {
    if (!isTauri) return;
    let cancelled = false;
    setView(null);
    const tick = async () => {
      if (busy.current) return;
      busy.current = true;
      try {
        const next =
          target === LOCAL_TARGET
            ? fromLocal(await api.monitorSample())
            : fromRemote(await api.monitorRemote(target));
        if (!cancelled) setView(next);
      } catch (err) {
        if (!cancelled) setStatus(String(err), true);
      } finally {
        busy.current = false;
      }
    };
    void tick();
    const timer = window.setInterval(tick, 2000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [target, setStatus]);

  const memory = view ? usage(view.memUsed, view.memTotal) : 0;
  const swap = view ? usage(view.swapUsed, view.swapTotal) : 0;
  const disk = view ? usage(view.diskUsed ?? 0, view.diskTotal ?? 0) : 0;
  const cpuLine = view
    ? [
        view.cpuBrand,
        view.cpuCores > 0
          ? t("monitor.cores", { count: view.cpuCores })
          : "",
      ]
        .filter(Boolean)
        .join(" · ")
    : "";

  return (
    <>
      <label className="monitor-target">
        <span>{t("monitor.target")}</span>
        <select
          value={target}
          onChange={(event) => setTarget(event.target.value)}
        >
          <option value={LOCAL_TARGET}>{t("monitor.localTarget")}</option>
          {targets.map((session) => (
            <option key={session.id} value={session.id}>
              {session.title}
            </option>
          ))}
        </select>
      </label>
      {!isTauri ? (
        <p className="hint">{t("monitor.desktopOnly")}</p>
      ) : !view ? (
        <p className="hint">{t("monitor.sampling")}</p>
      ) : (
        <div className="monitor-body">
          <div className="monitor-row">
            <span className="monitor-label">{t("monitor.cpu")}</span>
            <div className="meter" aria-hidden>
              <span style={{ width: `${view.cpuUsage}%` }} />
            </div>
            <span className="monitor-value">
              {view.cpuUsage.toFixed(0)}%
            </span>
          </div>
          <div className="monitor-row">
            <span className="monitor-label">{t("monitor.memory")}</span>
            <div className="meter" aria-hidden>
              <span style={{ width: `${memory}%` }} />
            </div>
            <span className="monitor-value">
              {formatBytes(view.memUsed)} / {formatBytes(view.memTotal)}
            </span>
          </div>
          {view.swapTotal > 0 && (
            <div className="monitor-row">
              <span className="monitor-label">{t("monitor.swap")}</span>
              <div className="meter" aria-hidden>
                <span style={{ width: `${swap}%` }} />
              </div>
              <span className="monitor-value">
                {formatBytes(view.swapUsed)} / {formatBytes(view.swapTotal)}
              </span>
            </div>
          )}
          {view.diskTotal ? (
            <div className="monitor-row">
              <span className="monitor-label">{t("monitor.disk")}</span>
              <div className="meter" aria-hidden>
                <span style={{ width: `${disk}%` }} />
              </div>
              <span className="monitor-value">
                {formatBytes(view.diskUsed ?? 0)} /{" "}
                {formatBytes(view.diskTotal)}
              </span>
            </div>
          ) : null}
          <div className="monitor-meta">
            <span>
              {t("monitor.load")}: {view.load1.toFixed(2)} /{" "}
              {view.load5.toFixed(2)} / {view.load15.toFixed(2)}
            </span>
            <span>
              {t("monitor.uptime")}: {formatUptime(view.uptimeSecs)}
            </span>
          </div>
          <div className="monitor-system">
            <div className="monitor-system-row">
              <span className="monitor-label">{t("monitor.host")}</span>
              <span>{view.hostName || "—"}</span>
            </div>
            <div className="monitor-system-row">
              <span className="monitor-label">{t("monitor.os")}</span>
              <span>
                {view.osName || "—"}
                {view.kernelVersion ? ` · ${view.kernelVersion}` : ""}
              </span>
            </div>
            <div className="monitor-system-row">
              <span className="monitor-label">{t("monitor.cpuModel")}</span>
              <span>{cpuLine || "—"}</span>
            </div>
            {target === LOCAL_TARGET && (
              <div className="monitor-system-row">
                <span className="monitor-label">{t("monitor.gpu")}</span>
                <span>
                  {view.gpus.length === 0 ? (
                    t("monitor.gpuNone")
                  ) : (
                    view.gpus.map((gpu, index) => (
                      <span key={`${gpu.name}-${index}`} className="monitor-gpu">
                        {gpu.name}
                        {gpu.utilization != null
                          ? ` · ${gpu.utilization.toFixed(0)}%`
                          : ""}
                        {gpu.memoryTotal
                          ? ` · ${formatBytes(gpu.memoryUsed ?? 0)} / ${formatBytes(gpu.memoryTotal)}`
                          : ""}
                      </span>
                    ))
                  )}
                </span>
              </div>
            )}
          </div>
        </div>
      )}
    </>
  );
}
