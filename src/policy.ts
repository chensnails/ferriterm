import {
  policyAddRule,
  policyEvaluate,
  policyRecord,
  type AuditChoice,
  type AuditOrigin,
  type PolicyDecision,
} from "./api";
import { useAppStore } from "./store";

/**
 * The one place a command reaches a terminal without a person typing it.
 *
 * Everything automatic goes through here: today the assistant's "run" action,
 * later the CLI agent and the MCP tools. The gate asks `ft-policy` for a
 * verdict and honours it — `allow` runs, `ask` opens the approval card, `deny`
 * never runs. A typed command does not come through here; the person at the
 * keyboard is the authorisation.
 */

/** "For this session" grants live only as long as the window is open. */
const sessionGrants = new Set<string>();

function grantKey(host: string, command: string) {
  return `${host}\u0000${command}`;
}

/** Forget every in-memory grant (called when the policy changes underneath). */
export function clearSessionGrants() {
  sessionGrants.clear();
}

export type GateOutcome =
  | { status: "ran" }
  | { status: "denied"; decision: PolicyDecision }
  | { status: "cancelled" }
  | { status: "failed"; message: string };

export interface GateOptions {
  /** Host name or label the command would run against. */
  host: string;
  /** The session already has root, so nothing needs `sudo` to be dangerous. */
  elevated: boolean;
  /** Actually run it. Only ever called after the verdict says so. */
  run: () => void;
  /** Who is asking. Defaults to `"ui"`; the continuous agent passes `"agent"`. */
  origin?: AuditOrigin;
}

/**
 * Append the decision to the audit chain. Best effort on purpose: failing to
 * write a log line must not turn a refusal into an execution, or hold up a
 * command the person already approved.
 */
function record(
  command: string,
  host: string,
  decision: PolicyDecision,
  choice: AuditChoice,
  origin: AuditOrigin,
) {
  // Everything the window does says so, so a record without an origin really
  // does mean "written before the field existed".
  void policyRecord({ command, host, decision, choice, origin }).catch(() => {});
}

export async function runGuarded(
  command: string,
  options: GateOptions,
): Promise<GateOutcome> {
  const line = command.trim();
  if (!line) return { status: "cancelled" };
  const { host, elevated, run, origin = "ui" } = options;

  let decision: PolicyDecision;
  try {
    decision = await policyEvaluate(line, host, elevated);
  } catch (err) {
    return { status: "failed", message: String(err) };
  }

  if (decision.verdict === "allow") {
    record(line, host, decision, "auto", origin);
    run();
    return { status: "ran" };
  }
  if (decision.verdict === "deny") {
    record(line, host, decision, "denied", origin);
    return { status: "denied", decision };
  }
  if (sessionGrants.has(grantKey(host, line))) {
    record(line, host, decision, "session", origin);
    run();
    return { status: "ran" };
  }

  // Nobody approves on the person's behalf: even the autonomous loop stops
  // here and waits for the card. A step that cannot be approved is a step the
  // person never saw, and silently refusing it would look like a failure.
  const choice = await useAppStore.getState().askApprove({
    command: line,
    host,
    decision,
    origin,
  });
  if (!choice) {
    record(line, host, decision, "cancelled", origin);
    return { status: "cancelled" };
  }

  if (choice === "always") {
    // An exact line, scoped to this host: the narrowest rule that keeps the
    // promise the button makes. Anything broader would allow commands the
    // person never saw.
    try {
      await policyAddRule(line, "allow", host, line);
    } catch {
      // The approval still stands for this run; the rule is a convenience.
    }
  } else if (choice === "session") {
    sessionGrants.add(grantKey(host, line));
  }

  record(line, host, decision, choice, origin);
  run();
  return { status: "ran" };
}
