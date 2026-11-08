import { lazy, Suspense } from "react";
import { useTranslation } from "react-i18next";
import { useAppStore } from "../store";
import { ConnectForm } from "./ConnectForm";
import { HostTree } from "./HostTree";
import { SessionList } from "./SessionList";
import { ForwardIcon, GearIcon, PulseIcon } from "./icons";

const SettingsDialog = lazy(() =>
  import("./SettingsDialog").then((module) => ({
    default: module.SettingsDialog,
  })),
);

export function Sidebar() {
  const { t } = useTranslation();
  const language = useAppStore((s) => s.language);
  const toggleTheme = useAppStore((s) => s.toggleTheme);
  const setLanguage = useAppStore((s) => s.setLanguage);
  const toggleMonitor = useAppStore((s) => s.toggleMonitor);
  const toggleForward = useAppStore((s) => s.toggleForward);
  const settingsOpen = useAppStore((s) => s.settingsOpen);
  const setSettingsOpen = useAppStore((s) => s.setSettingsOpen);

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
        <button
          className="icon-btn"
          title={t("monitor.open")}
          aria-label={t("monitor.open")}
          onClick={toggleMonitor}
        >
          <PulseIcon />
        </button>
        <button
          className="icon-btn"
          title={t("forward.open")}
          aria-label={t("forward.open")}
          onClick={toggleForward}
        >
          <ForwardIcon />
        </button>
        <button
          className="icon-btn"
          title={t("settings.open")}
          aria-label={t("settings.open")}
          onClick={() => setSettingsOpen(true)}
        >
          <GearIcon />
        </button>
      </header>
      <HostTree />
      <details id="quick-connect">
        <summary>{t("connect.quick")}</summary>
        <ConnectForm />
      </details>
      <SessionList />
      {settingsOpen && (
        <Suspense fallback={null}>
          <SettingsDialog onClose={() => setSettingsOpen(false)} />
        </Suspense>
      )}
    </aside>
  );
}