import { useMemo, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import {
  binaryToIpv4,
  describeSubnet,
  formatAddress,
  hexToIpv4,
  ipv4ToBinary,
  ipv4ToHex,
  netmaskForPrefix,
  hexMaskForPrefix,
  networkNodeBroadcast,
  parseAddress,
  prefixForHostCount,
  prefixForNetmask,
  splitCidr,
  wildcardForMask,
} from "../net/ip";
import { Notice } from "./ui";

/* ---------- shared building blocks ---------- */

function Panel({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="ipc-panel">
      <h3 className="ipc-title">{title}</h3>
      <div className="ipc-body">{children}</div>
    </section>
  );
}

function ResultCard({ rows }: { rows: { label: string; value: ReactNode }[] }) {
  if (rows.length === 0) return null;
  return (
    <section className="geo-card">
      <dl className="geo-rows">
        {rows.map((row) => (
          <div key={row.label} className="geo-row">
            <dt>{row.label}</dt>
            <dd className="mono">{row.value}</dd>
          </div>
        ))}
      </dl>
    </section>
  );
}

function CopyButton({ value }: { value: string }) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    if (!value) return;
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    } catch {
      setCopied(false);
    }
  };
  return (
    <button type="button" disabled={!value} onClick={() => void copy()}>
      {copied ? t("tools.common.copied") : t("tools.common.copy")}
    </button>
  );
}

function ComputeButtons({ onCompute, onReset }: { onCompute: () => void; onReset: () => void }) {
  const { t } = useTranslation();
  return (
    <div className="net-actions">
      <button type="button" className="primary" onClick={onCompute}>
        {t("tools.ipcalc.btn.calc")}
      </button>
      <button type="button" onClick={onReset}>
        {t("tools.ipcalc.btn.reset")}
      </button>
    </div>
  );
}

function BitsInput({
  value,
  onChange,
  ariaLabel,
}: {
  value: string;
  onChange: (value: string) => void;
  ariaLabel: string;
}) {
  return (
    <span className="ipc-slash">
      <span className="ipc-slash-mark">/</span>
      <input
        className="ipc-prefix net-target"
        inputMode="numeric"
        aria-label={ariaLabel}
        value={value}
        onChange={(event) =>
          onChange(event.target.value.replace(/[^0-9]/g, "").slice(0, 2))
        }
      />
    </span>
  );
}

function SelectField({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: string;
  options: string[];
  onChange: (value: string) => void;
}) {
  return (
    <label className="tool-field ipc-select-field">
      <span className="hint">{label}</span>
      <select
        className="ipc-select"
        value={value}
        onChange={(event) => onChange(event.target.value)}
      >
        {options.map((option) => (
          <option key={option} value={option}>
            {option}
          </option>
        ))}
      </select>
    </label>
  );
}

/** IPv4 class from the first octet: 8 (A), 16 (B), 24 (C), or 0 (none/classless). */
function classOf(value: bigint): number {
  const first = Number((value >> 24n) & 0xffn);
  if (first >= 1 && first <= 126) return 8;
  if (first >= 128 && first <= 191) return 16;
  if (first >= 192 && first <= 223) return 24;
  return 0;
}

const SUBNET_OPTIONS = [
  "2", "4", "8", "16", "32", "64", "128", "256",
  "512", "1024", "2048", "4096", "8192", "16384", "32768", "65536",
];

/* ---------- 1. network & IP address calculator ---------- */

function NetworkIpPanel() {
  const { t } = useTranslation();
  const [input, setInput] = useState("192.168.0.1/24");
  const report = useMemo(() => describeSubnet(input), [input]);

  const rows =
    typeof report === "string"
      ? []
      : [
          { label: t("tools.ipcalc.usable"), value: report.usableCount },
          { label: t("tools.ipcalc.netmask"), value: report.netmask },
          { label: t("tools.ipcalc.network"), value: report.network },
          { label: t("tools.ipcalc.firstUsable"), value: report.firstUsable },
          { label: t("tools.ipcalc.lastUsable"), value: report.lastUsable },
          { label: t("tools.ipcalc.broadcast"), value: report.last },
        ];
  const summary =
    typeof report === "string"
      ? ""
      : [
          `${t("tools.ipcalc.network")}: ${report.network}/${report.prefix}`,
          `${t("tools.ipcalc.netmask")}: ${report.netmask}`,
          `${t("tools.ipcalc.broadcast")}: ${report.last}`,
          `${t("tools.ipcalc.range")}: ${report.firstUsable} — ${report.lastUsable}`,
          `${t("tools.ipcalc.usable")}: ${report.usableCount}`,
        ].join("\n");

  return (
    <Panel title={t("tools.ipcalc.modules.network")}>
      <p className="hint">{t("tools.ipcalc.networkHint")}</p>
      <div className="net-form">
        <OctetPrefixInput value={input} onChange={setInput} />
      </div>
      <div className="net-actions">
        <button type="button" onClick={() => setInput("192.168.0.1/24")}>
          {t("tools.ipcalc.btn.reset")}
        </button>
        <CopyButton value={summary} />
      </div>
      {typeof report === "string" ? (
        <Notice tone="warn">
          {report === "invalid-prefix"
            ? t("tools.ipcalc.invalidPrefix")
            : t("tools.ipcalc.invalidAddress")}
        </Notice>
      ) : (
        <ResultCard rows={rows} />
      )}
      <p className="hint">{t("tools.ipcalc.note.cidr")}</p>
    </Panel>
  );
}

function OctetPrefixInput({
  value,
  onChange,
}: {
  value: string;
  onChange: (value: string) => void;
}) {
  const { address, prefix } = splitCidr(value);
  const octets = address.split(".");
  const pad = (index: number) => octets[index] ?? "";
  const setOctet = (index: number, raw: string) => {
    const clean = raw.replace(/[^0-9]/g, "").slice(0, 3);
    const next = [pad(0), pad(1), pad(2), pad(3)];
    next[index] = clean;
    onChange(`${next.join(".")}/${prefix ?? 24}`);
  };
  const setPrefix = (raw: string) => {
    const clean = raw.replace(/[^0-9]/g, "").slice(0, 2);
    onChange(`${address}/${clean}`);
  };
  return (
    <div className="ipc-octets">
      {[0, 1, 2, 3].map((index) => (
        <input
          key={index}
          className="ipc-octet net-target"
          inputMode="numeric"
          aria-label={`${index + 1}`}
          value={pad(index)}
          onChange={(event) => setOctet(index, event.target.value)}
        />
      ))}
      <span className="ipc-sep">/</span>
      <input
        className="ipc-prefix net-target"
        inputMode="numeric"
        aria-label="/"
        value={prefix ?? ""}
        onChange={(event) => setPrefix(event.target.value)}
      />
    </div>
  );
}

/* ---------- 2. subnet mask from mask bits ---------- */

function MaskFromBitsPanel() {
  const { t } = useTranslation();
  const [bits, setBits] = useState("24");
  const [result, setResult] = useState<{ dec: string; hex: string } | null>(null);

  const compute = () => {
    const prefix = Number.parseInt(bits, 10);
    const dec = netmaskForPrefix(prefix);
    const hex = hexMaskForPrefix(prefix);
    if (!dec || !hex) {
      setResult(null);
      return;
    }
    setResult({ dec, hex });
  };
  const reset = () => {
    setBits("24");
    setResult(null);
  };

  return (
    <Panel title={t("tools.ipcalc.modules.maskFromBits")}>
      <div className="net-form">
        <label className="tool-field">
          <span className="hint">{t("tools.ipcalc.f.maskBitsShort")}</span>
          <BitsInput
            value={bits}
            onChange={setBits}
            ariaLabel={t("tools.ipcalc.f.maskBitsShort")}
          />
        </label>
      </div>
      <ComputeButtons onCompute={compute} onReset={reset} />
      {result && (
        <ResultCard
          rows={[
            { label: t("tools.ipcalc.f.decimalMask"), value: result.dec },
            { label: t("tools.ipcalc.f.hexMask"), value: result.hex },
          ]}
        />
      )}
    </Panel>
  );
}

/* ---------- 3. IP subnet mask calculator (by count / by hosts) ---------- */

type NetClass = "none" | "a" | "b" | "c";

function SubnetByCountPanel() {
  const { t } = useTranslation();
  const [ip, setIp] = useState("192.168.0.1");
  const [netType, setNetType] = useState<NetClass>("none");
  const [mode, setMode] = useState<"subnets" | "hosts">("subnets");
  const [subnetCount, setSubnetCount] = useState("2");
  const [hostCount, setHostCount] = useState("2");
  const [error, setError] = useState("");
  const [showList, setShowList] = useState(false);
  const [result, setResult] = useState<{
    mask: string;
    subnets: number;
    hostsPer: number;
    list: string[];
    truncated: boolean;
  } | null>(null);

  const compute = () => {
    setError("");
    setShowList(false);
    const parsed = parseAddress(ip);
    if (!parsed || parsed.version !== 4) {
      setError(t("tools.ipcalc.invalidAddress"));
      setResult(null);
      return;
    }
    const classPrefix =
      netType === "a" ? 8 : netType === "b" ? 16 : netType === "c" ? 24 : classOf(parsed.value);
    if (classPrefix === 0) {
      setError(t("tools.ipcalc.invalidClass"));
      setResult(null);
      return;
    }
    const hostBits = 32 - classPrefix;
    let borrowed: number;
    let subnets: number;
    let hostsPer: number;
    if (mode === "subnets") {
      const wanted = Number.parseInt(subnetCount, 10);
      if (!wanted || wanted < 1) {
        setError(t("tools.ipcalc.invalidCount"));
        setResult(null);
        return;
      }
      borrowed = Math.ceil(Math.log2(wanted));
      subnets = 2 ** borrowed;
      hostsPer = 2 ** (hostBits - borrowed);
    } else {
      const wanted = Number.parseInt(hostCount, 10);
      if (!wanted || wanted < 1) {
        setError(t("tools.ipcalc.invalidCount"));
        setResult(null);
        return;
      }
      const needed = Math.ceil(Math.log2(wanted));
      borrowed = hostBits - needed;
      if (borrowed < 0) {
        setError(t("tools.ipcalc.tooManyHosts"));
        setResult(null);
        return;
      }
      subnets = 2 ** borrowed;
      hostsPer = 2 ** needed;
    }
    const newPrefix = classPrefix + borrowed;
    if (newPrefix > 32) {
      setError(t("tools.ipcalc.tooManySubnets"));
      setResult(null);
      return;
    }
    const dec = netmaskForPrefix(newPrefix);
    const hex = hexMaskForPrefix(newPrefix);
    if (!dec || !hex) {
      setResult(null);
      return;
    }
    const baseNetwork = (parsed.value >> BigInt(hostBits)) << BigInt(hostBits);
    const step = 2 ** (hostBits - borrowed);
    const list: string[] = [];
    for (let i = 0; i < subnets && i < 256; i += 1) {
      list.push(formatAddress(baseNetwork + BigInt(i) * BigInt(step), 4));
    }
    setResult({
      mask: `${dec} ${t("tools.ipcalc.twoFormatsSep")} ${hex}`,
      subnets,
      hostsPer,
      list,
      truncated: subnets > 256,
    });
  };

  const reset = () => {
    setIp("192.168.0.1");
    setNetType("none");
    setMode("subnets");
    setSubnetCount("2");
    setHostCount("2");
    setError("");
    setShowList(false);
    setResult(null);
  };

  return (
    <Panel title={t("tools.ipcalc.modules.subnetByCount")}>
      <div className="net-form">
        <label className="tool-field">
          <span className="hint">{t("tools.ipcalc.f.tcpIp")}</span>
          <input
            className="net-target"
            value={ip}
            spellCheck={false}
            aria-label={t("tools.ipcalc.f.tcpIp")}
            onChange={(event) => setIp(event.target.value)}
          />
        </label>
        <button type="button" onClick={() => setIp("192.168.0.1")}>
          {t("tools.ipcalc.btn.clear")}
        </button>
      </div>

      <fieldset className="ipc-radio">
        <legend className="hint">{t("tools.ipcalc.f.netType")}</legend>
        {(["none", "a", "b", "c"] as NetClass[]).map((option) => (
          <label key={option}>
            <input
              type="radio"
              name="ipc-nettype"
              checked={netType === option}
              onChange={() => setNetType(option)}
            />
            {t(`tools.ipcalc.netType.${option}`)}
          </label>
        ))}
      </fieldset>

      <SelectField
        label={t("tools.ipcalc.f.subnetIpCount")}
        value={subnetCount}
        options={SUBNET_OPTIONS}
        onChange={(value) => {
          setMode("subnets");
          setSubnetCount(value);
        }}
      />
      <SelectField
        label={t("tools.ipcalc.f.hostsPerSubnet")}
        value={hostCount}
        options={SUBNET_OPTIONS}
        onChange={(value) => {
          setMode("hosts");
          setHostCount(value);
        }}
      />

      <ComputeButtons onCompute={compute} onReset={reset} />

      {error && <Notice tone="warn">{error}</Notice>}
      {result && (
        <>
          <ResultCard
            rows={[
              { label: t("tools.ipcalc.f.netType"), value: t(`tools.ipcalc.netType.${netType}`) },
              { label: t("tools.ipcalc.netmask"), value: result.mask },
              {
                label: t("tools.ipcalc.subnets"),
                value: (
                  <span className="ipc-subnets">
                    {result.subnets}
                    <button
                      type="button"
                      className="ipc-netlist-btn"
                      onClick={() => setShowList((value) => !value)}
                    >
                      {t("tools.ipcalc.btn.netList")}
                    </button>
                  </span>
                ),
              },
              { label: t("tools.ipcalc.hostsPerNet"), value: result.hostsPer },
            ]}
          />
          {showList && (
            <div className="ipc-netlist">
              {result.list.map((entry) => (
                <code key={entry}>{entry}</code>
              ))}
              {result.truncated && <code>{t("tools.ipcalc.listTruncated")}</code>}
            </div>
          )}
        </>
      )}
    </Panel>
  );
}

/* ---------- 4. IP base converter ---------- */

function ConvRow({
  label,
  value,
  onChange,
  onCalc,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  onCalc: () => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="net-form">
      <label className="tool-field">
        <span className="hint">{label}</span>
        <input
          className="net-target"
          value={value}
          spellCheck={false}
          aria-label={label}
          onChange={(event) => onChange(event.target.value)}
        />
      </label>
      <div className="net-actions">
        <button type="button" onClick={onCalc}>
          {t("tools.ipcalc.btn.calc")}
        </button>
      </div>
    </div>
  );
}

function BaseConverterPanel() {
  const { t } = useTranslation();
  const [dec, setDec] = useState("192.168.0.1");
  const [bin, setBin] = useState("11000000.10101000.00000000.00000001");
  const [hex, setHex] = useState("C0.A8.00.01");
  const [dec2, setDec2] = useState("192.168.0.1");

  const fromDecimal = (source: string) => {
    const b = ipv4ToBinary(source);
    const h = ipv4ToHex(source);
    if (b && h) {
      setBin(b);
      setHex(h);
      setDec2(source);
    }
  };
  const fromBinary = (source: string) => {
    const d = binaryToIpv4(source);
    if (d) {
      setDec(d);
      setDec2(d);
      const h = ipv4ToHex(d);
      if (h) setHex(h);
    }
  };
  const fromHex = (source: string) => {
    const d = hexToIpv4(source);
    if (d) {
      setDec(d);
      setDec2(d);
      const b = ipv4ToBinary(d);
      if (b) setBin(b);
    }
  };

  return (
    <Panel title={t("tools.ipcalc.modules.base")}>
      <ConvRow
        label={t("tools.ipcalc.f.decimal")}
        value={dec}
        onChange={setDec}
        onCalc={() => fromDecimal(dec)}
      />
      <ConvRow
        label={t("tools.ipcalc.f.binary")}
        value={bin}
        onChange={setBin}
        onCalc={() => fromBinary(bin)}
      />
      <ConvRow
        label={t("tools.ipcalc.f.hex")}
        value={hex}
        onChange={setHex}
        onCalc={() => fromHex(hex)}
      />
      <ConvRow
        label={t("tools.ipcalc.f.decimal2")}
        value={dec2}
        onChange={setDec2}
        onCalc={() => fromDecimal(dec2)}
      />
    </Panel>
  );
}

/* ---------- 5. mask bits + multicast info ---------- */

function MaskMulticastPanel() {
  const { t } = useTranslation();
  const [bits, setBits] = useState("27");
  const [result, setResult] = useState<{ total: number; usable: number; dec: string } | null>(null);

  const compute = () => {
    const prefix = Number.parseInt(bits, 10);
    const dec = netmaskForPrefix(prefix);
    if (!dec || prefix < 0 || prefix > 32) {
      setResult(null);
      return;
    }
    const total = 2 ** (32 - prefix);
    const usable = total <= 2 ? total : total - 2;
    setResult({ total, usable, dec });
  };
  const reset = () => {
    setBits("27");
    setResult(null);
  };

  return (
    <Panel title={t("tools.ipcalc.modules.maskMulticast")}>
      <div className="net-form">
        <label className="tool-field">
          <span className="hint">{t("tools.ipcalc.f.maskBitsShort")}</span>
          <BitsInput
            value={bits}
            onChange={setBits}
            ariaLabel={t("tools.ipcalc.f.maskBitsShort")}
          />
        </label>
      </div>
      <ComputeButtons onCompute={compute} onReset={reset} />
      {result && (
        <ResultCard
          rows={[
            { label: t("tools.ipcalc.f.usableCount"), value: result.usable },
            { label: t("tools.ipcalc.f.totalCount"), value: result.total },
            { label: t("tools.ipcalc.f.decimalMask"), value: result.dec },
          ]}
        />
      )}
      <section className="geo-card">
        <dl className="geo-rows">
          <div className="geo-row">
            <dt>{t("tools.ipcalc.f.multicastRange")}</dt>
            <dd className="mono">224.0.0.0 – 239.255.255.255</dd>
          </div>
        </dl>
      </section>
      <p className="hint">{t("tools.ipcalc.note.multicast")}</p>
    </Panel>
  );
}

/* ---------- 6. subnet mask from host count ---------- */

function MaskFromHostsPanel() {
  const { t } = useTranslation();
  const [hosts, setHosts] = useState("5");
  const [result, setResult] = useState<{ prefix: number; dec: string; usable: number } | null>(null);

  const compute = () => {
    const wanted = Number.parseInt(hosts, 10);
    const prefix = prefixForHostCount(wanted);
    if (prefix === null) {
      setResult(null);
      return;
    }
    const dec = netmaskForPrefix(prefix);
    if (!dec) {
      setResult(null);
      return;
    }
    const total = 2 ** (32 - prefix);
    const usable = total <= 2 ? total : total - 2;
    setResult({ prefix, dec, usable });
  };
  const reset = () => {
    setHosts("5");
    setResult(null);
  };

  return (
    <Panel title={t("tools.ipcalc.modules.maskFromHosts")}>
      <div className="net-form">
        <label className="tool-field">
          <span className="hint">{t("tools.ipcalc.f.neededAddresses")}</span>
          <input
            className="net-target"
            inputMode="numeric"
            value={hosts}
            aria-label={t("tools.ipcalc.f.neededAddresses")}
            onChange={(event) =>
              setHosts(event.target.value.replace(/[^0-9]/g, "").slice(0, 9))
            }
          />
        </label>
      </div>
      <ComputeButtons onCompute={compute} onReset={reset} />
      {result && (
        <ResultCard
          rows={[
            { label: t("tools.ipcalc.f.maskBitsResult"), value: `/${result.prefix}` },
            { label: t("tools.ipcalc.f.decimalMask"), value: result.dec },
            { label: t("tools.ipcalc.f.usableCount"), value: result.usable },
          ]}
        />
      )}
    </Panel>
  );
}

/* ---------- 7. network / node calculator ---------- */

function NetNodePanel() {
  const { t } = useTranslation();
  const [mask, setMask] = useState("255.255.255.0");
  const [ip, setIp] = useState("192.168.0.1");
  const [error, setError] = useState("");
  const [result, setResult] = useState<{ network: string; node: string; broadcast: string } | null>(
    null,
  );

  const compute = () => {
    const found = networkNodeBroadcast(ip, mask);
    if (!found) {
      setError(t("tools.ipcalc.invalidPair"));
      setResult(null);
      return;
    }
    setError("");
    setResult(found);
  };
  const reset = () => {
    setMask("255.255.255.0");
    setIp("192.168.0.1");
    setError("");
    setResult(null);
  };

  return (
    <Panel title={t("tools.ipcalc.modules.netNode")}>
      <div className="net-form">
        <label className="tool-field">
          <span className="hint">{t("tools.ipcalc.f.decimalMask")}</span>
          <input
            className="net-target"
            value={mask}
            spellCheck={false}
            aria-label={t("tools.ipcalc.f.decimalMask")}
            onChange={(event) => setMask(event.target.value)}
          />
        </label>
      </div>
      <div className="net-form">
        <label className="tool-field">
          <span className="hint">{t("tools.ipcalc.f.tcpIp")}</span>
          <input
            className="net-target"
            value={ip}
            spellCheck={false}
            aria-label={t("tools.ipcalc.f.tcpIp")}
            onChange={(event) => setIp(event.target.value)}
          />
        </label>
      </div>
      <ComputeButtons onCompute={compute} onReset={reset} />
      {error && <Notice tone="warn">{error}</Notice>}
      {result && (
        <ResultCard
          rows={[
            { label: t("tools.ipcalc.network"), value: result.network },
            { label: t("tools.ipcalc.f.node"), value: result.node },
            { label: t("tools.ipcalc.broadcast"), value: result.broadcast },
          ]}
        />
      )}
    </Panel>
  );
}

/* ---------- 8. subnet mask converter (decimal <-> bits) ---------- */

function MaskConvertPanel() {
  const { t } = useTranslation();
  const [decIn, setDecIn] = useState("255.255.255.0");
  const [bitsIn, setBitsIn] = useState("24");
  const [decOut, setDecOut] = useState("");
  const [bitsOut, setBitsOut] = useState("");

  const toBits = () => {
    const prefix = prefixForNetmask(decIn);
    setBitsOut(prefix === null ? "" : String(prefix));
  };
  const toDec = () => {
    const prefix = Number.parseInt(bitsIn, 10);
    const dec = netmaskForPrefix(prefix);
    setDecOut(dec ?? "");
  };

  return (
    <Panel title={t("tools.ipcalc.modules.maskConvert")}>
      <div className="net-form">
        <label className="tool-field">
          <span className="hint">{t("tools.ipcalc.f.decimalMaskInput")}</span>
          <input
            className="net-target"
            value={decIn}
            spellCheck={false}
            aria-label={t("tools.ipcalc.f.decimalMaskInput")}
            onChange={(event) => setDecIn(event.target.value)}
          />
        </label>
        <div className="net-actions">
          <button type="button" onClick={toBits}>
            {t("tools.ipcalc.btn.calc")}
          </button>
        </div>
      </div>
      <p className="ipc-out mono">{bitsOut ? `/${bitsOut}` : ""}</p>

      <div className="net-form">
        <label className="tool-field">
          <span className="hint">{t("tools.ipcalc.f.maskBitsInput")}</span>
          <BitsInput
            value={bitsIn}
            onChange={setBitsIn}
            ariaLabel={t("tools.ipcalc.f.maskBitsInput")}
          />
        </label>
        <div className="net-actions">
          <button type="button" onClick={toDec}>
            {t("tools.ipcalc.btn.calc")}
          </button>
        </div>
      </div>
      <p className="ipc-out mono">{decOut}</p>
    </Panel>
  );
}

/* ---------- 9. wildcard (inverse) mask ---------- */

function WildcardPanel() {
  const { t } = useTranslation();
  const [mask, setMask] = useState("255.255.255.0");
  const [result, setResult] = useState("");

  const compute = () => {
    const wildcard = wildcardForMask(mask);
    setResult(wildcard ?? "");
  };
  const reset = () => {
    setMask("255.255.255.0");
    setResult("");
  };

  return (
    <Panel title={t("tools.ipcalc.modules.wildcard")}>
      <div className="net-form">
        <label className="tool-field">
          <span className="hint">{t("tools.ipcalc.f.maskInput")}</span>
          <input
            className="net-target"
            value={mask}
            spellCheck={false}
            aria-label={t("tools.ipcalc.f.maskInput")}
            onChange={(event) => setMask(event.target.value)}
          />
        </label>
      </div>
      <ComputeButtons onCompute={compute} onReset={reset} />
      {result && (
        <ResultCard rows={[{ label: t("tools.ipcalc.f.wildcardResult"), value: result }]} />
      )}
    </Panel>
  );
}

/* ---------- page ---------- */

export function IpCalcModules() {
  return (
    <div className="ipcalc-grid">
      <NetworkIpPanel />
      <MaskFromBitsPanel />
      <SubnetByCountPanel />
      <BaseConverterPanel />
      <MaskMulticastPanel />
      <MaskFromHostsPanel />
      <NetNodePanel />
      <MaskConvertPanel />
      <WildcardPanel />
    </div>
  );
}
