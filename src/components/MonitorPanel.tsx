import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, isTauri, type MonitorSnapshot } from "../api";
import { useAppStore } from "../store";

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
  const [snapshot, setSnapshot] = useState<MonitorSnapshot | null>(null);

  useEffect(() => {
    if (!isTauri) return;
    let cancelled = false;
    const tick = async () => {
      try {
        const next = await api.monitorSample();
        if (!cancelled) setSnapshot(next);
      } catch (err) {
        if (!cancelled) setStatus(String(err), true);
      }
    };
    void tick();
    const timer = window.setInterval(tick, 2000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [setStatus]);

  const memory = snapshot ? usage(snapshot.memUsed, snapshot.memTotal) : 0;
  const swap = snapshot ? usage(snapshot.swapUsed, snapshot.swapTotal) : 0;

  return (
    <>
      {!isTauri ? (
        <p className="hint">{t("monitor.desktopOnly")}</p>
      ) : !snapshot ? (
        <p className="hint">{t("monitor.sampling")}</p>
      ) : (
        <div className="monitor-body">
          <div className="monitor-row">
            <span className="monitor-label">{t("monitor.cpu")}</span>
            <div className="meter" aria-hidden>
              <span style={{ width: `${snapshot.cpuUsage}%` }} />
            </div>
            <span className="monitor-value">{snapshot.cpuUsage.toFixed(0)}%</span>
          </div>
          <div className="monitor-row">
            <span className="monitor-label">{t("monitor.memory")}</span>
            <div className="meter" aria-hidden>
              <span style={{ width: `${memory}%` }} />
            </div>
            <span className="monitor-value">
              {formatBytes(snapshot.memUsed)} / {formatBytes(snapshot.memTotal)}
            </span>
          </div>
          {snapshot.swapTotal > 0 && (
            <div className="monitor-row">
              <span className="monitor-label">{t("monitor.swap")}</span>
              <div className="meter" aria-hidden>
                <span style={{ width: `${swap}%` }} />
              </div>
              <span className="monitor-value">
                {formatBytes(snapshot.swapUsed)} / {formatBytes(snapshot.swapTotal)}
              </span>
            </div>
          )}
          <div className="monitor-meta">
            <span>
              {t("monitor.load")}: {snapshot.load1.toFixed(2)} /{" "}
              {snapshot.load5.toFixed(2)} / {snapshot.load15.toFixed(2)}
            </span>
            <span>
              {t("monitor.uptime")}: {formatUptime(snapshot.uptimeSecs)}
            </span>
          </div>
          <div className="monitor-system">
            <div className="monitor-system-row">
              <span className="monitor-label">{t("monitor.host")}</span>
              <span>{snapshot.hostName || "—"}</span>
            </div>
            <div className="monitor-system-row">
              <span className="monitor-label">{t("monitor.os")}</span>
              <span>
                {snapshot.osName || "—"}
                {snapshot.kernelVersion ? ` · ${snapshot.kernelVersion}` : ""}
              </span>
            </div>
            <div className="monitor-system-row">
              <span className="monitor-label">{t("monitor.cpuModel")}</span>
              <span>
                {snapshot.cpuBrand || "—"}
                {snapshot.cpuCores > 0
                  ? ` · ${t("monitor.cores", { count: snapshot.cpuCores })}`
                  : ""}
              </span>
            </div>
            <div className="monitor-system-row">
              <span className="monitor-label">{t("monitor.gpu")}</span>
              <span>
                {snapshot.gpus.length === 0 ? (
                  t("monitor.gpuNone")
                ) : (
                  snapshot.gpus.map((gpu, index) => (
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
          </div>
        </div>
      )}
    </>
  );
}
