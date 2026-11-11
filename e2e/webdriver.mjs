// Minimal W3C WebDriver client for tauri-driver. No Selenium/WebdriverIO: the
// smoke test needs find, click, keys, screenshot and script, and keeping the
// harness dependency-free means the E2E job only installs tauri-driver.

import { spawn } from "node:child_process";

const ELEMENT_KEY = "element-6066-11e4-a52e-4f735466cecf";
const BASE_URL = "http://127.0.0.1:4444";
const START_TIMEOUT_MS = 30_000;
const SESSION_TIMEOUT_MS = 90_000;

export const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

export class WebDriverError extends Error {
  constructor(error, message, path) {
    super(`${error}${message ? `: ${message}` : ""} (${path})`);
    this.error = error;
    this.path = path;
  }
}

async function readPayload(response) {
  const text = await response.text();
  return text ? JSON.parse(text) : {};
}

function failure(response, payload, path) {
  const value = payload.value ?? {};
  return new WebDriverError(
    value.error ?? `HTTP ${response.status}`,
    value.message ?? "",
    path,
  );
}

/** Polls `check` until it returns a truthy value; rejects with the last error. */
export async function waitFor(label, check, { timeoutMs = 20_000, intervalMs = 200 } = {}) {
  const deadline = Date.now() + timeoutMs;
  let lastError;
  for (;;) {
    try {
      const value = await check();
      if (value) return value;
      lastError = undefined;
    } catch (error) {
      if (error.fatal) throw error;
      lastError = error;
    }
    if (Date.now() >= deadline) {
      throw new Error(
        `timed out after ${timeoutMs} ms waiting for ${label}${lastError ? `: ${lastError.message}` : ""}`,
      );
    }
    await sleep(intervalMs);
  }
}

/**
 * Starts tauri-driver and waits for its status endpoint. The child inherits
 * `env`, which is where the automation flag and the isolated data dirs go.
 */
export async function startTauriDriver({ env = process.env } = {}) {
  const binary = process.env.TAURI_DRIVER_BIN || "tauri-driver";
  const child = spawn(binary, [], { env, stdio: ["ignore", "inherit", "inherit"] });
  let exit = null;
  let spawnError = null;
  child.once("error", (error) => {
    spawnError = error;
  });
  child.once("exit", (code, signal) => {
    exit = { code, signal };
  });

  try {
    await waitFor(
      "tauri-driver to listen on :4444",
      async () => {
        if (spawnError || exit) {
          const reason = spawnError
            ? `could not start tauri-driver: ${spawnError.message}`
            : `tauri-driver exited (code ${exit.code}, signal ${exit.signal})`;
          const error = new Error(reason);
          error.fatal = true;
          throw error;
        }
        try {
          const response = await fetch(`${BASE_URL}/status`, {
            signal: AbortSignal.timeout(1_000),
          });
          return response.ok;
        } catch {
          return false;
        }
      },
      { timeoutMs: START_TIMEOUT_MS, intervalMs: 250 },
    );
  } catch (error) {
    child.kill("SIGKILL");
    throw error;
  }

  return {
    get exited() {
      return exit;
    },
    async stop() {
      if (exit || spawnError) return;
      child.kill("SIGTERM");
      const closed = await Promise.race([
        new Promise((resolve) => child.once("exit", () => resolve(true))),
        sleep(5_000).then(() => false),
      ]);
      if (!closed) child.kill("SIGKILL");
    },
  };
}

/** Creates a session for the app binary; resolves once the window is up.
    tauri-driver starts listening before its native driver does, so transport
    failures during the first moments are retried, while a real WebDriver error
    is final. */
export async function openSession(application) {
  const deadline = Date.now() + START_TIMEOUT_MS;
  let lastError;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(`${BASE_URL}/session`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          capabilities: {
            alwaysMatch: {
              browserName: "wry",
              "tauri:options": { application },
            },
          },
        }),
        signal: AbortSignal.timeout(SESSION_TIMEOUT_MS),
      });
      const payload = await readPayload(response);
      if (!response.ok) throw failure(response, payload, "/session");
      const value = payload.value ?? {};
      const sessionId = value.sessionId ?? payload.sessionId;
      if (!sessionId) throw new Error("webdriver: session response carried no session id");
      return new Session(sessionId);
    } catch (error) {
      if (error instanceof WebDriverError) throw error;
      lastError = error;
      await sleep(300);
    }
  }
  throw new Error(
    `webdriver: no session after ${START_TIMEOUT_MS} ms${lastError ? `: ${lastError.message}` : ""}`,
  );
}

class Session {
  constructor(sessionId) {
    this.sessionId = sessionId;
  }

  async command(method, path, body, timeoutMs = 30_000) {
    const response = await fetch(`${BASE_URL}/session/${this.sessionId}${path}`, {
      method,
      headers: body === undefined ? undefined : { "content-type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
      signal: AbortSignal.timeout(timeoutMs),
    });
    const payload = await readPayload(response);
    if (!response.ok) throw failure(response, payload, path);
    return payload.value;
  }

  async find(selector) {
    const value = await this.command("POST", "/element", {
      using: "css selector",
      value: selector,
    });
    const id = value?.[ELEMENT_KEY];
    if (!id) throw new Error(`webdriver: element response for ${selector} had no reference`);
    return id;
  }

  async findOrNull(selector) {
    try {
      return await this.find(selector);
    } catch (error) {
      if (
        error instanceof WebDriverError &&
        (error.error === "no such element" || error.error === "stale element reference")
      ) {
        return null;
      }
      throw error;
    }
  }

  waitForElement(selector, timeoutMs = 20_000) {
    return waitFor(`element ${selector}`, () => this.findOrNull(selector), { timeoutMs });
  }

  async click(elementId) {
    await this.command("POST", `/element/${elementId}/click`, {});
  }

  async clear(elementId) {
    await this.command("POST", `/element/${elementId}/clear`, {});
  }

  async sendKeys(elementId, text) {
    try {
      await this.command("POST", `/element/${elementId}/value`, { text });
    } catch (error) {
      // W3C says `text`, the legacy dialect wants an array; WebKitWebDriver
      // versions in the wild accept different halves of that pair.
      if (!(error instanceof WebDriverError) || error.error !== "invalid argument") throw error;
      await this.command("POST", `/element/${elementId}/value`, { value: [...text] });
    }
  }

  execute(script, args = []) {
    return this.command("POST", "/execute/sync", { script, args });
  }

  async source() {
    return this.command("GET", "/source");
  }

  async screenshot() {
    return this.command("GET", "/screenshot");
  }

  async quit() {
    await this.command("DELETE", "", undefined, 10_000);
  }
}

export { ELEMENT_KEY };
