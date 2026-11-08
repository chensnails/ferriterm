import { useTranslation } from "react-i18next";
import { isTauri } from "../api";

export function BrowserBanner() {
  const { t } = useTranslation();
  if (isTauri) return null;
  return (
    <div id="browser-banner">
      {t("browser.banner", { command: "npm run tauri:dev" })}
    </div>
  );
}