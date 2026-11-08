import { useTranslation } from "react-i18next";
import { isTauri } from "../api";
import { useAppStore } from "../store";
import { FolderPlusIcon, PlusIcon, TerminalIcon } from "./icons";

export function HomeView() {
  const { t } = useTranslation();
  const requestNewHost = useAppStore((s) => s.requestNewHost);
  const requestNewFolder = useAppStore((s) => s.requestNewFolder);
  const setQuickConnectOpen = useAppStore((s) => s.setQuickConnectOpen);

  return (
    <div className="home-view">
      <div className="home-inner">
        <div className="home-logo">
          <span className="home-logo-mark" aria-hidden="true">
            &gt;_
          </span>
          <span className="home-logo-name">{t("app.name")}</span>
        </div>
        <p className="home-tagline">{t("home.tagline")}</p>
        <div className="home-actions">
          <button
            className="home-action"
            onClick={() => setQuickConnectOpen(true)}
          >
            <TerminalIcon />
            {t("connect.quick")}
          </button>
          <button
            className="home-action"
            disabled={!isTauri}
            onClick={requestNewHost}
          >
            <PlusIcon />
            {t("hosts.newHost")}
          </button>
          <button
            className="home-action"
            disabled={!isTauri}
            onClick={requestNewFolder}
          >
            <FolderPlusIcon />
            {t("hosts.newFolder")}
          </button>
        </div>
      </div>
    </div>
  );
}