import { useTranslation } from "react-i18next";
import { TRANSFER_TAB, useAppStore } from "../store";
import { FilesIcon, ListIcon, LogIcon, SparkleIcon } from "./icons";
import { RailButton } from "./ui";

/**
 * Right tool strip: file transfer on top, then one toggle per companion panel.
 * The dot on the transfer slot is driven by real queued jobs, not by a
 * decorative state.
 */
export function RightStrip() {
  const { t } = useTranslation();
  const commandsOpen = useAppStore((s) => s.commandsOpen);
  const toggleCommands = useAppStore((s) => s.toggleCommands);
  const execLogOpen = useAppStore((s) => s.execLogOpen);
  const toggleExecLog = useAppStore((s) => s.toggleExecLog);
  const aiOpen = useAppStore((s) => s.aiOpen);
  const toggleAi = useAppStore((s) => s.toggleAi);
  const activeId = useAppStore((s) => s.activeId);
  const openTransfer = useAppStore((s) => s.openTransfer);
  const closeTransfer = useAppStore((s) => s.closeTransfer);
  const transfers = useAppStore((s) => s.transfers);

  const transferActive = activeId === TRANSFER_TAB;
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
          label={t("transfer.open")}
          active={transferActive}
          badge={busy}
          badgeLabel={t("nav.activeTask")}
          onClick={() => (transferActive ? closeTransfer() : openTransfer())}
        >
          <FilesIcon size={18} />
        </RailButton>
      </div>

      <div className="strip-divider" />

      <div className="strip-group">
        <RailButton
          label={t("commands.title")}
          active={commandsOpen}
          onClick={toggleCommands}
        >
          <ListIcon size={18} />
        </RailButton>
        <RailButton
          label={t("log.toggle")}
          active={execLogOpen}
          onClick={toggleExecLog}
        >
          <LogIcon size={18} />
        </RailButton>
        <RailButton
          label={t("ai.toggle")}
          active={aiOpen}
          onClick={toggleAi}
        >
          <SparkleIcon size={18} />
        </RailButton>
      </div>
    </nav>
  );
}
