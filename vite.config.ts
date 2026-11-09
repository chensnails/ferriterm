import { execFileSync } from "node:child_process";
import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";

const now = new Date();
const buildStamp = [
  now.getFullYear(),
  String(now.getMonth() + 1).padStart(2, "0"),
  String(now.getDate()).padStart(2, "0"),
].join("");

// A commit cannot contain its own hash, so the HEAD and the recent history are
// sampled once whenever the Vite config is evaluated (build / dev server
// start). Restarting the dev server after a commit picks up the new snapshot.
interface BuildCommit {
  hash: string;
  date: string;
  subject: string;
}

const readGit = (args: string[]): string =>
  execFileSync("git", args, {
    cwd: process.cwd(),
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
  }).trim();

const readBuildHead = (): string => {
  try {
    const head = readGit(["rev-parse", "--short=7", "HEAD"]);
    return /^[0-9a-f]{7,40}$/i.test(head) ? head : "nogit";
  } catch {
    return "nogit";
  }
};

// Control characters keep commit subjects containing separators parseable.
const readBuildHistory = (): BuildCommit[] => {
  try {
    const raw = readGit([
      "log",
      "-n",
      "30",
      "--pretty=format:%h%x1f%aI%x1f%s%x1e",
    ]);
    return raw
      .split("\x1e")
      .map((record) => record.trim())
      .filter(Boolean)
      .map((record) => {
        const [hash = "", date = "", subject = ""] = record.split("\x1f");
        return { hash, date, subject };
      })
      .filter((entry) => /^[0-9a-f]{7,40}$/i.test(entry.hash));
  } catch {
    return [];
  }
};

const buildHead = readBuildHead();
const buildHistory = readBuildHistory();

// Vite only applies `define` statically during production builds; in dev the
// identifiers stay untouched. Replacing them in a pre-transform keeps the dev
// window and the packaged build on the same snapshot, while `define` remains
// as a fallback for bundlers that skip this hook. The typeof guards in
// version.ts keep dev servers started with an older config safe.
const replacements: Record<string, string> = {
  __BUILD_STAMP__: JSON.stringify(buildStamp),
  __BUILD_HEAD__: JSON.stringify(buildHead),
  __BUILD_HISTORY__: JSON.stringify(buildHistory),
};

const buildInfoPlugin = (): Plugin => ({
  name: "ferriterm-build-info",
  enforce: "pre",
  transform(code, id) {
    if (!id.replace(/\\/g, "/").endsWith("/src/version.ts")) return null;
    return {
      code: code.replace(
        /__BUILD_(?:STAMP|HEAD|HISTORY)__/g,
        (key) => replacements[key] ?? key,
      ),
      map: null,
    };
  },
});

export default defineConfig({
  define: {
    __BUILD_STAMP__: JSON.stringify(buildStamp),
    __BUILD_HEAD__: JSON.stringify(buildHead),
    __BUILD_HISTORY__: JSON.stringify(buildHistory),
  },
  plugins: [buildInfoPlugin(), react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    target: "es2021",
    minify: "esbuild",
    sourcemap: false,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (!id.includes("node_modules")) return;
          if (id.includes("/@xterm/")) return "xterm";
          if (id.includes("/@tauri-apps/")) return "tauri";
          if (
            id.includes("/react/") ||
            id.includes("/react-dom/") ||
            id.includes("/scheduler/")
          ) {
            return "react";
          }
          if (
            id.includes("/i18next/") ||
            id.includes("/i18next-icu/") ||
            id.includes("/react-i18next/") ||
            id.includes("/intl-messageformat/") ||
            id.includes("/@formatjs/")
          ) {
            return "i18n";
          }
          return "vendor";
        },
      },
    },
  },
});