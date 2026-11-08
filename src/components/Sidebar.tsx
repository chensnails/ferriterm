import { useTranslation } from "react-i18next";
import { useAppStore } from "../store";
import { ConnectForm } from "./ConnectForm";
import { SessionList } from "./SessionList";

export function Sidebar() {
  const { t } = useTranslation();
  const language = useAppStore((s) => s.language);
  const toggleTheme = useAppStore((s) => s.toggleTheme);
  const setLanguage = useAppStore((s) => s.setLanguage);

  return (
    <aside id="sidebar">
      <header className="brand">
        <span className="brand-mark">&gt;_</span>
        <span className="brand-name">{t("app.name")}</span>
        <button
          className="icon-btn"
          title={t("language.toggle")}
          onClick={() => setLanguage(language === "en" ? "zh-CN" : "en")}
        >
          {language === "en" ? "EN" : "中"}
        </button>
        <button className="icon-btn" title={t("theme.toggle")} onClick={toggleTheme}>
          ◐
        </button>
      </header>
      <ConnectForm />
      <SessionList />
    </aside>
  );
}