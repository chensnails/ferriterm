import { useEffect, useMemo, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { UtilityToolId } from "../toolGroups";
import { Notice, Segmented } from "./ui";

/**
 * Local, self-contained tools. Unlike the network probes these run entirely in
 * the renderer, so they work in the browser preview as well as in the app.
 */
export function UtilityTool({ id }: { id: UtilityToolId }) {
  switch (id) {
    case "json":
      return <JsonTool />;
    case "color":
      return <ColorTool />;
    case "regex":
      return <RegexTool />;
    case "base64":
      return <Base64Tool />;
    case "url":
      return <UrlTool />;
    case "timestamp":
      return <TimestampTool />;
    case "counter":
      return <CounterTool />;
    case "hash":
      return <HashTool />;
    case "uuid":
      return <UuidTool />;
    default:
      return null;
  }
}

/* ---------- shared building blocks ---------- */

function LabeledInput({
  label,
  value,
  onChange,
  placeholder,
  type = "text",
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  type?: "text" | "number";
}) {
  return (
    <label className="tool-field">
      <span className="hint">{label}</span>
      <input
        className="net-target"
        type={type}
        value={value}
        spellCheck={false}
        placeholder={placeholder}
        aria-label={label}
        onChange={(event) => onChange(event.target.value)}
      />
    </label>
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

function ResultRows({ rows }: { rows: { label: string; value: ReactNode }[] }) {
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

/* ---------- frontend ---------- */

const JSON_SAMPLE = '{"name":"Ferriterm","version":1,"tags":["ssh","terminal"]}';

function JsonTool() {
  const { t } = useTranslation();
  const [input, setInput] = useState(JSON_SAMPLE);
  const [mode, setMode] = useState<"format" | "minify">("format");
  const [indent, setIndent] = useState(2);

  const result = useMemo(() => {
    if (!input.trim()) return { text: "", error: "" };
    try {
      const value: unknown = JSON.parse(input);
      return {
        text:
          mode === "format"
            ? JSON.stringify(value, null, indent)
            : JSON.stringify(value),
        error: "",
      };
    } catch {
      return { text: "", error: t("tools.json.invalid") };
    }
  }, [input, mode, indent, t]);

  return (
    <div className="net-pane">
      <div className="net-form">
        <Segmented
          label={t("tools.common.mode")}
          value={mode}
          onChange={(value) => setMode(value as "format" | "minify")}
          options={[
            { value: "format", label: t("tools.json.format") },
            { value: "minify", label: t("tools.json.minify") },
          ]}
        />
        {mode === "format" && (
          <label className="net-number">
            <span>{t("tools.json.indent")}</span>
            <input
              type="number"
              min={1}
              max={8}
              value={indent}
              onChange={(event) =>
                setIndent(
                  Math.min(8, Math.max(1, Number.parseInt(event.target.value, 10) || 2)),
                )
              }
            />
          </label>
        )}
        <div className="net-actions">
          <button type="button" onClick={() => setInput(JSON_SAMPLE)}>
            {t("tools.common.sample")}
          </button>
        </div>
      </div>
      <textarea
        className="tool-area"
        value={input}
        spellCheck={false}
        aria-label={t("tools.common.input")}
        onChange={(event) => setInput(event.target.value)}
      />
      {result.error && <Notice tone="warn">{result.error}</Notice>}
      <pre className={result.text ? "net-console" : "net-console empty"}>
        {result.text || t("tools.common.empty")}
      </pre>
      <div className="net-actions">
        <CopyButton value={result.text} />
      </div>
    </div>
  );
}

interface Rgb {
  r: number;
  g: number;
  b: number;
}

const COLOR_SAMPLE = "rgb(79, 140, 255)";

function parseColor(raw: string): Rgb | null {
  const value = raw.trim().toLowerCase();
  const short = /^#?([0-9a-f]{3})$/.exec(value);
  if (short) {
    const [r, g, b] = short[1].split("");
    return {
      r: Number.parseInt(r + r, 16),
      g: Number.parseInt(g + g, 16),
      b: Number.parseInt(b + b, 16),
    };
  }
  const long = /^#?([0-9a-f]{6})$/.exec(value);
  if (long) {
    return {
      r: Number.parseInt(long[1].slice(0, 2), 16),
      g: Number.parseInt(long[1].slice(2, 4), 16),
      b: Number.parseInt(long[1].slice(4, 6), 16),
    };
  }
  const rgb = /^rgba?\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})/.exec(value);
  if (rgb) {
    const [r, g, b] = rgb.slice(1, 4).map((part) =>
      Math.min(255, Math.max(0, Number.parseInt(part, 10))),
    );
    return { r, g, b };
  }
  return null;
}

function toHsl({ r, g, b }: Rgb) {
  const rn = r / 255;
  const gn = g / 255;
  const bn = b / 255;
  const max = Math.max(rn, gn, bn);
  const min = Math.min(rn, gn, bn);
  const lightness = (max + min) / 2;
  const delta = max - min;
  let hue = 0;
  let saturation = 0;
  if (delta !== 0) {
    saturation = delta / (1 - Math.abs(2 * lightness - 1));
    if (max === rn) hue = ((gn - bn) / delta) % 6;
    else if (max === gn) hue = (bn - rn) / delta + 2;
    else hue = (rn - gn) / delta + 4;
    hue *= 60;
    if (hue < 0) hue += 360;
  }
  return {
    h: Math.round(hue),
    s: Math.round(saturation * 100),
    l: Math.round(lightness * 100),
  };
}

function ColorTool() {
  const { t } = useTranslation();
  const [input, setInput] = useState(COLOR_SAMPLE);
  const color = useMemo(() => parseColor(input), [input]);
  const hex = color
    ? "#" +
      [color.r, color.g, color.b]
        .map((part) => part.toString(16).padStart(2, "0"))
        .join("")
    : "";
  const hsl = color ? toHsl(color) : null;

  return (
    <div className="net-pane">
      <div className="net-form">
        <LabeledInput label={t("tools.color.value")} value={input} onChange={setInput} />
        <div className="net-actions">
          <button type="button" onClick={() => setInput(COLOR_SAMPLE)}>
            {t("tools.common.sample")}
          </button>
        </div>
      </div>
      {!color ? (
        <Notice tone="warn">{t("tools.color.invalid")}</Notice>
      ) : (
        <>
          <div className="tool-swatch-row">
            <span className="tool-swatch" style={{ backgroundColor: hex }} />
            <span className="hint">{t("tools.color.preview")}</span>
          </div>
          <ResultRows
            rows={[
              { label: t("tools.color.hex"), value: hex.toUpperCase() },
              { label: t("tools.color.rgb"), value: `rgb(${color.r}, ${color.g}, ${color.b})` },
              { label: t("tools.color.hsl"), value: `hsl(${hsl?.h}, ${hsl?.s}%, ${hsl?.l}%)` },
            ]}
          />
          <div className="net-actions">
            <CopyButton value={hex} />
          </div>
        </>
      )}
    </div>
  );
}

const REGEX_SAMPLE = "\\b\\w+@\\w+\\.\\w+\\b";
const REGEX_TEXT_SAMPLE = "Contact alice@example.com or bob@test.org for details.";

function RegexTool() {
  const { t } = useTranslation();
  const [pattern, setPattern] = useState(REGEX_SAMPLE);
  const [flags, setFlags] = useState("gi");
  const [text, setText] = useState(REGEX_TEXT_SAMPLE);

  const result = useMemo(() => {
    if (!pattern) return { matches: [] as string[], error: "" };
    try {
      // matchAll needs a global expression; add it only for the scan.
      const scanFlags = flags.includes("g") ? flags : `${flags}g`;
      const expression = new RegExp(pattern, scanFlags);
      const matches = Array.from(text.matchAll(expression))
        .slice(0, 100)
        .map((match) => match[0]);
      return { matches, error: "" };
    } catch {
      return { matches: [] as string[], error: t("tools.regex.invalid") };
    }
  }, [pattern, flags, text, t]);

  return (
    <div className="net-pane">
      <div className="net-form">
        <LabeledInput label={t("tools.regex.pattern")} value={pattern} onChange={setPattern} />
        <label className="net-number">
          <span>{t("tools.regex.flags")}</span>
          <input
            value={flags}
            spellCheck={false}
            aria-label={t("tools.regex.flags")}
            onChange={(event) => setFlags(event.target.value)}
          />
        </label>
      </div>
      <textarea
        className="tool-area"
        value={text}
        spellCheck={false}
        aria-label={t("tools.regex.text")}
        onChange={(event) => setText(event.target.value)}
      />
      {result.error ? (
        <Notice tone="warn">{result.error}</Notice>
      ) : (
        <section className="geo-card">
          <p className="tool-match-count">
            {result.matches.length
              ? t("tools.regex.matches", { count: result.matches.length })
              : t("tools.regex.noMatches")}
          </p>
          {result.matches.length > 0 && (
            <div className="tool-matches">
              {result.matches.slice(0, 24).map((match, index) => (
                <code key={`${match}-${index}`} className="tool-match">
                  {match || t("tools.regex.emptyMatch")}
                </code>
              ))}
            </div>
          )}
        </section>
      )}
    </div>
  );
}

/* ---------- encoding & conversion ---------- */

function toBase64(text: string) {
  const bytes = new TextEncoder().encode(text);
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

function fromBase64(value: string) {
  const binary = atob(value.trim());
  const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
  return new TextDecoder().decode(bytes);
}

function Base64Tool() {
  const { t } = useTranslation();
  const [input, setInput] = useState("Ferriterm");
  const [mode, setMode] = useState<"encode" | "decode">("encode");

  const result = useMemo(() => {
    if (!input.trim()) return { text: "", error: "" };
    try {
      return {
        text: mode === "encode" ? toBase64(input) : fromBase64(input),
        error: "",
      };
    } catch {
      return { text: "", error: t("tools.base64.invalid") };
    }
  }, [input, mode, t]);

  return <CodecTool input={input} setInput={setInput} mode={mode} setMode={setMode} result={result} />;
}

function UrlTool() {
  const { t } = useTranslation();
  const [input, setInput] = useState("https://example.com/?q=终端 工具");
  const [mode, setMode] = useState<"encode" | "decode">("encode");

  const result = useMemo(() => {
    if (!input.trim()) return { text: "", error: "" };
    try {
      return {
        text:
          mode === "encode" ? encodeURIComponent(input) : decodeURIComponent(input),
        error: "",
      };
    } catch {
      return { text: "", error: t("tools.url.invalid") };
    }
  }, [input, mode, t]);

  return <CodecTool input={input} setInput={setInput} mode={mode} setMode={setMode} result={result} />;
}

function CodecTool({
  input,
  setInput,
  mode,
  setMode,
  result,
}: {
  input: string;
  setInput: (value: string) => void;
  mode: "encode" | "decode";
  setMode: (value: "encode" | "decode") => void;
  result: { text: string; error: string };
}) {
  const { t } = useTranslation();
  return (
    <div className="net-pane">
      <div className="net-form">
        <Segmented
          label={t("tools.common.mode")}
          value={mode}
          onChange={(value) => setMode(value as "encode" | "decode")}
          options={[
            { value: "encode", label: t("tools.common.encode") },
            { value: "decode", label: t("tools.common.decode") },
          ]}
        />
      </div>
      <textarea
        className="tool-area"
        value={input}
        spellCheck={false}
        aria-label={t("tools.common.input")}
        onChange={(event) => setInput(event.target.value)}
      />
      {result.error && <Notice tone="warn">{result.error}</Notice>}
      <pre className={result.text ? "net-console" : "net-console empty"}>
        {result.text || t("tools.common.empty")}
      </pre>
      <div className="net-actions">
        <CopyButton value={result.text} />
        <button
          type="button"
          onClick={() => {
            if (result.text) {
              setInput(result.text);
              setMode(mode === "encode" ? "decode" : "encode");
            }
          }}
        >
          {t("tools.common.swap")}
        </button>
      </div>
    </div>
  );
}

function TimestampTool() {
  const { t } = useTranslation();
  const [input, setInput] = useState(() => String(Math.floor(Date.now() / 1000)));

  const result = useMemo(() => {
    const raw = Number(input.trim());
    if (!input.trim() || !Number.isFinite(raw)) return null;
    const milliseconds = Math.abs(raw) < 1e11 ? raw * 1000 : raw;
    const date = new Date(milliseconds);
    if (Number.isNaN(date.getTime())) return null;
    return {
      milliseconds,
      local: date.toLocaleString(),
      utc: date.toUTCString(),
      iso: date.toISOString(),
    };
  }, [input]);

  return (
    <div className="net-pane">
      <div className="net-form">
        <LabeledInput
          label={t("tools.timestamp.unix")}
          value={input}
          onChange={setInput}
          placeholder="1700000000"
        />
        <div className="net-actions">
          <button
            type="button"
            onClick={() => setInput(String(Math.floor(Date.now() / 1000)))}
          >
            {t("tools.timestamp.now")}
          </button>
        </div>
      </div>
      {!result ? (
        <Notice tone="warn">{t("tools.timestamp.invalid")}</Notice>
      ) : (
        <>
          <ResultRows
            rows={[
              { label: t("tools.timestamp.local"), value: result.local },
              { label: t("tools.timestamp.utc"), value: result.utc },
              { label: t("tools.timestamp.iso"), value: result.iso },
              { label: t("tools.timestamp.milliseconds"), value: result.milliseconds },
            ]}
          />
          <div className="net-actions">
            <CopyButton value={result.iso} />
          </div>
        </>
      )}
    </div>
  );
}

/* ---------- text ---------- */

function CounterTool() {
  const { t } = useTranslation();
  const [input, setInput] = useState("");
  const stats = useMemo(() => {
    const trimmed = input.trim();
    return {
      chars: Array.from(input).length,
      words: trimmed ? trimmed.split(/\s+/).length : 0,
      lines: input ? input.split(/\r\n|\r|\n/).length : 0,
      bytes: new TextEncoder().encode(input).length,
    };
  }, [input]);

  return (
    <div className="net-pane">
      <textarea
        className="tool-area"
        value={input}
        spellCheck={false}
        aria-label={t("tools.common.input")}
        placeholder={t("tools.counter.placeholder")}
        onChange={(event) => setInput(event.target.value)}
      />
      <ResultRows
        rows={[
          { label: t("tools.counter.chars"), value: stats.chars },
          { label: t("tools.counter.words"), value: stats.words },
          { label: t("tools.counter.lines"), value: stats.lines },
          { label: t("tools.counter.bytes"), value: stats.bytes },
        ]}
      />
      <div className="net-actions">
        <button type="button" disabled={!input} onClick={() => setInput("")}>
          {t("tools.common.clear")}
        </button>
      </div>
    </div>
  );
}

const HASH_ALGORITHMS = ["SHA-1", "SHA-256", "SHA-512"] as const;

function HashTool() {
  const { t } = useTranslation();
  const [input, setInput] = useState("Ferriterm");
  const [digests, setDigests] = useState<{ label: string; value: string }[]>([]);
  const [error, setError] = useState("");

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      if (!globalThis.crypto?.subtle) {
        setDigests([]);
        setError(t("tools.hash.unsupported"));
        return;
      }
      try {
        const data = new TextEncoder().encode(input);
        const next = await Promise.all(
          HASH_ALGORITHMS.map(async (algorithm) => {
            const buffer = await crypto.subtle.digest(algorithm, data);
            const hex = Array.from(new Uint8Array(buffer))
              .map((byte) => byte.toString(16).padStart(2, "0"))
              .join("");
            return { label: algorithm, value: hex };
          }),
        );
        if (!cancelled) {
          setDigests(next);
          setError("");
        }
      } catch {
        if (!cancelled) {
          setDigests([]);
          setError(t("tools.common.error"));
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [input, t]);

  const all = digests.map((row) => `${row.label}: ${row.value}`).join("\n");

  return (
    <div className="net-pane">
      <textarea
        className="tool-area"
        value={input}
        spellCheck={false}
        aria-label={t("tools.common.input")}
        onChange={(event) => setInput(event.target.value)}
      />
      {error && <Notice tone="warn">{error}</Notice>}
      {digests.length > 0 && (
        <>
          <ResultRows
            rows={digests.map((row) => ({ label: row.label, value: row.value }))}
          />
          <div className="net-actions">
            <CopyButton value={all} />
          </div>
        </>
      )}
    </div>
  );
}

function fallbackUuid() {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = Array.from(bytes)
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

function UuidTool() {
  const { t } = useTranslation();
  const [count, setCount] = useState(5);
  const [seed, setSeed] = useState(0);
  const values = useMemo(() => {
    const total = Math.min(50, Math.max(1, Number.isFinite(count) ? count : 1));
    return Array.from({ length: total }, () =>
      typeof crypto.randomUUID === "function" ? crypto.randomUUID() : fallbackUuid(),
    );
    // `seed` is the "generate again" trigger.
  }, [count, seed]);
  const joined = values.join("\n");

  return (
    <div className="net-pane">
      <div className="net-form">
        <label className="net-number">
          <span>{t("tools.uuid.count")}</span>
          <input
            type="number"
            min={1}
            max={50}
            value={count}
            onChange={(event) => setCount(Number.parseInt(event.target.value, 10) || 1)}
          />
        </label>
        <div className="net-actions">
          <button type="button" onClick={() => setSeed((value) => value + 1)}>
            {t("tools.uuid.generate")}
          </button>
        </div>
      </div>
      <pre className="net-console">{joined || t("tools.common.empty")}</pre>
      <div className="net-actions">
        <CopyButton value={joined} />
      </div>
    </div>
  );
}
