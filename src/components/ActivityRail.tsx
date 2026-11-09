import { useTranslation } from "react-i18next";
import { useAppStore } from "../store";
import { FilesIcon, GearIcon, SidebarIcon, TerminalIcon } from "./icons";
import { RailButton } from "./ui";

/**
 * Left activity rail: module selection on top, tool-island toggle and global
 * actions below. Modules map onto views that already exist in the app, so the
 * rail never exposes an entry point without a working destination.
 */
export function ActivityRail() {
  const { t } = useTranslation();
  const activeModule = useAppStore((s) => s.activeModule);
  const setActiveModule = useAppStore((s) => s.setActiveModule);
  const sidebarOpen = useAppStore((s) => s.sidebarOpen);
  const toggleSidebar = useAppStore((s) => s.toggleSidebar);
  const setSettingsOpen = useAppStore((s) => s.setSettingsOpen);

  return (
    <nav className="activity-rail" aria-label={t("nav.modules")}>
      <div className="rail-group">
        <RailButton
          label={t("nav.terminal")}
          active={activeModule === "terminal"}
          onClick={() => setActiveModule("terminal")}
        >
          <TerminalIcon size={18} />
        </RailButton>
        <RailButton
          label={t("transfer.open")}
          active={activeModule === "transfer"}
          onClick={() => setActiveModule("transfer")}
        >
          <FilesIcon size={18} />
        </RailButton>
      </div>

      <div className="rail-divider" />

      <div className="rail-group">
        <RailButton
          label={sidebarOpen ? t("nav.collapse") : t("nav.expand")}
          pressed={sidebarOpen}
          active={sidebarOpen}
          onClick={toggleSidebar}
        >
          <SidebarIcon size={18} />
        </RailButton>
      </div>

      <div className="rail-spacer" />

      <div className="rail-group">
        <RailButton
          label={t("settings.open")}
          onClick={() => setSettingsOpen(true)}
        >
          <GearIcon size={18} />
        </RailButton>
      </div>
    </nav>
  );
}
