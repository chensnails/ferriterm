// End-to-end smoke test through tauri-driver: create a host in the UI, connect
// to the loopback SSH fixture, trust its host key and run a command whose
// output only the server can produce.
//
// Linux/Windows only (tauri-driver has no macOS backend). Build the frontend
// with VITE_E2E=1 so the terminal renders its accessibility tree, start the
// fixture first, then run this script under a display (xvfb-run on CI).

import { mkdir, mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { openSession, startTauriDriver, waitFor } from "./webdriver.mjs";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const binary = resolve(
  process.env.FERRITERM_E2E_BINARY || join(repoRoot, "target/debug/ferriterm"),
);
const artifacts = resolve(
  process.env.FERRITERM_E2E_ARTIFACTS || join(repoRoot, "e2e-artifacts"),
);

const HOST_NAME = "E2E Loopback";
const HOST = "127.0.0.1";
const PORT = "2222";
const USER = "test";
const PASSWORD = "ferriterm-test";
// Printed only by the fixture, so seeing it proves the command reached the
// server; the client-side echo of the typed bytes cannot fake it.
const SERVER_PROOF = "Ferriterm-TestShell loopback 0.1";

const log = (message) => console.log(`e2e: ${message}`);

async function saveScreenshot(session, name) {
  try {
    const png = await session.screenshot();
    await writeFile(join(artifacts, name), Buffer.from(png, "base64"));
    log(`saved ${name}`);
  } catch (error) {
    // A screenshot is evidence, not a gate; WebKitGTK builds differ here.
    log(`screenshot unavailable: ${error.message}`);
  }
}

async function recordFailure(session) {
  if (!session) return;
  await saveScreenshot(session, "failure.png");
  try {
    await writeFile(join(artifacts, "failure.html"), await session.source());
  } catch {
    // page source is a diagnostic, not a requirement
  }
}

async function run(session) {
  log("waiting for the home view");
  const newHost = await session.waitForElement('[data-testid="home-new-host"]', 60_000);
  await session.click(newHost);

  log("creating the host through the dialog");
  await session.sendKeys(await session.waitForElement('[data-testid="host-name"]'), HOST_NAME);
  await session.sendKeys(await session.waitForElement('[data-testid="host-address"]'), HOST);
  const port = await session.waitForElement('[data-testid="host-port"]');
  await session.clear(port);
  await session.sendKeys(port, PORT);
  await session.sendKeys(await session.waitForElement('[data-testid="host-username"]'), USER);
  await session.click(await session.waitForElement('[data-testid="host-save"]'));

  const row = await session.waitForElement(
    `[data-testid="host-row"][data-host-name="${HOST_NAME}"]`,
    20_000,
  );
  // Selecting the row reveals its action strip, which is transparent until
  // hover/focus/selection.
  await session.click(row);

  log("connecting to the loopback fixture");
  await session.click(
    await session.waitForElement('[data-testid="host-row"] [data-testid="host-connect"]'),
  );
  await session.sendKeys(
    await session.waitForElement('[data-testid="connect-password"]', 20_000),
    PASSWORD,
  );
  await session.click(await session.waitForElement('[data-testid="connect-submit"]'));

  log("trusting the fixture host key");
  await session.click(await session.waitForElement('[data-testid="hostkey-accept"]', 30_000));

  log("typing into the terminal");
  const textarea = await session.waitForElement(".xterm-helper-textarea", 20_000);
  await session.sendKeys(textarea, "uname");
  await session.sendKeys(textarea, "\uE007");

  const screen = await waitFor(
    "the fixture's uname output",
    async () => {
      const text = await session.execute(
        "const tree = document.querySelector('.xterm-accessibility-tree');" +
          "return tree ? tree.textContent : '';",
      );
      return text.includes(SERVER_PROOF) ? text : null;
    },
    { timeoutMs: 30_000 },
  );
  log(`terminal shows the fixture output (${screen.length} chars in the a11y tree)`);
}

async function main() {
  await mkdir(artifacts, { recursive: true });
  // Keep every run independent: a reused data dir would restore tabs and the
  // trusted host key of the previous run, skipping half the flow.
  const dataRoot = await mkdtemp(join(tmpdir(), "ferriterm-e2e-"));
  const dirs = {
    home: join(dataRoot, "home"),
    data: join(dataRoot, "data"),
    config: join(dataRoot, "config"),
    cache: join(dataRoot, "cache"),
  };
  for (const dir of Object.values(dirs)) await mkdir(dir, { recursive: true });

  const driver = await startTauriDriver({
    env: {
      ...process.env,
      TAURI_WEBVIEW_AUTOMATION: "true",
      HOME: dirs.home,
      XDG_DATA_HOME: dirs.data,
      XDG_CONFIG_HOME: dirs.config,
      XDG_CACHE_HOME: dirs.cache,
    },
  });
  let session;
  let failure;
  try {
    session = await openSession(binary);
    log(`session ${session.sessionId} opened for ${binary}`);
    await run(session);
    await saveScreenshot(session, "terminal.png");
    log("ok");
  } catch (error) {
    failure = error;
    await recordFailure(session);
  } finally {
    if (session) await session.quit().catch(() => {});
    await driver.stop();
  }
  if (failure) throw failure;
}

main().catch((error) => {
  console.error(`e2e: FAILED: ${error.message}`);
  process.exitCode = 1;
});
