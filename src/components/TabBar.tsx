import { useTranslation } from "react-i18next";
import { useAppStore } from "../store";
import { terminalView } from "../terminal";
import { ListIcon, BroadcastIcon, SplitIcon, FilesIcon } from "./icons";

export function TabBar() {
  const { t } = useTranslation();
  const sessions = useAppStore((s) => s.sessions);
  const activeId = useAppStore((s) => s.activeId);
  const activate = useAppStore((s) => s.activate);
  const closeSession = useAppStore((s) => s.closeSession);
  const startRecording = useAppStore((s) => s.startRecording);
  const stopRecording = useAppStore((s) => s.stopRecording);
  const reconnect = useAppStore((s) => s.reconnect);
  const splitId = useAppStore((s) => s.splitId);
  const splitActive = useAppStore((s) => s.splitActive);
  const closeSplit = useAppStore((s) => s.closeSplit);
  const commandsOpen = useAppStore((s) => s.commandsOpen);
  const toggleCommands = useAppStore((s) => s.toggleCommands);
  const broadcastTargets = useAppStore((s) => s.broadcastTargets);
  const requestBroadcast = useAppStore((s) => s.requestBroadcast);
  const stopBroadcast = useAppStore((s) => s.stopBroadcast);
  const openTransfer = useAppStore((s) => s.openTransfer);

  if (sessions.length === 0) return null;

  return (
    <div id="tabbar">
      {sessions.map((s) => {
        const cls = ["tab"];
        if (s.id === activeId) cls.push("active");
        if (s.closed) cls.push("closed");
        return (
          <div
            key={s.id}
            className={cls.join(" ")}
            draggable
            onDragStart={(e) => {
              e.dataTransfer.setData("text/ferriterm-session", s.id);
              e.dataTransfer.effectAllowed = "move";
            }}
            onClick={() => activate(s.id)}
          >
            {s.production && (
              <span className="prod-dot" title={t("hosts.production")} />
            )}
            <span className="tab-label">{s.title}</span>
            {s.closed && s.lastConnect && (
              <button
                className="tab-reconnect"
                title={t("sessions.reconnect")}
                aria-label={t("sessions.reconnect")}
                onClick={(e) => {
                  e.stopPropagation();
                  reconnect(s.id);
                }}
              >
                ↻
              </button>
            )}
            <button
              className={s.recording ? "tab-record active" : "tab-record"}
              title={s.recording ? t("sessions.stopRecord") : t("sessions.record")}
              aria-label={s.recording ? t("sessions.stopRecord") : t("sessions.record")}
              aria-pressed={!!s.recording}
              onClick={(e) => {
                e.stopPropagation();
                if (s.recording) {
                  void stopRecording(s.id);
                } else {
                  const view = terminalView(s.id);
                  void startRecording(
                    s.id,
                    view?.term.cols ?? 80,
                    view?.term.rows ?? 24,
                  );
                }
              }}
            >
              ●
            </button>
            <button
              className="tab-close"
              title={t("sessions.closeTab")}
              onClick={(e) => {
                e.stopPropagation();
                closeSession(s.id);
              }}
            >
              ×
            </button>
          </div>
        );
      })}
      <button
        className="icon-btn tab-split"
        title={splitId ? t("split.close") : t("split.start")}
        aria-label={splitId ? t("split.close") : t("split.start")}
        aria-pressed={!!splitId}
        onClick={() => (splitId ? closeSplit() : splitActive())}
      >
        <SplitIcon />
      </button>
      <button
        className={broadcastTargets.length > 0 ? "icon-btn active" : "icon-btn"}
        title={
          broadcastTargets.length > 0
            ? t("broadcast.stop")
            : t("broadcast.toggle")
        }
        aria-label={
          broadcastTargets.length > 0
            ? t("broadcast.stop")
            : t("broadcast.toggle")
        }
        aria-pressed={broadcastTargets.length > 0}
        onClick={() =>
          broadcastTargets.length > 0 ? stopBroadcast() : requestBroadcast()
        }
      >
        <BroadcastIcon />
      </button>
      <button
        className="icon-btn tab-files"
        title={t("transfer.open")}
        aria-label={t("transfer.open")}
        onClick={() => openTransfer(activeId)}
      >
        <FilesIcon />
      </button>
      <button
        className="icon-btn tab-commands"
        title={t("commands.toggle")}
        aria-label={t("commands.toggle")}
        aria-pressed={commandsOpen}
        onClick={toggleCommands}
      >
        <ListIcon />
      </button>
    </div>
  );
}