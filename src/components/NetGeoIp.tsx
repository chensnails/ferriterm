import { useState } from "react";
import { useTranslation } from "react-i18next";
import { geoipLookup, isTauri, type GeoInfo } from "../api";
import { Chip, EmptyState, ErrorState, LoadingState, Notice } from "./ui";

export function NetGeoIp() {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");
  const [info, setInfo] = useState<GeoInfo | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const lookup = async (value: string) => {
    if (!isTauri) {
      setError(t("tools.desktopOnly"));
      return;
    }
    setLoading(true);
    setError("");
    try {
      setInfo(await geoipLookup(value.trim()));
    } catch (err) {
      setInfo(null);
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  const rows: { label: string; value: string }[] = info
    ? [
        { label: t("tools.geoip.ip"), value: info.ip },
        {
          label: t("tools.geoip.location"),
          value: [info.country, info.region, info.city]
            .filter(Boolean)
            .join(" · "),
        },
        {
          label: t("tools.geoip.coordinates"),
          value: `${info.latitude}, ${info.longitude}`,
        },
        { label: t("tools.geoip.isp"), value: info.isp },
        { label: t("tools.geoip.org"), value: info.org },
        { label: t("tools.geoip.asn"), value: info.asn },
        { label: t("tools.geoip.timezone"), value: info.timezone },
        { label: t("tools.geoip.source"), value: info.source },
      ]
    : [];

  return (
    <div className="net-pane">
      <div className="net-form">
        <input
          className="net-target"
          value={query}
          spellCheck={false}
          placeholder={t("tools.geoip.placeholder")}
          aria-label={t("tools.geoip.ip")}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") void lookup(query);
          }}
        />
        <div className="net-actions">
          <button
            type="button"
            className="primary"
            disabled={loading || !isTauri}
            onClick={() => void lookup(query)}
          >
            {loading ? t("tools.geoip.loading") : t("tools.geoip.lookup")}
          </button>
          <button
            type="button"
            disabled={loading || !isTauri}
            onClick={() => {
              setQuery("");
              void lookup("");
            }}
          >
            {t("tools.geoip.mine")}
          </button>
        </div>
      </div>
      {!isTauri && <Notice tone="warn">{t("tools.desktopOnly")}</Notice>}
      {error && (
        <ErrorState
          title={t("common.errorTitle")}
          detail={error}
        />
      )}
      {loading && !info && (
        <LoadingState label={t("tools.geoip.loading")} />
      )}
      {!loading && !info && !error && (
        <EmptyState title={t("tools.geoip.empty")} />
      )}
      {info && (
        <section className="geo-card">
          <header>
            <h3>{info.ip}</h3>
            {info.version && <Chip tone="info">{info.version}</Chip>}
            {info.countryCode && <Chip>{info.countryCode}</Chip>}
          </header>
          <dl className="geo-rows">
            {rows.map((row) => (
              <div key={row.label} className="geo-row">
                <dt>{row.label}</dt>
                <dd>{row.value || "—"}</dd>
              </div>
            ))}
          </dl>
        </section>
      )}
    </div>
  );
}
