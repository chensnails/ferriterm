import { useTranslation } from "react-i18next";
import type { PolicyRisk } from "../api";
import { useAppStore } from "../store";
import { Modal } from "./Modal";
import { Chip } from "./ui";

/**
 * The gate a command passes through when the policy answers "ask".
 *
 * It shows the literal line — not a summary, because the whole point is that
 * the person approves what will actually run — plus why it was flagged. The
 * four answers differ only in how long they last: one run, this window, or
 * until the rule is deleted in settings.
 */
export function ApproveDialog() {
  const { t } = useTranslation();
  const pending = useAppStore((s) => s.appPrompt);
  const settle = useAppStore((s) => s.settlePrompt);

  if (!pending || pending.request.kind !== "approve") return null;
  const { command, host, decision, origin } = pending.request;

  const cancel = () => settle(null);

  return (
    <Modal
      title={t("policy.approve.title")}
      subtitle={t("policy.approve.subtitle", { host: host || t("policy.approve.localHost") })}
      onClose={cancel}
      wide
      footer={
        <>
          <button type="button" onClick={cancel}>
            {t("policy.approve.deny")}
          </button>
          <button
            type="button"
            className="primary"
            onClick={() => settle("once")}
          >
            {t("policy.approve.once")}
          </button>
          <button type="button" onClick={() => settle("session")}>
            {t("policy.approve.session")}
          </button>
          {/* Deliberately not the primary button: the narrowest approval is
              the default, and widening it to a rule takes intent. */}
          <button type="button" onClick={() => settle("always")}>
            {t("policy.approve.always")}
          </button>
        </>
      }
    >
      <div className="approve-body">
        <div className="approve-meta">
          <Chip tone={riskTone(decision.risk)}>
            {t(`policy.risk.${decision.risk}`)}
          </Chip>
          {decision.reasons.map((reason) => (
            <Chip key={reason} tone="neutral">
              {t(`policy.reason.${reason}`)}
            </Chip>
          ))}
        </div>
        <pre className="approve-command">{command}</pre>
        {origin === "agent" && (
          <p className="approve-origin">{t("policy.approve.fromAgent")}</p>
        )}
        <p className="hint">{t("policy.approve.hint")}</p>
      </div>
    </Modal>
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
