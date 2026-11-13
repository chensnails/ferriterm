import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  cancelNetProbe,
  isTauri,
  onNetToolDone,
  onNetToolOutput,
  startNetProbe,
  type NetProbeKind,
  type NetToolChunk,
  type ProbeSummary,
} from "../api";
import { useAppStore } from "../store";
import { isNetworkTool } from "../toolGroups";
import { NetGeoIp } from "./NetGeoIp";
import { NetIpCalc } from "./NetIpCalc";
import { UtilityTool } from "./UtilityTools";
import { Notice, Segmented } from "./ui";

/** Content page of the toolbox module. */
export function NetTools() {
  const { t } = useTranslation();
  const tool = useAppStore((s) => s.netTool);

  return (
    <div className="net-tools">
      <header className="net-tools-head">
        <h2>{t(`tools.${tool}.title`)}</h2>
        <p className="hint">{t(`tools.${tool}.hint`)}</p>
      </header>
      <div className="net-tools-body">
        {isNetworkTool(tool) ? (
          <>
            {tool === "ping" && <NetProbe />}
            {tool === "geoip" && <NetGeoIp />}
            {tool === "ipcalc" && <NetIpCalc />}
          </>
        ) : (
          <UtilityTool id={tool} />
        )}
      </div>
    </div>
  );
}

const MAX_LINES = 2000;

const KINDS: { value: NetProbeKind; label: string; port: boolean }[] = [
  { value: "ping", label: "tools.ping.kindIcmp", port: false },
  { value: "tcping", label: "tools.ping.kindTcp", port: true },
  { value: "udp", label: "tools.ping.kindUdp", port: true },
  { value: "traceroute", label: "tools.ping.kindTrace", port: false },
];

function NumberField({
  label,
  value,
  min,
  max,
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  onChange: (next: number) => void;
}) {
  return (
    <label className="net-number">
      <span>{label}</span>
      <input
        type="number"
        min={min}
        max={max}
        value={value}
        onChange={(event) => {
          const next = Number.parseInt(event.target.value, 10);
          onChange(Number.isFinite(next) ? Math.min(max, Math.max(min, next)) : min);
        }}
      />
    </label>
  );
}

function NetProbe() {
  const { t } = useTranslation();
  const [kind, setKind] = useState<NetProbeKind>("ping");
  const [target, setTarget] = useState("");
  const [count, setCount] = useState(4);
  const [hops, setHops] = useState(20);
  const [port, setPort] = useState(443);
  const [interval, setIntervalMs] = useState(1000);
  const [timeout, setTimeoutMs] = useState(2000);
  const [size, setSize] = useState(56);
  const [advanced, setAdvanced] = useState(false);
  const [lines, setLines] = useState<NetToolChunk[]>([]);
  const [summary, setSummary] = useState<ProbeSummary | null>(null);
  const [running, setRunning] = useState(false);
  const [status, setStatus] = useState("");
  const [error, setError] = useState("");
  const jobRef = useRef<string | null>(null);
  const consoleRef = useRef<HTMLPreElement>(null);

  const wantsPort = kind === "tcping" || kind === "udp";
  const isTrace = kind === "traceroute";

  useEffect(() => {
    if (!isTauri) return;
    let disposed = false;
    const stops: (() => void)[] = [];
    void (async () => {
      const stopOutput = await onNetToolOutput((chunk) => {
        if (chunk.id !== jobRef.current) return;
        setLines((previous) => {
          const next = previous.concat(chunk);
          return next.length > MAX_LINES ? next.slice(-MAX_LINES) : next;
        });
      });
      const stopDone = await onNetToolDone((event) => {
        if (event.id !== jobRef.current) return;
        jobRef.current = null;
        setRunning(false);
        setSummary(event.summary);
        if (event.cancelled) setStatus(t("tools.ping.cancelled"));
        else if (event.error) setStatus(t("tools.ping.failed", { error: event.error }));
        else setStatus(t("tools.ping.finished", { code: event.code ?? 0 }));
      });
      if (disposed) {
        stopOutput();
        stopDone();
      } else {
        stops.push(stopOutput, stopDone);
      }
    })();
    return () => {
      disposed = true;
      stops.forEach((stop) => stop());
    };
  }, [t]);

  useEffect(() => {
    const node = consoleRef.current;
    if (node) node.scrollTop = node.scrollHeight;
  }, [lines]);

  const run = async () => {
    const value = target.trim();
    if (!value || running || !isTauri) return;
    setLines([]);
    setSummary(null);
    setStatus("");
    setError("");
    try {
      const id = await startNetProbe(kind, value, {
        count: isTrace ? undefined : count,
        hops,
        port: wantsPort ? port : undefined,
        interval,
        timeout,
        size,
      });
      jobRef.current = id;
      setRunning(true);
    } catch (err) {
      setError(String(err));
    }
  };

  const stop = async () => {
    const id = jobRef.current;
    if (!id) return;
    try {
      await cancelNetProbe(id);
    } catch (err) {
      setError(String(err));
    }
  };

  const output = lines
    .map((chunk) => (chunk.stream === "err" ? `! ${chunk.line}` : chunk.line))
    .join("\n");

  const loss =
    summary && summary.sent > 0
      ? Math.round(((summary.sent - summary.received) * 100) / summary.sent)
      : 0;
  const latency = (value: number | null) =>
    value === null ? "—" : `${value.toFixed(1)} ms`;

  return (
    <div className="net-pane">
      <Segmented
        className="net-kind"
        value={kind}
        onChange={(value) => setKind(value as NetProbeKind)}
        options={KINDS.map((item) => ({ value: item.value, label: t(item.label) }))}
      />
      <div className="net-form">
        <input
          className="net-target"
          value={target}
          spellCheck={false}
          placeholder={t("tools.ping.targetPlaceholder")}
          aria-label={t("tools.ping.target")}
          onChange={(event) => setTarget(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") void run();
          }}
        />
        {wantsPort && (
          <NumberField
            label={t("tools.ping.port")}
            value={port}
            min={1}
            max={65535}
            onChange={setPort}
          />
        )}
        {isTrace ? (
          <NumberField
            label={t("tools.ping.hops")}
            value={hops}
            min={1}
            max={64}
            onChange={setHops}
          />
        ) : (
          <NumberField
            label={t("tools.ping.count")}
            value={count}
            min={1}
            max={100}
            onChange={setCount}
          />
        )}
        <button
          type="button"
          className="link-btn"
          aria-expanded={advanced}
          onClick={() => setAdvanced((open) => !open)}
        >
          {advanced ? t("tools.ping.advancedHide") : t("tools.ping.advanced")}
        </button>
        <div className="net-actions">
          <button
            type="button"
            className="primary"
            disabled={running || !target.trim() || !isTauri}
            onClick={() => void run()}
          >
            {running ? t("tools.ping.running") : t("tools.ping.run")}
          </button>
          <button type="button" disabled={!running} onClick={() => void stop()}>
            {t("tools.ping.stop")}
          </button>
        </div>
      </div>
      {advanced && (
        <div className="net-form net-params">
          <NumberField
            label={t("tools.ping.interval")}
            value={interval}
            min={100}
            max={10000}
            onChange={setIntervalMs}
          />
          <NumberField
            label={t("tools.ping.timeout")}
            value={timeout}
            min={100}
            max={10000}
            onChange={setTimeoutMs}
          />
          <NumberField
            label={t("tools.ping.size")}
            value={size}
            min={1}
            max={65500}
            onChange={setSize}
          />
          <span className="hint">{t("tools.ping.units")}</span>
        </div>
      )}
      {kind === "udp" && <p className="hint">{t("tools.ping.udpNote")}</p>}
      {!isTauri && <Notice tone="warn">{t("tools.desktopOnly")}</Notice>}
      {error && <Notice tone="danger">{error}</Notice>}
      {summary && !isTrace && (
        <div className="net-stats">
          <div className="net-stat">
            <span>{t("tools.ping.statSent")}</span>
            <strong>{summary.sent}</strong>
          </div>
          <div className="net-stat">
            <span>{kind === "udp" ? t("tools.ping.statReplied") : t("tools.ping.statReceived")}</span>
            <strong>{kind === "udp" ? summary.replied : summary.received}</strong>
          </div>
          {/* UDP has no handshake, so a silent port is not a lost packet. */}
          {kind !== "udp" && (
            <div className="net-stat">
              <span>{t("tools.ping.statLoss")}</span>
              <strong>{loss}%</strong>
            </div>
          )}
          <div className="net-stat">
            <span>{t("tools.ping.statMin")}</span>
            <strong>{latency(summary.min)}</strong>
          </div>
          <div className="net-stat">
            <span>{t("tools.ping.statAvg")}</span>
            <strong>{latency(summary.avg)}</strong>
          </div>
          <div className="net-stat">
            <span>{t("tools.ping.statMax")}</span>
            <strong>{latency(summary.max)}</strong>
          </div>
        </div>
      )}
      <pre
        ref={consoleRef}
        className={output ? "net-console" : "net-console empty"}
        aria-live="polite"
      >
        {output || t("tools.ping.empty")}
      </pre>
      {status && <p className="hint">{status}</p>}
    </div>
  );
}
