import { useState } from "react";
import { useTranslation } from "react-i18next";
import { formatBytes } from "../format";
import { TRANSFER_TAB, useAppStore } from "../store";
import { ContextMenu } from "./ContextMenu";
import { FilesIcon, LogIcon, SplitIcon } from "./icons";
import { StatusMonitor } from "./StatusMonitor";

/**
 * Bottom status bar. Left cell mirrors where you are, the middle cell carries
 * the last message, the right cell mirrors the transfer queue and the log.
 * Every cell is also the shortcut into the panel that owns it, so the three
 * columns and the dock stay in step instead of being separate islands.
 */
export function StatusBar() {
  const { t } = useTranslation();
  const sessions = useAppStore((s) => s.sessions);
  const activeId = useAppStore((s) => s.activeId);
  const splitId = useAppStore((s) => s.splitId);
  const status = useAppStore((s) => s.status);
  const transfers = useAppStore((s) => s.transfers);
  const sidebarView = useAppStore((s) => s.sidebarView);
  const activate = useAppStore((s) => s.activate);
  const openTransfer = useAppStore((s) => s.openTransfer);
  const execLogOpen = useAppStore((s) => s.execLogOpen);
  const toggleExecLog = useAppStore((s) => s.toggleExecLog);
  const clearStatus = useAppStore((s) => s.clearStatus);
  const statusMonitor = useAppStore((s) => s.statusMonitor);
  const setStatusMonitor = useAppStore((s) => s.setStatusMonitor);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);

  const open = sessions.filter((session) => !session.closed);
  const current = sessions.find((session) => session.id === activeId);
  const running = transfers.filter((job) => job.status === "running");
  const queued = transfers.filter(
    (job) => job.status === "pending" || job.status === "paused",
  );
  const speed = running.reduce((total, job) => total + job.speed, 0);

  const label = current
    ? current.title
    : activeId === TRANSFER_TAB
      ? t("transfer.title")
      : sidebarView === "tools"
        ? t("status.tools")
        : t("status.home");

  const tone = current ? (current.closed ? "closed" : "live") : "idle";

  // The left cell doubles as "back to the terminal": from home or the transfer
  // view it returns to the last open session, which is the one that was active.
  const focusTerminal = () => {
    if (current) return;
    const last = open[open.length - 1];
    if (last) activate(last.id);
  };

  return (
    <footer
      id="statusbar"
      // The bar itself is the way back once the readout is switched off, so
      // hiding it never traps the setting.
      onContextMenu={(event) => {
        event.preventDefault();
        setMenu({ x: event.clientX, y: event.clientY });
      }}
    >
      <div className="statusbar-group">
        <button
          type="button"
          className={`statusbar-session ${tone}`}
          onClick={focusTerminal}
          title={t("status.focusTerminal")}
        >
          <span className="statusbar-dot" aria-hidden="true" />
          <span className="statusbar-title">{label}</span>
          {current?.production && (
            <span className="statusbar-flag">{t("status.production")}</span>
          )}
        </button>
        <span className="statusbar-meta">
          {t("status.sessions", { count: open.length })}
        </span>
        {splitId && (
          <span className="statusbar-meta">
            <SplitIcon size={12} />
            {t("status.split")}
          </span>
        )}
      </div>

      <button
        type="button"
        className={`statusbar-message ${status.isError ? "error" : ""}`}
        title={status.text}
        onClick={clearStatus}
      >
        {status.text}
      </button>

      <div className="statusbar-group end">
        <StatusMonitor />
        <button
          type="button"
          className={`statusbar-item ${running.length > 0 ? "busy" : ""}`}
          onClick={() => openTransfer()}
          title={t("transfer.open")}
        >
          <FilesIcon size={13} />
          {running.length > 0 ? (
            <>
              <span className="statusbar-item-label">
                {t("transfer.statActive", { n: running.length })}
              </span>
              {speed > 0 && (
                <span className="statusbar-mono">
                  {formatBytes(speed)}
                  {t("transfer.perSecond")}
                </span>
              )}
            </>
          ) : (
            <span className="statusbar-item-label">
              {queued.length > 0
                ? t("transfer.statWaiting", { n: queued.length })
                : t("status.transferIdle")}
            </span>
          )}
        </button>
        <button
          type="button"
          className={`statusbar-item ${execLogOpen ? "active" : ""}`}
          onClick={toggleExecLog}
          title={t("log.toggle")}
        >
          <LogIcon size={13} />
          <span className="statusbar-item-label">{t("log.title")}</span>
        </button>
      </div>

      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          onClose={() => setMenu(null)}
          items={[
            {
              label: t("status.monitorToggle"),
              checked: statusMonitor,
              onSelect: () => setStatusMonitor(!statusMonitor),
            },
          ]}
        />
      )}
    </footer>
  );
}
