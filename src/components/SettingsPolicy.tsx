import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  isTauri,
  policyAddRule,
  policyAuditList,
  policyAuditMonths,
  policyAuditPath,
  policyAuditVerify,
  policyGet,
  policyRemoveRule,
  policySave,
  type AuditEntry,
  type AuditVerifyReport,
  type PolicyConfig,
  type PolicyEffect,
  type PolicyRisk,
} from "../api";
import { useAppStore } from "../store";
import { Chip, EmptyState, Notice, SettingGroup, SettingRow } from "./ui";
import { SegmentedSetting } from "./SettingsControls";

const EMPTY: PolicyConfig = { mode: "auto", rules: [] };
const AUDIT_ROWS = 40;

/** Settings → Security: the gate's mode, its rules, and the audit log. */
export function SettingsPolicy() {
  const { t } = useTranslation();
  const askConfirm = useAppStore((s) => s.askConfirm);

  const [config, setConfig] = useState<PolicyConfig>(EMPTY);
  const [months, setMonths] = useState<string[]>([]);
  const [month, setMonth] = useState("");
  const [entries, setEntries] = useState<AuditEntry[]>([]);
  const [report, setReport] = useState<AuditVerifyReport | null>(null);
  const [where, setWhere] = useState("");
  const [pattern, setPattern] = useState("");
  const [effect, setEffect] = useState<PolicyEffect>("allow");
  const [ruleHost, setRuleHost] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!isTauri) return;
    void (async () => {
      try {
        const [loaded, monthList] = await Promise.all([
          policyGet(),
          policyAuditMonths(),
        ]);
        setConfig(loaded);
        setMonths(monthList);
        setMonth(monthList[monthList.length - 1] ?? "");
      } catch (err) {
        setError(String(err));
      }
    })();
  }, []);

  useEffect(() => {
    if (!isTauri || !month) {
      setEntries([]);
      return;
    }
    void (async () => {
      try {
        const [list, file] = await Promise.all([
          policyAuditList(month),
          policyAuditPath(month),
        ]);
        setEntries(list);
        setWhere(file);
        setReport(null);
      } catch (err) {
        setError(String(err));
      }
    })();
  }, [month]);

  const setMode = async (mode: string) => {
    const next = { ...config, mode: mode as PolicyConfig["mode"] };
    setConfig(next);
    try {
      await policySave(next);
    } catch (err) {
      setError(String(err));
    }
  };

  const addRule = async () => {
    if (!pattern.trim()) return;
    try {
      const next = await policyAddRule(
        pattern.trim(),
        effect,
        ruleHost.trim() || undefined,
      );
      setConfig(next);
      setPattern("");
    } catch (err) {
      setError(String(err));
    }
  };

  const removeRule = async (id: string) => {
    const confirmed = await askConfirm({
      title: t("policy.settings.removeRule"),
      body: t("policy.settings.removeRuleBody"),
      danger: true,
      confirmLabel: t("common.delete"),
    });
    if (!confirmed) return;
    try {
      setConfig(await policyRemoveRule(id));
    } catch (err) {
      setError(String(err));
    }
  };

  const verify = async () => {
    setBusy(true);
    try {
      const reports = await policyAuditVerify(month || undefined);
      setReport(reports[0] ?? null);
      // Commands run by the agent and the CLI land in the same file while this
      // page is open, so a verify also refreshes what is on screen. Otherwise
      // the chain would report ten records above a list still showing three.
      if (month) {
        const [list, file] = await Promise.all([
          policyAuditList(month),
          policyAuditPath(month),
        ]);
        setEntries(list);
        setWhere(file);
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const recent = [...entries].reverse().slice(0, AUDIT_ROWS);

  return (
    <>
      {error && (
        <Notice tone="danger">
          {error}{" "}
          <button type="button" className="link-btn" onClick={() => setError(null)}>
            {t("common.close")}
          </button>
        </Notice>
      )}

      <SettingGroup
        title={t("policy.settings.modeGroup")}
        hint={t("policy.settings.modeHint")}
      >
        <SegmentedSetting
          title={t("policy.settings.mode")}
          hint={t("policy.settings.modeDetail")}
          value={config.mode}
          onChange={(value) => void setMode(value)}
          options={[
            { value: "auto", label: t("policy.mode.auto") },
            { value: "ask", label: t("policy.mode.ask") },
            { value: "plan", label: t("policy.mode.plan") },
          ]}
        />
        <SettingRow
          title={t("policy.settings.manualUnless")}
          hint={t("policy.settings.manualUnlessHint")}
        >
          <span className="ui-setting-current">{t("policy.settings.manualOnly")}</span>
        </SettingRow>
      </SettingGroup>

      <SettingGroup
        title={t("policy.settings.rulesGroup")}
        hint={t("policy.settings.rulesHint")}
      >
        <SettingRow
          title={t("policy.settings.addRule")}
          hint={t("policy.settings.addRuleHint")}
          stack
        >
          <div className="policy-form">
            <input
              className="ai-field"
              value={pattern}
              placeholder={t("policy.settings.patternPlaceholder")}
              spellCheck={false}
              onChange={(event) => setPattern(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") void addRule();
              }}
            />
            <select
              className="ai-field"
              value={effect}
              aria-label={t("policy.settings.effect")}
              onChange={(event) => setEffect(event.target.value as PolicyEffect)}
            >
              <option value="allow">{t("policy.settings.effectAllow")}</option>
              <option value="ask">{t("policy.settings.effectAsk")}</option>
              <option value="deny">{t("policy.settings.effectDeny")}</option>
            </select>
            <input
              className="ai-field"
              value={ruleHost}
              placeholder={t("policy.settings.hostPlaceholder")}
              spellCheck={false}
              onChange={(event) => setRuleHost(event.target.value)}
            />
            <button
              type="button"
              className="primary"
              disabled={pattern.trim().length === 0}
              onClick={() => void addRule()}
            >
              {t("policy.settings.add")}
            </button>
          </div>
        </SettingRow>

        {config.rules.length === 0 ? (
          <EmptyState title={t("policy.settings.noRules")} />
        ) : (
          <div className="policy-list">
            {config.rules.map((rule) => (
              <div className="policy-row" key={rule.id}>
                <Chip tone={effectTone(rule.effect)}>
                  {t(`policy.settings.effect_${rule.effect}`)}
                </Chip>
                <div className="policy-row-main">
                  <span className="policy-row-pattern">{rule.pattern}</span>
                  <span className="policy-row-meta">
                    {rule.host
                      ? t("policy.settings.onHost", { host: rule.host })
                      : t("policy.settings.onAnyHost")}
                  </span>
                </div>
                <button
                  type="button"
                  className="link-btn danger"
                  onClick={() => void removeRule(rule.id)}
                >
                  {t("common.delete")}
                </button>
              </div>
            ))}
          </div>
        )}
      </SettingGroup>

      <SettingGroup
        title={t("policy.settings.auditGroup")}
        hint={t("policy.settings.auditHint")}
      >
        <SettingRow
          title={t("policy.settings.auditMonth")}
          hint={t("policy.settings.auditMonthHint")}
        >
          <div className="policy-form">
            <select
              className="ai-field"
              value={month}
              aria-label={t("policy.settings.auditMonth")}
              onChange={(event) => setMonth(event.target.value)}
            >
              {months.length === 0 && <option value="">{t("policy.settings.noAudit")}</option>}
              {months.map((item) => (
                <option key={item} value={item}>
                  {item.slice(0, 4)}-{item.slice(4)}
                </option>
              ))}
            </select>
            <button
              type="button"
              disabled={busy || !month}
              onClick={() => void verify()}
            >
              {busy ? t("policy.settings.verifying") : t("policy.settings.verify")}
            </button>
          </div>
        </SettingRow>

        {report && (
          <SettingRow
            title={t("policy.settings.verifyResult")}
            hint={
              report.ok
                ? t("policy.settings.verifyOkBody", { count: report.entries })
                : t("policy.settings.verifyBrokenBody", {
                    at: report.brokenAt ?? 0,
                    reason: report.brokenReason ?? "hash",
                  })
            }
          >
            <Chip tone={report.ok ? "info" : "danger"}>
              {report.ok ? t("policy.settings.verifyOk") : t("policy.settings.verifyBroken")}
            </Chip>
          </SettingRow>
        )}

        {where && (
          <SettingRow title={t("policy.settings.auditFile")} hint={where}>
            <span className="ui-setting-current">
              {t("policy.settings.auditRecords", { count: entries.length })}
            </span>
          </SettingRow>
        )}

        {recent.length === 0 ? (
          <EmptyState title={t("policy.settings.auditEmpty")} />
        ) : (
          <div className="policy-list">
            {recent.map((entry) => (
              <div className="policy-row" key={entry.id}>
                <span className="policy-row-time">{entry.ts.slice(11, 19)}</span>
                <div className="policy-row-main">
                  <span className="policy-row-pattern">{entry.command}</span>
                  <span className="policy-row-meta">
                    {entry.host || t("policy.approve.localHost")} · {entry.ts.slice(0, 10)}
                  </span>
                </div>
                <Chip tone={riskTone(entry.risk)}>{t(`policy.risk.${entry.risk}`)}</Chip>
                <Chip tone={entry.choice === "cancelled" || entry.choice === "denied" ? "warn" : "neutral"}>
                  {t(`policy.settings.choice_${entry.choice}`)}
                </Chip>
              </div>
            ))}
          </div>
        )}
      </SettingGroup>
    </>
  );
}

function riskTone(risk: PolicyRisk): "neutral" | "info" | "warn" | "danger" {
  switch (risk) {
    case "r3":
      return "warn";
    case "r4":
      return "danger";
    case "r2":
      return "info";
    default:
      return "neutral";
  }
}

function effectTone(effect: PolicyEffect): "info" | "warn" | "danger" {
  if (effect === "allow") return "info";
  if (effect === "ask") return "warn";
  return "danger";
}
