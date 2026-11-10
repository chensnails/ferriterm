import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import {
  api,
  isTauri,
  type DiskUsage,
  type GpuInfo,
  type MonitorSnapshot,
  type NetInterface,
  type ProcessInfo,
  type RemoteSnapshot,
} from "../api";
import i18n from "../i18n";
import { formatRate } from "../format";
import { useAppStore } from "../store";
import {
  CpuIcon,
  DiskIcon,
  InfoIcon,
  ListIcon,
  MemoryIcon,
  NetworkIcon,
} from "./icons";

const LOCAL_TARGET = "local";

/** Local and remote samples share the same rows, so one view keeps the
    render code single-path. Remote hosts have no GPU probe and no brand. */
interface HostView {
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
  disks: DiskUsage[];
  net: NetInterface[];
  processes: ProcessInfo[];
  latencyMs: number;
}

function fromLocal(snapshot: MonitorSnapshot): HostView {
  return { ...snapshot, latencyMs: 0 };
}

function fromRemote(snapshot: RemoteSnapshot): HostView {
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

/** "50 weeks, 1 day, 1 hour" — the largest units first, at most three. */
function formatUptime(secs: number): string {
  const units: [number, Intl.NumberFormatOptions["unit"]][] = [
    [Math.floor(secs / 604_800), "week"],
    [Math.floor((secs % 604_800) / 86_400), "day"],
    [Math.floor((secs % 86_400) / 3600), "hour"],
    [Math.floor((secs % 3600) / 60), "minute"],
  ];
  const parts = units
    .filter(([value]) => value > 0)
    .slice(0, 3)
    .map(([value, unit]) =>
      new Intl.NumberFormat(i18n.language, {
        style: "unit",
        unit,
        unitDisplay: "long",
      }).format(value),
    );
  return parts.length > 0 ? parts.join(" ") : "—";
}

function usage(used: number, total: number): number {
  return total > 0 ? Math.min(100, (used / total) * 100) : 0;
}

function clampPercent(value: number): number {
  return Math.max(0, Math.min(100, value));
}

function Meter({ percent, tone }: { percent: number; tone?: "warn" }) {
  return (
    <span className="host-meter" aria-hidden>
      <span
        className={["host-meter-fill", tone === "warn" ? "warn" : ""]
          .filter(Boolean)
          .join(" ")}
        style={{ width: `${clampPercent(percent)}%` }}
      />
    </span>
  );
}

function Section({
  icon,
  title,
  meta,
  children,
}: {
  icon: ReactNode;
  title: string;
  meta?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="host-card">
      <header className="host-card-head">
        <span className="host-card-icon" aria-hidden>
          {icon}
        </span>
        <h4>{title}</h4>
        {meta && <span className="host-card-meta">{meta}</span>}
      </header>
      <div className="host-card-body">{children}</div>
    </section>
  );
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
  const [view, setView] = useState<HostView | null>(null);
  const busy = useRef(false);

  // A tab that closed while selected must not keep polling its dead id.
  useEffect(() => {
    if (
      target !== LOCAL_TARGET &&
      !targets.some((session) => session.id === target)
    ) {
      setTarget(LOCAL_TARGET);
    }
  }, [target, targets, setTarget]);

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

  const kill = async (process: ProcessInfo) => {
    if (target === LOCAL_TARGET) return;
    const confirmed = await useAppStore.getState().askConfirm({
      title: t("monitor.killTitle"),
      body: t("monitor.killBody", {
        pid: process.pid,
        name: process.name,
      }),
      confirmLabel: t("monitor.kill"),
      danger: true,
    });
    if (!confirmed) return;
    try {
      await api.monitorKillProcess(target, process.pid);
      setStatus(t("monitor.killSent", { pid: process.pid }), false);
    } catch (err) {
      setStatus(String(err), true);
    }
  };

  const memory = view ? usage(view.memUsed, view.memTotal) : 0;
  const load = view ? clampPercent((view.load1 / Math.max(1, view.cpuCores)) * 100) : 0;
  const cpuLine = view
    ? [view.cpuBrand, view.cpuCores > 0 ? t("monitor.cores", { count: view.cpuCores }) : ""]
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
        <div className="host-tools">
          <Section
            icon={<InfoIcon size={13} />}
            title={t("monitor.overview")}
            meta={
              view.latencyMs > 0
                ? t("monitor.latencyShort", { ms: view.latencyMs })
                : undefined
            }
          >
            <dl className="host-kv">
              <div>
                <dt>{t("monitor.host")}</dt>
                <dd>
                  {view.hostName || "—"}
                  {view.osName ? ` · ${view.osName}` : ""}
                </dd>
              </div>
              <div>
                <dt>{t("monitor.uptime")}</dt>
                <dd>{formatUptime(view.uptimeSecs)}</dd>
              </div>
              <div>
                <dt>{t("monitor.cpu")}</dt>
                <dd>{cpuLine || "—"}</dd>
              </div>
              <div>
                <dt>{t("monitor.memory")}</dt>
                <dd>
                  {formatBytes(view.memUsed)} / {formatBytes(view.memTotal)}
                </dd>
              </div>
            </dl>
          </Section>

          <Section
            icon={<CpuIcon size={13} />}
            title={t("monitor.cpu")}
            meta={`${view.load1.toFixed(2)} / ${view.load5.toFixed(2)} / ${view.load15.toFixed(2)}`}
          >
            <div className="host-line">
              <span className="host-line-label">{t("monitor.load")}</span>
              <Meter percent={load} />
              <span className="host-line-value">{load.toFixed(0)}%</span>
            </div>
            <p className="hint host-hint">{t("monitor.loadHint")}</p>
          </Section>

          <Section
            icon={<MemoryIcon size={13} />}
            title={t("monitor.memory")}
            meta={`${memory.toFixed(0)}%`}
          >
            <div className="host-line">
              <span className="host-line-label">RAM</span>
              <Meter percent={memory} tone={memory >= 90 ? "warn" : undefined} />
              <span className="host-line-value">
                {formatBytes(view.memUsed)} / {formatBytes(view.memTotal)}
              </span>
            </div>
            {view.swapTotal > 0 && (
              <div className="host-line">
                <span className="host-line-label">
                  {t("monitor.swap")}
                </span>
                <Meter percent={usage(view.swapUsed, view.swapTotal)} />
                <span className="host-line-value">
                  {formatBytes(view.swapUsed)} / {formatBytes(view.swapTotal)}
                </span>
              </div>
            )}
          </Section>

          <Section icon={<DiskIcon size={13} />} title={t("monitor.disk")}>
            {view.disks.length === 0 ? (
              <p className="hint">{t("monitor.disksEmpty")}</p>
            ) : (
              view.disks.map((disk) => (
                <div className="host-line" key={disk.mount}>
                  <span className="host-line-label" title={disk.mount}>
                    {disk.mount}
                  </span>
                  <Meter percent={usage(disk.used, disk.total)} />
                  <span className="host-line-value">
                    {formatBytes(disk.used)} / {formatBytes(disk.total)}
                  </span>
                </div>
              ))
            )}
          </Section>

          <Section icon={<NetworkIcon size={13} />} title={t("monitor.network")}>
            {/* One row, one adapter: the card mirrors the status bar, so the
                extra interfaces only live in the raw probe output. */}
            {(() => {
              const primary =
                view.net.find((iface) => iface.primary) ?? view.net[0];
              if (!primary) {
                return <p className="hint">{t("monitor.netEmpty")}</p>;
              }
              return (
                <div className="host-net-row" key={primary.name}>
                  <span className="host-net-name" title={primary.name}>
                    {primary.name}
                  </span>
                  <span
                    className="host-net-rate"
                    title={t("monitor.netTotals", {
                      rx: formatBytes(primary.rxTotal),
                      tx: formatBytes(primary.txTotal),
                    })}
                  >
                    <span aria-hidden>↓</span>
                    {formatRate(primary.rxRate)}
                  </span>
                  <span
                    className="host-net-rate"
                    title={t("monitor.netTotals", {
                      rx: formatBytes(primary.rxTotal),
                      tx: formatBytes(primary.txTotal),
                    })}
                  >
                    <span aria-hidden>↑</span>
                    {formatRate(primary.txRate)}
                  </span>
                </div>
              );
            })()}
          </Section>

          <Section
            icon={<ListIcon size={13} />}
            title={t("monitor.processes")}
            meta={t("monitor.processCount", { count: view.processes.length })}
          >
            {view.processes.length === 0 ? (
              <p className="hint">{t("monitor.procEmpty")}</p>
            ) : (
              <div className="host-proc">
                <div className="host-proc-head">
                  <span>{t("monitor.pid")}</span>
                  <span>{t("monitor.command")}</span>
                  <span>{t("monitor.cpuPercent")}</span>
                  <span>{t("monitor.memPercent")}</span>
                  <span />
                </div>
                {view.processes.map((process) => (
                  <div className="host-proc-row" key={process.pid}>
                    <span className="host-proc-pid">{process.pid}</span>
                    <span className="host-proc-name" title={process.name}>
                      {process.name}
                    </span>
                    <span className="host-proc-num">
                      {process.cpu.toFixed(1)}
                    </span>
                    <span className="host-proc-num">
                      {process.mem.toFixed(1)}
                    </span>
                    {target === LOCAL_TARGET ? (
                      <span />
                    ) : (
                      <button
                        type="button"
                        className="link-btn danger host-proc-kill"
                        onClick={() => void kill(process)}
                      >
                        {t("monitor.kill")}
                      </button>
                    )}
                  </div>
                ))}
              </div>
            )}
            {target === LOCAL_TARGET && (
              <p className="hint host-hint">{t("monitor.killLocalHint")}</p>
            )}
          </Section>
        </div>
      )}
    </>
  );
}
