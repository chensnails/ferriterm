import { useTranslation } from "react-i18next";
import { isTauri } from "../api";
import { useAppStore } from "../store";
import { HostTree } from "./HostTree";
import { FolderPlusIcon, PlusIcon, SearchIcon } from "./icons";

export function HomeView() {
  const { t } = useTranslation();
  const hostFilter = useAppStore((s) => s.hostFilter);
  const setHostFilter = useAppStore((s) => s.setHostFilter);
  const requestNewHost = useAppStore((s) => s.requestNewHost);
  const requestNewFolder = useAppStore((s) => s.requestNewFolder);

  return (
    <div className="home-view">
      <div className="home-inner">
        <div className="home-logo">
          <span className="home-logo-mark" aria-hidden="true">
            &gt;_
          </span>
          <span className="home-logo-name">{t("app.name")}</span>
        </div>
        <div className="home-search">
          <label className="search-field home-search-field">
            <SearchIcon />
            <input
              value={hostFilter}
              onChange={(e) => setHostFilter(e.target.value)}
              placeholder={t("hosts.searchPlaceholder")}
              aria-label={t("hosts.searchPlaceholder")}
              autoComplete="off"
              spellCheck={false}
            />
          </label>
          <button
            className="icon-btn"
            title={t("hosts.newHost")}
            aria-label={t("hosts.newHost")}
            disabled={!isTauri}
            onClick={requestNewHost}
          >
            <PlusIcon />
          </button>
          <button
            className="icon-btn"
            title={t("hosts.newFolder")}
            aria-label={t("hosts.newFolder")}
            disabled={!isTauri}
            onClick={requestNewFolder}
          >
            <FolderPlusIcon />
          </button>
        </div>
        <HostTree chromeless />
      </div>
    </div>
  );
}