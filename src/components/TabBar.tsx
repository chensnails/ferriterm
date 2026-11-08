import { useState } from "react";
import { useTranslation } from "react-i18next";
import { isTauri } from "../api";
import { HOME_TAB, TRANSFER_TAB, useAppStore } from "../store";
import { terminalHandle } from "../terminal-registry";
import { ContextMenu, type MenuItem } from "./ContextMenu";
import {
  BroadcastIcon,
  FilesIcon,
  ForwardIcon,
  GearIcon,
  HomeIcon,
  ListIcon,
  LogIcon,
  PlusIcon,
  PulseIcon,
  SearchIcon,
  SplitIcon,
  TerminalIcon,
} from "./icons";

export function TabBar() {
  const { t } = useTranslation();
  const sessions = useAppStore((s) => s.sessions);
  const activeId = useAppStore((s) => s.activeId);
  const activate = useAppStore((s) => s.activate);
  const goHome = useAppStore((s) => s.goHome);
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
  const transferOpen = useAppStore((s) => s.transferOpen);
  const openTransfer = useAppStore((s) => s.openTransfer);
  const closeTransfer = useAppStore((s) => s.closeTransfer);
  const execLogOpen = useAppStore((s) => s.execLogOpen);
  const toggleExecLog = useAppStore((s) => s.toggleExecLog);
  const monitorOpen = useAppStore((s) => s.monitorOpen);
  const toggleMonitor = useAppStore((s) => s.toggleMonitor);
  const forwardOpen = useAppStore((s) => s.forwardOpen);
  const toggleForward = useAppStore((s) => s.toggleForward);
  const openSearch = useAppStore((s) => s.openSearch);
  const toggleTheme = useAppStore((s) => s.toggleTheme);
  const language = useAppStore((s) => s.language);
  const setLanguage = useAppStore((s) => s.setLanguage);
  const setSettingsOpen = useAppStore((s) => s.setSettingsOpen);
  const setQuickConnectOpen = useAppStore((s) => s.setQuickConnectOpen);
  const openSession = useAppStore((s) => s.openSession);

  const [newMenu, setNewMenu] = useState<{ x: number; y: number } | null>(null);

  const isTerminalView = activeId !== HOME_TAB && activeId !== TRANSFER_TAB;
  const hasActiveSession = sessions.some((s) => s.id === activeId && !s.closed);

  const newMenuItems = (): MenuItem[] => [
    {
      label: t("tab.newLocal"),
      disabled: !isTauri,
      onSelect: () =>
        openSession("pty", t("session.localTitle"), { type: "pty" }),
    },
    {
      label: t("tab.newSsh"),
      onSelect: () => setQuickConnectOpen(true),
    },
  ];

  return (
    <div id="tabbar">
      <div className="tab-strip" role="tablist">
        <div
          className={activeId === HOME_TAB ? "tab tab-home active" : "tab tab-home"}
          role="tab"
          aria-selected={activeId === HOME_TAB}
          tabIndex={0}
          onClick={goHome}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              goHome();
            }
          }}
        >
          <HomeIcon />
          <span className="tab-label">{t("app.name")}</span>
        </div>
        {sessions.map((s) => {
          const cls = ["tab"];
          if (s.id === activeId) cls.push("active");
          if (s.closed) cls.push("closed");
          return (
            <div
              key={s.id}
              className={cls.join(" ")}
              role="tab"
              aria-selected={s.id === activeId}
              draggable
              onDragStart={(e) => {
                e.dataTransfer.setData("text/ferriterm-session", s.id);
                e.dataTransfer.effectAllowed = "move";
              }}
              onClick={() => activate(s.id)}
            >
              <TerminalIcon />
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
                    const view = terminalHandle(s.id);
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
                aria-label={t("sessions.closeTab")}
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
        {transferOpen && (
          <div
            className={activeId === TRANSFER_TAB ? "tab active" : "tab"}
            role="tab"
            aria-selected={activeId === TRANSFER_TAB}
            tabIndex={0}
            onClick={() => activate(TRANSFER_TAB)}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                activate(TRANSFER_TAB);
              }
            }}
          >
            <FilesIcon />
            <span className="tab-label">{t("tab.transfer")}</span>
            <button
              className="tab-close"
              title={t("common.close")}
              aria-label={t("common.close")}
              onClick={(e) => {
                e.stopPropagation();
                closeTransfer();
              }}
            >
              ×
            </button>
          </div>
        )}
        <button
          className="icon-btn tab-new"
          title={t("tab.new")}
          aria-label={t("tab.new")}
          aria-haspopup="menu"
          onClick={(e) => {
            const rect = e.currentTarget.getBoundingClientRect();
            setNewMenu({ x: rect.left, y: rect.bottom + 4 });
          }}
        >
          <PlusIcon />
        </button>
      </div>
      <div className="tab-tools">
        <button
          className="icon-btn"
          title={splitId ? t("split.close") : t("split.start")}
          aria-label={splitId ? t("split.close") : t("split.start")}
          aria-pressed={!!splitId}
          disabled={!isTerminalView}
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
          className={transferOpen ? "icon-btn active" : "icon-btn"}
          title={t("transfer.open")}
          aria-label={t("transfer.open")}
          aria-pressed={transferOpen}
          onClick={() => openTransfer(isTerminalView ? activeId : undefined)}
        >
          <FilesIcon />
        </button>
        <button
          className={execLogOpen ? "icon-btn active" : "icon-btn"}
          title={t("log.toggle")}
          aria-label={t("log.toggle")}
          aria-pressed={execLogOpen}
          onClick={toggleExecLog}
        >
          <LogIcon />
        </button>
        <button
          className={commandsOpen ? "icon-btn active" : "icon-btn"}
          title={t("commands.toggle")}
          aria-label={t("commands.toggle")}
          aria-pressed={commandsOpen}
          onClick={toggleCommands}
        >
          <ListIcon />
        </button>
        <button
          className={monitorOpen ? "icon-btn active" : "icon-btn"}
          title={t("monitor.open")}
          aria-label={t("monitor.open")}
          aria-pressed={monitorOpen}
          onClick={toggleMonitor}
        >
          <PulseIcon />
        </button>
        <button
          className={forwardOpen ? "icon-btn active" : "icon-btn"}
          title={t("forward.open")}
          aria-label={t("forward.open")}
          aria-pressed={forwardOpen}
          onClick={toggleForward}
        >
          <ForwardIcon />
        </button>
        <button
          className="icon-btn"
          title={t("terminal.search")}
          aria-label={t("terminal.search")}
          disabled={!hasActiveSession}
          onClick={openSearch}
        >
          <SearchIcon />
        </button>
        <button
          className="icon-btn"
          title={t("theme.toggle")}
          aria-label={t("theme.toggle")}
          onClick={toggleTheme}
        >
          ◐
        </button>
        <button
          className="icon-btn tab-lang"
          title={t("language.toggle")}
          aria-label={t("language.toggle")}
          onClick={() => setLanguage(language === "en" ? "zh-CN" : "en")}
        >
          {language === "en" ? "EN" : "中"}
        </button>
        <button
          className="icon-btn"
          title={t("settings.open")}
          aria-label={t("settings.open")}
          onClick={() => setSettingsOpen(true)}
        >
          <GearIcon />
        </button>
      </div>
      {newMenu && (
        <ContextMenu
          x={newMenu.x}
          y={newMenu.y}
          items={newMenuItems()}
          onClose={() => setNewMenu(null)}
        />
      )}
    </div>
  );
}