import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, isTauri } from "../api";
import { useAppStore } from "../store";
import { PulseIcon } from "./icons";

const LOCAL_TARGET = "local";
/** The status bar is a glance, not a chart: sampling every 2s would keep an
 *  SSH exec probe running for no visible benefit. */
const POLL_MS = 5000;

interface Readout {
  host: string;
  cpu: number;
  memory: number;
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

function Gauge({ label, value }: { label: string; value: number | null }) {
  return (
    <span className="statusbar-gauge">
      <span className="statusbar-gauge-label">{label}</span>
      <span className="statusbar-gauge-track" aria-hidden>
        <span
          className={[
            "statusbar-gauge-fill",
            value !== null && value >= 85 ? "high" : "",
          ]
            .filter(Boolean)
            .join(" ")}
          style={{ width: `${value ?? 0}%` }}
        />
      </span>
      <span className="statusbar-mono">
        {value === null ? "—" : `${value}%`}
      </span>
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
        setReadout({
          host: sample.hostName,
          cpu: sample.cpuUsage,
          memory:
            sample.memTotal > 0
              ? (sample.memUsed / sample.memTotal) * 100
              : 0,
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
  const memory = readout ? Math.round(readout.memory) : null;
  const detail = readout
    ? `${where} · ${t("monitor.cpu")} ${cpu}% · ${t("monitor.memory")} ${memory}%`
    : where;

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
      title={`${detail} — ${t("status.monitorOpen")}`}
      aria-label={`${t("status.monitor")}: ${detail}`}
    >
      <PulseIcon size={13} />
      <span className="statusbar-monitor-host">{where}</span>
      {failed ? (
        <span className="statusbar-monitor-error">
          {t("status.monitorFailed")}
        </span>
      ) : (
        <>
          <Gauge label="CPU" value={cpu} />
          <Gauge label="MEM" value={memory} />
        </>
      )}
    </button>
  );
}
