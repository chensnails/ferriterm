/**
 * Development build labels shown in the About page:
 * YYYYMMDDHHmmss+<baseline>+<short HEAD>, e.g. 20261110110835+1.0.0+eb4e626.
 *
 * The dev baseline is a display-only label chosen by the product owner; it is
 * intentionally independent from the packaged version in package.json /
 * src-tauri/tauri.conf.json. Vite injects the build timestamp, the HEAD and
 * the recent commits sampled at config evaluation time, with fallbacks so dev
 * servers started before the config change (or builds without a git checkout)
 * still show a valid label and an honest empty history.
 * A commit cannot embed its own hash, so the suffix always refers to the HEAD
 * at build / dev-server start; restart after a commit to refresh it.
 */
const DEV_BASELINE = "1.0.0";

const localStamp = (() => {
  const now = new Date();
  return [
    now.getFullYear(),
    String(now.getMonth() + 1).padStart(2, "0"),
    String(now.getDate()).padStart(2, "0"),
    String(now.getHours()).padStart(2, "0"),
    String(now.getMinutes()).padStart(2, "0"),
    String(now.getSeconds()).padStart(2, "0"),
  ].join("");
})();

const buildStamp =
  typeof __BUILD_STAMP__ === "string" ? __BUILD_STAMP__ : localStamp;

const buildHead =
  typeof __BUILD_HEAD__ === "string" && __BUILD_HEAD__
    ? __BUILD_HEAD__
    : "nogit";

export interface VersionRecord {
  hash: string;
  date: string;
  subject: string;
  label: string;
}

// Read the injected snapshot once: a typeof guard would repeat the whole
// array literal three times in the bundle. The try/catch also covers dev
// servers started before the build-info config existed.
const readInjectedHistory = (): Omit<VersionRecord, "label">[] => {
  try {
    const value: unknown = __BUILD_HISTORY__;
    return Array.isArray(value)
      ? (value as Omit<VersionRecord, "label">[])
      : [];
  } catch {
    return [];
  }
};

export const BUILD_HEAD = buildHead;

// Commit dates arrive as ISO strings (YYYY-MM-DDTHH:mm:ss+ZZ:zz). Stripping
// everything after the seconds keeps the author's own timezone, matching how
// `git log --pretty=format:%aI` reports it.
function compactStamp(iso: string): string {
  const digits = iso.slice(0, 19).replace(/\D/g, "");
  return digits.length === 14 ? digits : buildStamp;
}

export const BUILD_HISTORY: VersionRecord[] = readInjectedHistory().map((entry) => ({
  hash: entry.hash,
  date: entry.date,
  subject: entry.subject,
  label: `${compactStamp(entry.date)}+${DEV_BASELINE}+${entry.hash}`,
}));

export const DEV_VERSION = `${buildStamp}+${DEV_BASELINE}+${buildHead}`;
