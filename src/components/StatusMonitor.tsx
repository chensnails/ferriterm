import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, isTauri, type RemoteSnapshot } from "../api";
import { formatBytes } from "../format";
import i18n from "../i18n";
import { useAppStore } from "../store";
import { PulseIcon } from "./icons";

const LOCAL_TARGET = "local";
/** The status bar is a glance, not a chart: sampling every 2s would keep an
 *  SSH exec probe running for no visible benefit. */
const POLL_MS = 5000;

interface Readout {
  host: string;
  cpu: number;
  cores: number;
  memUsed: number;
  memTotal: number;
  mount: string;
  diskUsed: number;
  diskTotal: number;
  load1: number;
  load5: number;
  load15: number;
  latency: number;
  at: number;
}

/** The remote probe is Linux `/proc`-based. These two messages are the only
 *  ones that mean "this host can never be sampled"; timeouts and closed
 *  connections are transient and must keep the entry visible. */
function isUnsupported(message: string): boolean {
  return (
    message.includes("does not report /proc/stat") ||
    message.includes("did not report /proc/meminfo")
  );
}

function percent(used: number, total: number): number {
  return total > 0 ? Math.min(100, (used / total) * 100) : 0;
}

function tone(value: number): string {
  if (value >= 95) return "danger";
  if (value >= 85) return "warn";
  return "";
}

/** Circular gauge: 13px ring plus the value, matching the host-tools readout. */
function Ring({
  label,
  value,
  display,
}: {
  label: string;
  value: number;
  display: string;
}) {
  const radius = 5.25;
  const circumference = 2 * Math.PI * radius;
  const offset = circumference * (1 - Math.min(100, Math.max(0, value)) / 100);
  return (
    <span className="statusbar-gauge">
      <span className="statusbar-gauge-label">{label}</span>
      <svg className="statusbar-ring" viewBox="0 0 14 14" aria-hidden>
        <circle
          className="statusbar-ring-track"
          cx="7"
          cy="7"
          r={radius}
          fill="none"
          strokeWidth="2"
        />
        <circle
          className={["statusbar-ring-fill", tone(value)]
            .filter(Boolean)
            .join(" ")}
          cx="7"
          cy="7"
          r={radius}
          fill="none"
          strokeWidth="2"
          strokeDasharray={circumference}
          strokeDashoffset={offset}
          transform="rotate(-90 7 7)"
        />
      </svg>
      <span className="statusbar-mono">{display}</span>
    </span>
  );
}

/**
 * Permanent status-bar monitor readout. It follows the active tab: no
 * monitorable session shows this machine, an SSH/SFTP session shows that
 * host. Hosts that answer the probe with "no /proc" are hidden entirely,
 * and the whole entry can be switched off from its context menu or settings.
 */
export function StatusMonitor() {
  const { t } = useTranslation();
  const enabled = useAppStore((s) => s.statusMonitor);
  const sessions = useAppStore((s) => s.sessions);
  const activeId = useAppStore((s) => s.activeId);
  const monitorOpen = useAppStore((s) => s.monitorOpen);
  const toggleMonitor = useAppStore((s) => s.toggleMonitor);
  const setMonitorTarget = useAppStore((s) => s.setMonitorTarget);
  const [readout, setReadout] = useState<Readout | null>(null);
  const [failed, setFailed] = useState(false);
  const [hovered, setHovered] = useState(false);
  const [unsupported, setUnsupported] = useState<Record<string, boolean>>({});

  const session = sessions.find(
    (item) =>
      item.id === activeId &&
      !item.closed &&
      (item.kind === "ssh" || item.kind === "sftp"),
  );
  const target = session ? session.id : LOCAL_TARGET;
  const blocked = unsupported[target] === true;

  useEffect(() => {
    if (!enabled || !isTauri || blocked) return;
    let cancelled = false;
    setReadout(null);
    setFailed(false);
    const tick = async () => {
      try {
        const sample =
          target === LOCAL_TARGET
            ? await api.monitorSample()
            : await api.monitorRemote(target);
        if (cancelled) return;
        const disk =
          sample.disks.find((item) =>
            target === LOCAL_TARGET ? true : item.mount === "/",
          ) ?? sample.disks[0];
        setReadout({
          host: sample.hostName,
          cpu: sample.cpuUsage,
          cores: sample.cpuCores,
          memUsed: sample.memUsed,
          memTotal: sample.memTotal,
          mount: disk?.mount ?? "",
          diskUsed: disk?.used ?? 0,
          diskTotal: disk?.total ?? 0,
          load1: sample.load1,
          load5: sample.load5,
          load15: sample.load15,
          // Local sampling has no network hop, so there is no latency to show.
          latency: target === LOCAL_TARGET ? 0 : (sample as RemoteSnapshot).latencyMs,
          at: Date.now(),
        });
        setFailed(false);
      } catch (err) {
        if (cancelled) return;
        if (isUnsupported(String(err))) {
          // Permanent for this session: hide the entry instead of retrying.
          setUnsupported((prev) => ({ ...prev, [target]: true }));
          return;
        }
        setFailed(true);
      }
    };
    void tick();
    const timer = window.setInterval(tick, POLL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [target, enabled, blocked]);

  if (!enabled || blocked) return null;

  const where = session ? session.title : t("status.monitorLocal");
  const cpu = readout ? Math.round(readout.cpu) : null;
  const memory = readout ? percent(readout.memUsed, readout.memTotal) : 0;
  const disk = readout ? percent(readout.diskUsed, readout.diskTotal) : 0;
  const updated = readout
    ? new Date(readout.at).toLocaleTimeString(i18n.language, { hour12: false })
    : "";

  const openPanel = () => {
    // Point the card at whatever the bar is showing, then make sure it is open.
    setMonitorTarget(target);
    if (!monitorOpen) toggleMonitor();
  };

  return (
    <button
      type="button"
      className="statusbar-item statusbar-monitor"
      onClick={openPanel}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      onFocus={() => setHovered(true)}
      onBlur={() => setHovered(false)}
      title={t("status.monitorOpen")}
      aria-label={`${t("status.monitor")}: ${where}`}
    >
      <PulseIcon size={13} />
      <span className="statusbar-monitor-host">{where}</span>
      {failed ? (
        <span className="statusbar-monitor-error">
          {t("status.monitorFailed")}
        </span>
      ) : (
        <>
          <Ring
            label="CPU"
            value={cpu ?? 0}
            display={cpu === null ? "—" : `${cpu}%`}
          />
          <Ring
            label={t("monitor.memory")}
            value={memory}
            display={readout ? `${Math.round(memory)}%` : "—"}
          />
          <Ring
            label={t("monitor.disk")}
            value={disk}
            display={readout ? `${Math.round(disk)}%` : "—"}
          />
          <span className="statusbar-gauge">
            <span className="statusbar-gauge-label">{t("monitor.load")}</span>
            <span className="statusbar-mono statusbar-load">
              {readout ? readout.load1.toFixed(2) : "—"}
            </span>
          </span>
          {readout && readout.latency > 0 && (
            <span
              className={[
                "statusbar-mono",
                "statusbar-latency",
                readout.latency >= 800
                  ? "danger"
                  : readout.latency >= 400
                    ? "warn"
                    : "",
              ]
                .filter(Boolean)
                .join(" ")}
            >
              {t("monitor.latencyShort", { ms: readout.latency })}
            </span>
          )}
        </>
      )}

      {hovered && readout && !failed && (
        <span className="statusbar-pop" role="tooltip">
          <span className="statusbar-pop-title">{where}</span>
          <span>
            CPU {cpu}% · {t("monitor.cores", { count: readout.cores })}
          </span>
          <span>
            {t("monitor.memory")} {formatBytes(readout.memUsed)} /{" "}
            {formatBytes(readout.memTotal)} ({Math.round(memory)}%)
          </span>
          <span>
            {t("monitor.disk")} {readout.mount} {formatBytes(readout.diskUsed)}{" "}
            / {formatBytes(readout.diskTotal)} ({Math.round(disk)}%)
          </span>
          <span>
            {t("monitor.load")} {readout.load1.toFixed(2)} /{" "}
            {readout.load5.toFixed(2)} / {readout.load15.toFixed(2)}
          </span>
          {readout.latency > 0 && (
            <span>
              {t("monitor.latency")} {t("monitor.latencyShort", { ms: readout.latency })}
            </span>
          )}
          <span>
            {t("monitor.updatedAt", { time: updated })}
          </span>
          <span className="statusbar-pop-hint">{t("monitor.openTools")}</span>
        </span>
      )}
    </button>
  );
}
