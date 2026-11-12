import { aiGenerateCommand, type AiCommandReply, type AiStep } from "./api";
import { terminalHandle } from "./terminal-registry";
import { runGuarded } from "./policy";

/**
 * The continuous agent loop.
 *
 * Drives one terminal session on its own: read the screen, ask the model for
 * the next single command, run it through the policy gate (as `agent`, so an
 * `ask` verdict pauses the loop and waits for the approval card), wait for the
 * output to settle, and repeat until the goal is met or a stop condition hits.
 *
 * It never types without the newline, and every command still goes through
 * `runGuarded` — the policy gate is the one place a command reaches a terminal,
 * and the agent is no exception.
 */

export type AgentStepVerdict =
  | "ran"
  | "ask-denied"
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
  /** The model's one-sentence rationale for this step, shown as its "thinking". */
  thinking?: string;
}

export interface AgentLoopState {
  running: boolean;
  goal: string;
  sessionId: string;
  steps: AgentStep[];
  status: string;
  /** True while the model is generating the next command. */
  thinking: boolean;
}

export interface AgentLoopOptions {
  sessionId: string;
  goal: string;
  providerId: string;
  /** Model chosen in the panel; empty uses the provider default. */
  model?: string;
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
/** One extra attempt per step before the miss counts against the run. */
const RETRY_DELAYS_MS = [400];
/** Consecutive unusable replies before the run gives up with a reason. */
const MAX_SKIPS = 2;
/** Ceiling on pager advances so a stuck console cannot spin forever. */
const MAX_PAGES = 200;

/**
 * Network consoles page long output and then sit waiting for a key. The screen
 * looks perfectly stable at that point, so a plain "is it quiet?" check would
 * declare the command finished and the next command would be eaten as a pager
 * keystroke. These patterns are what Huawei VRP, H3C Comware and Cisco IOS
 * print at the bottom of a full page.
 */
const PAGER_PATTERNS = [
  /-{2,}\s*more\s*-{2,}/i,
  /--\s*more\s*--/i,
  /\(\s*more\s*\)/i,
  /<\s*-+\s*more\s*-+\s*>/i,
  /--\s*more\s*--\s*,\s*next\s*page/i,
  /press\s+any\s+key/i,
  /\(\s*q\s*\)\s*uit/i,
];

function looksLikePager(text: string): boolean {
  const bottom = text.slice(-400);
  return PAGER_PATTERNS.some((pattern) => pattern.test(bottom));
}

/**
 * A console waiting for a password looks exactly like a console waiting for the
 * next command: quiet. Typing the next command into it would feed that line to
 * `sudo` as the password, so the loop has to recognise the prompt and stop —
 * only a person can answer it.
 */
const AUTH_PROMPTS = [
  /\[sudo\]\s+password\s+for\s+\S+/i,
  /password\s+for\s+\S+\s*:\s*$/i,
  /^\s*(?:current\s+)?(?:unix\s+)?password\s*:\s*$/im,
  /\(current\)\s+unix\s+password/i,
  /enter\s+passphrase\s+for\s+key/i,
  /^\s*passphrase\s*.*:\s*$/im,
];

function awaitingPassword(text: string): boolean {
  const bottom = text.slice(-400);
  return AUTH_PROMPTS.some((pattern) => pattern.test(bottom));
}

/** Page through whatever the console already has waiting, before typing. */
async function drainPager(
  handle: ReturnType<typeof terminalHandle> | null,
  max: number,
): Promise<void> {
  if (!handle) return;
  for (let page = 0; page < MAX_PAGES; page += 1) {
    const screen = handle.screenLines(max).join("\n");
    if (!looksLikePager(screen)) return;
    handle.insert(" ");
    await sleep(150);
  }
}

function buildRequest(goal: string, index: number): string {
  return (
    `Goal: ${goal}\n` +
    `You are in continuous mode and act step by step; this is step ${index + 1}. ` +
    `If the goal is already met, reply {"done": true, "explanation": "..."} and stop. ` +
    `Otherwise reply {"command": "...", "explanation": "..."} with exactly ONE command that makes progress. ` +
    `Never send an empty command. Do not repeat a command whose result you have already seen.`
  );
}

function tail(text: string, max = OUTPUT_TAIL): string {
  return text.length > max ? text.slice(text.length - max) : text;
}

interface SettledOutput {
  text: string;
  /** The console is asking for a password; nothing else may be typed. */
  paused: boolean;
}

async function waitForSettled(
  handle: ReturnType<typeof terminalHandle> | null,
  max: number,
  timeoutMs: number,
): Promise<SettledOutput> {
  if (!handle) return { text: "", paused: false };
  const deadline0 = Date.now() + timeoutMs;
  // Paging is genuine progress, so each page may extend the wait — but never
  // past a hard ceiling, or a console stuck on a pager would run forever.
  const hardDeadline = Date.now() + timeoutMs * 4;
  let deadline = deadline0;
  let last = handle.screenLines(max).join("\n");
  let stable = 0;
  let pages = 0;
  while (Date.now() < deadline) {
    await sleep(SAMPLE_MS);
    const current = handle.screenLines(max).join("\n");
    if (current === last) {
      stable += 1;
      if (stable >= STABLE_SAMPLES) {
        // A password prompt is stable too, but it is waiting on a person.
        if (awaitingPassword(current)) {
          return { text: current, paused: true };
        }
        // A pager prompt is stable too, but it is not the end of the output.
        if (pages < MAX_PAGES && looksLikePager(current)) {
          handle.insert(" ");
          pages += 1;
          stable = 0;
          deadline = Math.min(hardDeadline, Date.now() + timeoutMs);
          continue;
        }
        break;
      }
    } else {
      stable = 0;
      last = current;
    }
  }
  return { text: last, paused: awaitingPassword(last) };
}

const TERMINAL = new Set([
  "done",
  "denied",
  "aborted",
  "timeout",
  "errors",
  "blocked",
  "password",
  "no-session",
]);

export function startAgentLoop(options: AgentLoopOptions): AgentLoopHandle {
  const {
    sessionId,
    goal,
    providerId,
    model,
    host,
    maxSteps = 20,
    maxStepMs = 15000,
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
    thinking: false,
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
      state.thinking = true;
      emit();

      // A previous command (or the user) may have left the console on a pager
      // page; page it out first so the model reads the whole reply.
      await drainPager(handle, lines);
      // A prompt nobody answered outranks the plan: typing here would feed the
      // next command to `sudo` as the password.
      if (awaitingPassword(handle.screenLines(lines).join("\n"))) {
        pushStep({ index, command: "", verdict: "failed", note: "authPrompt" });
        state.status = "password";
        break;
      }
      const screen = handle.screenLines(lines);
      let reply: AiCommandReply | null = null;
      let lastError = "";
      for (let attempt = 0; attempt <= RETRY_DELAYS_MS.length; attempt += 1) {
        try {
          const candidate = await aiGenerateCommand(providerId, buildRequest(goal, index), {
            host: host || null,
            shell: null,
            model: model || null,
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
          // `done` is the only finish signal. An empty command without it means
          // the model broke the contract, which is retried rather than treated
          // as either success or an immediate failure.
          if (candidate.done || (candidate.command || "").trim()) {
            reply = candidate;
            break;
          }
          lastError = "ai.err.emptyCommand";
        } catch (err) {
          lastError = String(err);
        }
        if (attempt < RETRY_DELAYS_MS.length) {
          await sleep(RETRY_DELAYS_MS[attempt]);
          if (token.aborted) break;
        }
      }

      state.thinking = false;
      if (!reply) {
        pushStep({ index, command: "", verdict: "failed", note: lastError });
        consecutiveSkips += 1;
        if (consecutiveSkips >= MAX_SKIPS) {
          state.status = "errors";
          break;
        }
        continue;
      }

      const command = (reply.command || "").trim();
      const thinking = (reply.explanation || "").trim();
      if (reply.done) {
        pushStep({ index, command: "", verdict: "done", thinking });
        state.status = "done";
        break;
      }

      // The gate may stop here to ask a person. Say so before awaiting, so the
      // panel explains the pause instead of looking stuck.
      state.status = "waiting";
      emit();
      const outcome = await runGuarded(command, {
        host,
        elevated: false,
        origin: "agent",
        run: () => {
          // A stop pressed while the approval card was open outranks a late
          // approval: the person already said the run is over.
          if (token.aborted) return;
          handle.insert(`${command}\n`);
        },
      });
      state.status = `step ${index + 1}/${maxSteps}`;
      emit();

      if (outcome.status === "ran") {
        if (token.aborted) {
          pushStep({ index, command, verdict: "aborted", note: "user-stopped", thinking });
          state.status = "aborted";
          break;
        }
        consecutiveSkips = 0;
        const after = await waitForSettled(handle, lines, maxStepMs);
        if (after.paused) {
          // The command itself asked for a password. Stop here rather than
          // typing the next step into that prompt.
          pushStep({
            index,
            command,
            verdict: "failed",
            note: "authPrompt",
            output: tail(after.text, 800),
            thinking,
          });
          state.status = "password";
          break;
        }
        const output = tail(after.text);
        history.push({ command, output });
        pushStep({ index, command, verdict: "ran", output, thinking });
      } else if (outcome.status === "denied") {
        const reason = outcome.decision.hard
          ? "fatal"
          : outcome.decision.reasons.join(",") || "policy";
        pushStep({ index, command, verdict: "denied", note: reason, thinking });
        state.status = "denied";
        break;
      } else if (outcome.status === "cancelled") {
        // A person said no. The model gets to pick another route, but two
        // refusals in a row mean the run has lost the plot.
        pushStep({
          index,
          command,
          verdict: "ask-denied",
          note: "policy-ask",
          thinking,
        });
        consecutiveSkips += 1;
        if (consecutiveSkips >= MAX_SKIPS) {
          state.status = "blocked";
          break;
        }
      } else {
        pushStep({
          index,
          command,
          verdict: "failed",
          note: outcome.message,
          thinking,
        });
        consecutiveSkips += 1;
        if (consecutiveSkips >= MAX_SKIPS) {
          state.status = "errors";
          break;
        }
      }
    }

    if (state.running && !TERMINAL.has(state.status)) {
      // Reaching here means the for-loop ended by hitting maxSteps, so the
      // last status is still a "step N/M" string rather than a terminal one.
      state.status = state.status.startsWith("step") ? "maxSteps" : "finished";
    }
    // Whatever ended the run, nothing is being generated any more.
    state.thinking = false;
    state.running = false;
    emit();
  })();

  return {
    stop() {
      token.aborted = true;
    },
  };
}
