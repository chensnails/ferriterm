import { useTranslation } from "react-i18next";
import { HOME_TAB, useAppStore } from "../store";
import { GlobeIcon, HomeIcon } from "./icons";
import { RailButton } from "./ui";

/**
 * Left activity rail. Each slot is a module: it swaps what the sidebar shows
 * (host list or the network toolbox) and returns the content area to the home
 * view, which hosts the selected module's page.
 */
export function ActivityRail() {
  const { t } = useTranslation();
  const activeId = useAppStore((s) => s.activeId);
  const sidebarOpen = useAppStore((s) => s.sidebarOpen);
  const toggleSidebar = useAppStore((s) => s.toggleSidebar);
  const sidebarView = useAppStore((s) => s.sidebarView);
  const showSidebarView = useAppStore((s) => s.showSidebarView);

  const onHome = activeId === HOME_TAB;
  const current = onHome && sidebarOpen ? sidebarView : null;

  // Clicking the active module again collapses the sidebar, so one slot covers
  // both "show me" and "get out of the way".
  const activate = (view: "hosts" | "tools") => {
    if (current === view) {
      toggleSidebar();
      return;
    }
    showSidebarView(view);
  };

  return (
    <nav className="activity-rail" aria-label={t("nav.modules")}>
      <div className="rail-group">
        <RailButton
          label={current === "hosts" ? t("nav.hosts") : t("nav.showHosts")}
          pressed={current === "hosts"}
          active={current === "hosts"}
          onClick={() => activate("hosts")}
        >
          <HomeIcon size={18} />
        </RailButton>
        <RailButton
          label={current === "tools" ? t("tools.title") : t("nav.showTools")}
          pressed={current === "tools"}
          active={current === "tools"}
          onClick={() => activate("tools")}
        >
          <GlobeIcon size={18} />
        </RailButton>
      </div>
    </nav>
  );
}
