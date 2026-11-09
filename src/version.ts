/**
 * Development build label shown in the About page: YYYYMMDD-dev<baseline>.
 * The dev baseline is a display-only label chosen by the product owner; it is
 * intentionally independent from the packaged version in package.json /
 * src-tauri/tauri.conf.json. Vite injects the build date at build time, with a
 * local-date fallback so dev servers started before the config change still
 * show a valid label.
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

export const DEV_VERSION = `${buildStamp}-dev${DEV_BASELINE}`;
