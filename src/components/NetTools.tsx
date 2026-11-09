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
} from "../api";
import { useAppStore } from "../store";
import { NetGeoIp } from "./NetGeoIp";
import { NetIpCalc } from "./NetIpCalc";
import { Notice, Segmented } from "./ui";

/** Content page of the network toolbox module. */
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
        {tool === "ping" && <NetProbe />}
        {tool === "geoip" && <NetGeoIp />}
        {tool === "ipcalc" && <NetIpCalc />}
      </div>
    </div>
  );
}

const MAX_LINES = 2000;

function NetProbe() {
  const { t } = useTranslation();
  const [kind, setKind] = useState<NetProbeKind>("ping");
  const [target, setTarget] = useState("");
  const [count, setCount] = useState(4);
  const [hops, setHops] = useState(20);
  const [lines, setLines] = useState<NetToolChunk[]>([]);
  const [running, setRunning] = useState(false);
  const [status, setStatus] = useState("");
  const [error, setError] = useState("");
  const jobRef = useRef<string | null>(null);
  const consoleRef = useRef<HTMLPreElement>(null);

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
    setStatus("");
    setError("");
    try {
      const id = await startNetProbe(kind, value, { count, hops });
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

  return (
    <div className="net-pane">
      <Segmented
        className="net-kind"
        value={kind}
        onChange={(value) => setKind(value as NetProbeKind)}
        options={[
          { value: "ping", label: t("tools.ping.kindPing") },
          { value: "traceroute", label: t("tools.ping.kindTrace") },
        ]}
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
        {kind === "ping" ? (
          <label className="net-number">
            <span>{t("tools.ping.count")}</span>
            <input
              type="number"
              min={1}
              max={20}
              value={count}
              onChange={(event) =>
                setCount(Number.parseInt(event.target.value, 10) || 4)
              }
            />
          </label>
        ) : (
          <label className="net-number">
            <span>{t("tools.ping.hops")}</span>
            <input
              type="number"
              min={1}
              max={64}
              value={hops}
              onChange={(event) =>
                setHops(Number.parseInt(event.target.value, 10) || 20)
              }
            />
          </label>
        )}
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
      {!isTauri && <Notice tone="warn">{t("tools.desktopOnly")}</Notice>}
      {error && <Notice tone="danger">{error}</Notice>}
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
