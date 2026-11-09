import { useTranslation } from "react-i18next";
import { HOME_TAB, useAppStore } from "../store";
import { HomeIcon, PuzzleIcon } from "./icons";
import { RailButton } from "./ui";

/**
 * Left activity rail. It only carries the two destinations that exist today —
 * the host list and the reserved tool list — because the transfer view and the
 * companion panels now live on the right tool strip, and settings sits in the
 * tab bar's right-hand toolbar.
 */
export function ActivityRail() {
  const { t } = useTranslation();
  const activeId = useAppStore((s) => s.activeId);
  const goHome = useAppStore((s) => s.goHome);
  const sidebarOpen = useAppStore((s) => s.sidebarOpen);
  const toggleSidebar = useAppStore((s) => s.toggleSidebar);

  const onHome = activeId === HOME_TAB;

  // First click always lands on the host list; clicking it again while the
  // list is already in front collapses it, so one slot covers both jobs.
  const showHosts = () => {
    if (onHome && sidebarOpen) {
      toggleSidebar();
      return;
    }
    if (!onHome) goHome();
    if (!sidebarOpen) toggleSidebar();
  };

  return (
    <nav className="activity-rail" aria-label={t("nav.modules")}>
      <div className="rail-group">
        <RailButton
          label={sidebarOpen ? t("nav.hosts") : t("nav.showHosts")}
          pressed={sidebarOpen}
          active={sidebarOpen}
          onClick={showHosts}
        >
          <HomeIcon size={18} />
        </RailButton>
        <RailButton
          label={t("nav.toolsReserved")}
          disabled
          onClick={() => undefined}
        >
          <PuzzleIcon size={18} />
        </RailButton>
      </div>
    </nav>
  );
}
