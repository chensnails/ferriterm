/**
 * Development build label shown in the About page:
 * YYYYMMDD-dev<baseline>+<short HEAD>, for example 20261109-dev1.0.0+eb4e626.
 *
 * The dev baseline is a display-only label chosen by the product owner; it is
 * intentionally independent from the packaged version in package.json /
 * src-tauri/tauri.conf.json. Vite injects the build date and the HEAD sampled
 * at config evaluation time, with fallbacks so dev servers started before the
 * config change (or builds without a git checkout) still show a valid label.
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
  ].join("");
})();

const buildStamp =
  typeof __BUILD_STAMP__ === "string" ? __BUILD_STAMP__ : localStamp;

const buildHead =
  typeof __BUILD_HEAD__ === "string" && __BUILD_HEAD__
    ? __BUILD_HEAD__
    : "nogit";

export const DEV_VERSION = `${buildStamp}-dev${DEV_BASELINE}+${buildHead}`;
