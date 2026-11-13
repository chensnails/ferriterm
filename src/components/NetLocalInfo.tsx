import { useCallback, useEffect, useRef, useState, type CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import {
  isTauri,
  localNetSnapshot,
  type AdapterKind,
  type LinkStatus,
  type LocalNetAdapter,
  type LocalNetSnapshot,
  type LocalWifi,
  type PeerConnection,
  type WifiState,
} from "../api";
import { RefreshIcon } from "./icons";
import {
  Chip,
  EmptyState,
  ErrorState,
  LoadingState,
  Notice,
  SectionHeader,
  Segmented,
} from "./ui";

/** Long connection tables stay readable; the exact count is shown instead. */
const MAX_ROWS = 200;

type ConnFilter = "active" | "established";

function statusTone(status: LinkStatus) {
  if (status === "up") return "info" as const;
  if (status === "down" || status === "disabled") return "warn" as const;
  return "neutral" as const;
}

function wifiTone(state: WifiState) {
  if (state === "connected") return "info" as const;
  if (state === "disconnected" || state === "disabled") return "warn" as const;
  return "neutral" as const;
}

/** A field with no value is a labelled absence, never a blank cell. */
function Field({
  label,
  values,
  missing,
}: {
  label: string;
  values: string[];
  missing: string;
}) {
  return (
    <div className="netinfo-row">
      <dt>{label}</dt>
      <dd>
        {values.length ? (
          values.map((value) => <code key={value}>{value}</code>)
        ) : (
          <span className="netinfo-missing">— {missing}</span>
        )}
      </dd>
    </div>
  );
}

function Notes({ notes, translate }: { notes: string[]; translate: (key: string) => string }) {
  if (!notes.length) return null;
  return (
    <div className="netinfo-notes">
      {notes.map((note) => (
        <Chip key={note} tone="warn">
          {translate(note)}
        </Chip>
      ))}
    </div>
  );
}

function AdapterCard({
  adapter,
  translate,
}: {
  adapter: LocalNetAdapter;
  translate: (key: string) => string;
}) {
  const { t } = useTranslation();
  const missing = t("tools.netinfo.missing");
  return (
    <article className="geo-card netinfo-card">
      <header>
        <h3>{adapter.name}</h3>
        <Chip tone={statusTone(adapter.status)}>
          {t(`tools.netinfo.status.${adapter.status}`)}
        </Chip>
        <Chip>{t(`tools.netinfo.kind.${adapter.kind as AdapterKind}`)}</Chip>
      </header>
      {adapter.description && <p className="hint">{adapter.description}</p>}
      <dl className="netinfo-rows">
        <Field label={t("tools.netinfo.fields.mac")} values={adapter.mac ? [adapter.mac] : []} missing={missing} />
        <Field label={t("tools.netinfo.fields.ipv4")} values={adapter.ipv4} missing={missing} />
        <Field label={t("tools.netinfo.fields.ipv6")} values={adapter.ipv6} missing={missing} />
        <Field label={t("tools.netinfo.fields.gateway")} values={adapter.gateways} missing={missing} />
        <Field
          label={
            adapter.dnsScope === "global"
              ? t("tools.netinfo.fields.dnsGlobal")
              : t("tools.netinfo.fields.dns")
          }
          values={adapter.dns}
          missing={missing}
        />
      </dl>
      <Notes notes={adapter.notes} translate={translate} />
    </article>
  );
}

function WifiCard({
  wifi,
  translate,
}: {
  wifi: LocalWifi;
  translate: (key: string) => string;
}) {
  const { t } = useTranslation();
  const missing = t("tools.netinfo.missing");
  const percent = wifi.signalPercent;
  return (
    <article className="geo-card netinfo-card">
      <header>
        <h3>{wifi.interface}</h3>
        <Chip tone={wifiTone(wifi.state)}>{t(`tools.netinfo.wifiState.${wifi.state}`)}</Chip>
        {wifi.band && <Chip>{wifi.band}</Chip>}
      </header>
      {percent !== null && (
        <div
          className="netinfo-signal"
          style={{ "--signal": `${percent}%` } as CSSProperties}
        >
          <div className="netinfo-signal-track">
            <div className="netinfo-signal-fill" />
          </div>
          <span className="netinfo-signal-value">{percent}%</span>
        </div>
      )}
      <dl className="netinfo-rows">
        <Field label={t("tools.netinfo.fields.ssid")} values={wifi.ssid ? [wifi.ssid] : []} missing={missing} />
        <Field
          label={t("tools.netinfo.fields.signal")}
          values={percent !== null ? [`${percent}%`] : []}
          missing={missing}
        />
        <Field
          label={t("tools.netinfo.fields.rssi")}
          values={wifi.rssiDbm !== null ? [`${wifi.rssiDbm} dBm`] : []}
          missing={missing}
        />
        <Field
          label={t("tools.netinfo.fields.rxRate")}
          values={wifi.rxRateMbps !== null ? [`${wifi.rxRateMbps} Mbps`] : []}
          missing={missing}
        />
        <Field
          label={t("tools.netinfo.fields.txRate")}
          values={wifi.txRateMbps !== null ? [`${wifi.txRateMbps} Mbps`] : []}
          missing={missing}
        />
        <Field label={t("tools.netinfo.fields.bssid")} values={wifi.bssid ? [wifi.bssid] : []} missing={missing} />
        <Field label={t("tools.netinfo.fields.mac")} values={wifi.mac ? [wifi.mac] : []} missing={missing} />
      </dl>
      <Notes notes={wifi.notes} translate={translate} />
    </article>
  );
}

function connectionRows(
  connections: PeerConnection[],
  filter: ConnFilter,
): PeerConnection[] {
  const active = connections.filter((item) => item.state !== "LISTEN");
  return filter === "established"
    ? active.filter((item) => item.state === "ESTABLISHED")
    : active;
}

/** Toolbox page: a structured snapshot of this machine's network state. */
export function NetLocalInfo() {
  const { t } = useTranslation();
  const [snapshot, setSnapshot] = useState<LocalNetSnapshot | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [filter, setFilter] = useState<ConnFilter>("active");
  const alive = useRef(true);

  const load = useCallback(async () => {
    if (!isTauri) return;
    setLoading(true);
    setError("");
    try {
      const next = await localNetSnapshot();
      if (alive.current) setSnapshot(next);
    } catch (err) {
      if (alive.current) setError(String(err));
    } finally {
      if (alive.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    alive.current = true;
    void load();
    return () => {
      alive.current = false;
    };
  }, [load]);

  const translateNote = useCallback(
    (key: string) => t(`tools.netinfo.notes.${key}`),
    [t],
  );
  const rows = snapshot ? connectionRows(snapshot.connections, filter) : [];
  const shown = rows.slice(0, MAX_ROWS);
  const takenAt = snapshot
    ? new Date(snapshot.generatedAt * 1000).toLocaleTimeString()
    : "";

  return (
    <div className="net-pane netinfo-pane">
      <div className="netinfo-bar">
        <span className="netinfo-time">
          {snapshot && t("tools.netinfo.generatedAt", { time: takenAt })}
        </span>
        {snapshot && (
          <Chip>{t(`tools.netinfo.platform.${snapshot.platform}`)}</Chip>
        )}
        <div className="net-actions">
          <button
            type="button"
            className="primary"
            disabled={loading || !isTauri}
            onClick={() => void load()}
          >
            <RefreshIcon size={13} />
            {loading ? t("tools.netinfo.refreshing") : t("tools.netinfo.refresh")}
          </button>
        </div>
      </div>

      {!isTauri && <Notice tone="warn">{t("tools.desktopOnly")}</Notice>}
      {snapshot?.sources
        .filter((source) => !source.ok)
        .map((source, index) => (
          <Notice key={`${source.source}-${index}`} tone="warn">
            {t("tools.netinfo.sourceFailed", {
              source: t(`tools.netinfo.source.${source.source}`),
              detail: source.detail,
            })}
          </Notice>
        ))}
      {error && <ErrorState title={t("common.errorTitle")} detail={error} />}
      {loading && !snapshot && <LoadingState label={t("tools.netinfo.loading")} />}

      {snapshot && (
        <>
          <section className="netinfo-section">
            <SectionHeader
              title={t("tools.netinfo.sections.adapters")}
              count={snapshot.adapters.length}
            />
            {snapshot.adapters.length ? (
              <div className="netinfo-cards">
                {snapshot.adapters.map((adapter, index) => (
                  <AdapterCard
                    key={`${adapter.name}-${index}`}
                    adapter={adapter}
                    translate={translateNote}
                  />
                ))}
              </div>
            ) : (
              <EmptyState title={t("tools.netinfo.adaptersEmpty")} />
            )}
          </section>

          <section className="netinfo-section">
            <SectionHeader
              title={t("tools.netinfo.sections.wifi")}
              count={snapshot.wifi.length}
            />
            {snapshot.wifi.length ? (
              <div className="netinfo-cards">
                {snapshot.wifi.map((wifi, index) => (
                  <WifiCard
                    key={`${wifi.interface}-${index}`}
                    wifi={wifi}
                    translate={translateNote}
                  />
                ))}
              </div>
            ) : (
              <EmptyState title={t("tools.netinfo.wifiEmpty")} />
            )}
          </section>

          <section className="netinfo-section">
            <SectionHeader
              title={t("tools.netinfo.sections.connections")}
              count={rows.length}
            />
            <Segmented
              className="netinfo-filter"
              value={filter}
              onChange={(value) => setFilter(value as ConnFilter)}
              options={[
                { value: "active", label: t("tools.netinfo.conn.filterActive") },
                {
                  value: "established",
                  label: t("tools.netinfo.conn.filterEstablished"),
                },
              ]}
            />
            {rows.length ? (
              <>
                <table className="file-table netinfo-table">
                  <thead>
                    <tr>
                      <th>{t("tools.netinfo.conn.protocol")}</th>
                      <th>{t("tools.netinfo.conn.local")}</th>
                      <th>{t("tools.netinfo.conn.remote")}</th>
                      <th>{t("tools.netinfo.conn.remotePort")}</th>
                      <th>{t("tools.netinfo.conn.state")}</th>
                      <th>{t("tools.netinfo.conn.process")}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {shown.map((item, index) => (
                      <tr
                        key={`${item.protocol}-${item.localAddr}-${item.localPort}-${item.remoteAddr}-${item.remotePort}-${index}`}
                      >
                        <td>{item.protocol}</td>
                        <td className="mono">
                          {item.localAddr}:{item.localPort}
                        </td>
                        <td className="mono">{item.remoteAddr}</td>
                        <td className="mono">
                          {item.remotePort || "—"}
                        </td>
                        <td>
                          <Chip tone={item.state === "ESTABLISHED" ? "info" : "neutral"}>
                            {item.state}
                          </Chip>
                        </td>
                        <td className="mono">
                          {item.process || (item.pid ? `#${item.pid}` : "—")}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
                {rows.length > shown.length && (
                  <Notice>
                    {t("tools.netinfo.conn.limit", {
                      count: shown.length,
                      total: rows.length,
                    })}
                  </Notice>
                )}
              </>
            ) : (
              <EmptyState title={t("tools.netinfo.conn.empty")} />
            )}
          </section>
        </>
      )}
    </div>
  );
}
