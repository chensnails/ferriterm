import { aiGenerateCommand, type AiStep } from "./api";
import { terminalHandle } from "./terminal-registry";
import { runGuarded } from "./policy";

/**
 * The continuous agent loop.
 *
 * Drives one terminal session on its own: read the screen, ask the model for
 * the next single command, run it through the policy gate (as `agent`, so an
 * `ask` verdict is auto-cancelled instead of waiting for a person), wait for the
 * output to settle, and repeat until the goal is met or a stop condition hits.
 *
 * It never types without the newline, and every command still goes through
 * `runGuarded` — the policy gate is the one place a command reaches a terminal,
 * and the agent is no exception.
 */

export type AgentStepVerdict =
  | "ran"
  | "ask-cancelled"
  | "denied"
  | "failed"
  | "done"
  | "aborted";

export interface AgentStep {
  index: number;
  command: string;
  verdict: AgentStepVerdict;
  output?: string;
  note?: string;
}

export interface AgentLoopState {
  running: boolean;
  goal: string;
  sessionId: string;
  steps: AgentStep[];
  status: string;
}

export interface AgentLoopOptions {
  sessionId: string;
  goal: string;
  providerId: string;
  host: string;
  /** Ceiling on executed steps. */
  maxSteps?: number;
  /** How long to wait for a command's output to settle. */
  maxStepMs?: number;
  /** Wall-clock ceiling for the whole run. */
  maxDurationMs?: number;
  /** Screen lines read each round and sent as context. */
  screenLines?: number;
  onUpdate?: (state: AgentLoopState) => void;
}

export interface AgentLoopHandle {
  stop(): void;
}

const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

/** Three stable snapshots in a row (~900ms) means the command finished. */
const STABLE_SAMPLES = 3;
const SAMPLE_MS = 300;
const OUTPUT_TAIL = 3000;

function buildRequest(goal: string, index: number): string {
  return (
    `Goal: ${goal}\n` +
    `You are in continuous mode and act step by step; this is step ${index + 1}. ` +
    `If the goal is already met, return an EMPTY command so the run ends. ` +
    `Otherwise return exactly ONE shell command that makes progress. ` +
    `Do not repeat a command whose result you have already seen.`
  );
}

function tail(text: string, max = OUTPUT_TAIL): string {
  return text.length > max ? text.slice(text.length - max) : text;
}

async function waitForSettled(
  handle: ReturnType<typeof terminalHandle> | null,
  max: number,
  timeoutMs: number,
): Promise<string> {
  if (!handle) return "";
  const deadline = Date.now() + timeoutMs;
  let last = handle.screenLines(max).join("\n");
  let stable = 0;
  while (Date.now() < deadline) {
    await sleep(SAMPLE_MS);
    const current = handle.screenLines(max).join("\n");
    if (current === last) {
      stable += 1;
      if (stable >= STABLE_SAMPLES) break;
    } else {
      stable = 0;
      last = current;
    }
  }
  return last;
}

const TERMINAL = new Set([
  "done",
  "denied",
  "aborted",
  "timeout",
  "errors",
  "blocked",
  "no-session",
]);

export function startAgentLoop(options: AgentLoopOptions): AgentLoopHandle {
  const {
    sessionId,
    goal,
    providerId,
    host,
    maxSteps = 20,
    maxStepMs = 8000,
    maxDurationMs = 10 * 60 * 1000,
    screenLines: lines = 60,
    onUpdate,
  } = options;

  const token = { aborted: false };
  const steps: AgentStep[] = [];
  const state: AgentLoopState = {
    running: true,
    goal,
    sessionId,
    steps,
    status: "starting",
  };
  const emit = () => onUpdate?.(state);
  const pushStep = (step: AgentStep) => {
    steps.push(step);
    emit();
  };

  void (async () => {
    const handle = terminalHandle(sessionId);
    if (!handle) {
      state.status = "no-session";
      state.running = false;
      emit();
      return;
    }
    const startedAt = Date.now();
    let consecutiveSkips = 0;
    const history: AiStep[] = [];

    for (let index = 0; index < maxSteps; index += 1) {
      if (token.aborted) {
        pushStep({ index, command: "", verdict: "aborted", note: "user-stopped" });
        state.status = "aborted";
        break;
      }
      if (Date.now() - startedAt > maxDurationMs) {
        state.status = "timeout";
        break;
      }
      state.status = `step ${index + 1}/${maxSteps}`;
      emit();

      const screen = handle.screenLines(lines);
      let reply;
      try {
        reply = await aiGenerateCommand(providerId, buildRequest(goal, index), {
          host: host || null,
          shell: null,
          context: {
            selection: null,
            lastCommand: null,
            screen,
            scope: "screen",
            lines,
            sendOutput: true,
            redact: true,
            history,
          },
        });
      } catch (err) {
        pushStep({ index, command: "", verdict: "failed", note: String(err) });
        consecutiveSkips += 1;
        if (consecutiveSkips >= STABLE_SAMPLES) {
          state.status = "errors";
          break;
        }
        continue;
      }

      const command = (reply.command || "").trim();
      if (!command) {
        pushStep({ index, command: "", verdict: "done" });
        state.status = "done";
        break;
      }

      const outcome = await runGuarded(command, {
        host,
        elevated: false,
        origin: "agent",
        autonomous: true,
        run: () => handle.insert(`${command}\n`),
      });

      if (outcome.status === "ran") {
        consecutiveSkips = 0;
        const after = await waitForSettled(handle, lines, maxStepMs);
        const output = tail(after);
        history.push({ command, output });
        pushStep({ index, command, verdict: "ran", output });
      } else if (outcome.status === "denied") {
        const reason = outcome.decision.hard
          ? "fatal"
          : outcome.decision.reasons.join(",") || "policy";
        pushStep({ index, command, verdict: "denied", note: reason });
        state.status = "denied";
        break;
      } else if (outcome.status === "cancelled") {
        pushStep({ index, command, verdict: "ask-cancelled", note: "policy-ask" });
        consecutiveSkips += 1;
        if (consecutiveSkips >= STABLE_SAMPLES) {
          state.status = "blocked";
          break;
        }
      } else {
        pushStep({
          index,
          command,
          verdict: "failed",
          note: outcome.message,
        });
        consecutiveSkips += 1;
        if (consecutiveSkips >= STABLE_SAMPLES) {
          state.status = "errors";
          break;
        }
      }
    }

    if (state.running && !TERMINAL.has(state.status)) {
      state.status = "finished";
    }
    state.running = false;
    emit();
  })();

  return {
    stop() {
      token.aborted = true;
    },
  };
}
