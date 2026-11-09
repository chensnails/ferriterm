import { useTranslation } from "react-i18next";
import { TRANSFER_TAB, useAppStore } from "../store";
import { FilesIcon, ListIcon, LogIcon, PulseIcon } from "./icons";
import { RailButton } from "./ui";

/**
 * Right tool strip: one toggle per existing companion panel. The dot on the
 * transfer slot is driven by real queued jobs, not by a decorative state.
 */
export function RightStrip() {
  const { t } = useTranslation();
  const commandsOpen = useAppStore((s) => s.commandsOpen);
  const toggleCommands = useAppStore((s) => s.toggleCommands);
  const monitorOpen = useAppStore((s) => s.monitorOpen);
  const toggleMonitor = useAppStore((s) => s.toggleMonitor);
  const execLogOpen = useAppStore((s) => s.execLogOpen);
  const toggleExecLog = useAppStore((s) => s.toggleExecLog);
  const activeId = useAppStore((s) => s.activeId);
  const openTransfer = useAppStore((s) => s.openTransfer);
  const transfers = useAppStore((s) => s.transfers);

  const busy = transfers.some(
    (job) =>
      job.status === "running" ||
      job.status === "pending" ||
      job.status === "paused",
  );

  return (
    <nav className="tool-strip" aria-label={t("nav.panels")}>
      <div className="strip-group">
        <RailButton
          label={t("commands.title")}
          active={commandsOpen}
          onClick={toggleCommands}
        >
          <ListIcon size={18} />
        </RailButton>
        <RailButton
          label={t("monitor.open")}
          active={monitorOpen}
          onClick={toggleMonitor}
        >
          <PulseIcon size={18} />
        </RailButton>
        <RailButton
          label={t("log.toggle")}
          active={execLogOpen}
          onClick={toggleExecLog}
        >
          <LogIcon size={18} />
        </RailButton>
      </div>

      <div className="strip-divider" />

      <div className="strip-group">
        <RailButton
          label={t("nav.queue")}
          active={activeId === TRANSFER_TAB}
          badge={busy}
          badgeLabel={t("nav.activeTask")}
          onClick={() => openTransfer()}
        >
          <FilesIcon size={18} />
        </RailButton>
      </div>
    </nav>
  );
}
