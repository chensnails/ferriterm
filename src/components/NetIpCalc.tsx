import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { describeSubnet } from "../net/ip";
import { Chip, Notice } from "./ui";

const SAMPLE = "192.168.1.10/24";

export function NetIpCalc() {
  const { t } = useTranslation();
  const [input, setInput] = useState(SAMPLE);

  const report = useMemo(() => describeSubnet(input), [input]);

  const rows =
    typeof report === "string"
      ? []
      : [
          { label: t("tools.ipcalc.network"), value: `${report.network}/${report.prefix}` },
          { label: t("tools.ipcalc.netmask"), value: report.netmask },
          { label: t("tools.ipcalc.wildcard"), value: report.wildcard },
          {
            label: report.version === 4 ? t("tools.ipcalc.broadcast") : t("tools.ipcalc.lastAddress"),
            value: report.last,
          },
          {
            label: t("tools.ipcalc.range"),
            value: `${report.firstUsable} — ${report.lastUsable}`,
          },
          { label: t("tools.ipcalc.hosts"), value: report.hostCount },
          { label: t("tools.ipcalc.usable"), value: report.usableCount },
          { label: t("tools.ipcalc.scope"), value: t(`tools.scope.${report.scope}`) },
        ];

  return (
    <div className="net-pane">
      <div className="net-form">
        <input
          className="net-target"
          value={input}
          spellCheck={false}
          placeholder={SAMPLE}
          aria-label={t("tools.ipcalc.input")}
          onChange={(event) => setInput(event.target.value)}
        />
        <div className="net-actions">
          <button type="button" onClick={() => setInput(SAMPLE)}>
            {t("tools.ipcalc.sample")}
          </button>
        </div>
      </div>

      {typeof report === "string" ? (
        <Notice tone="warn">
          {report === "invalid-prefix"
            ? t("tools.ipcalc.invalidPrefix")
            : t("tools.ipcalc.invalidAddress")}
        </Notice>
      ) : (
        <>
          <section className="geo-card">
            <header>
              <h3>{report.address}</h3>
              <Chip tone="info">{`IPv${report.version}`}</Chip>
              <Chip>{`/${report.prefix}`}</Chip>
            </header>
            <dl className="geo-rows">
              {rows.map((row) => (
                <div key={row.label} className="geo-row">
                  <dt>{row.label}</dt>
                  <dd className="mono">{row.value}</dd>
                </div>
              ))}
            </dl>
          </section>
          <section className="net-binary">
            <div>
              <span className="hint">{t("tools.ipcalc.addressBits")}</span>
              <pre>{report.binaryAddress}</pre>
            </div>
            <div>
              <span className="hint">{t("tools.ipcalc.networkBits")}</span>
              <pre>{report.binaryNetwork}</pre>
            </div>
            <div>
              <span className="hint">{t("tools.ipcalc.maskBits")}</span>
              <pre>{report.binaryMask}</pre>
            </div>
          </section>
        </>
      )}
      <p className="hint">{t("tools.ipcalc.hintShort")}</p>
    </div>
  );
}
