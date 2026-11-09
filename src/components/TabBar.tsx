import { useState } from "react";
import { useTranslation } from "react-i18next";
import { isTauri } from "../api";
import { HOME_TAB, TRANSFER_TAB, useAppStore } from "../store";
import { terminalHandle } from "../terminal-registry";
import { ContextMenu, type MenuItem } from "./ContextMenu";
import { Modal } from "./Modal";
import { WindowControls } from "./WindowControls";
import {
  BroadcastIcon,
  FilesIcon,
  ForwardIcon,
  GearIcon,
  HomeIcon,
  PlayIcon,
  PlusIcon,
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
  const requestCloseSession = useAppStore((s) => s.requestCloseSession);
  const closeSession = useAppStore((s) => s.closeSession);
  const renameSession = useAppStore((s) => s.renameSession);
  const closeOtherSessions = useAppStore((s) => s.closeOtherSessions);
  const closeAllSessions = useAppStore((s) => s.closeAllSessions);
  const tabNumbering = useAppStore((s) => s.tabNumbering);
  const startRecording = useAppStore((s) => s.startRecording);
  const stopRecording = useAppStore((s) => s.stopRecording);
  const reconnect = useAppStore((s) => s.reconnect);
  const splitId = useAppStore((s) => s.splitId);
  const splitActive = useAppStore((s) => s.splitActive);
  const closeSplit = useAppStore((s) => s.closeSplit);
  const broadcastTargets = useAppStore((s) => s.broadcastTargets);
  const requestBroadcast = useAppStore((s) => s.requestBroadcast);
  const stopBroadcast = useAppStore((s) => s.stopBroadcast);
  const transferOpen = useAppStore((s) => s.transferOpen);
  const openTransfer = useAppStore((s) => s.openTransfer);
  const closeTransfer = useAppStore((s) => s.closeTransfer);
  const forwardOpen = useAppStore((s) => s.forwardOpen);
  const toggleForward = useAppStore((s) => s.toggleForward);
  const recordingsOpen = useAppStore((s) => s.recordingsOpen);
  const setRecordingsOpen = useAppStore((s) => s.setRecordingsOpen);
  const openSearch = useAppStore((s) => s.openSearch);
  const toggleTheme = useAppStore((s) => s.toggleTheme);
  const windowChrome = useAppStore((s) => s.windowChrome);
  const language = useAppStore((s) => s.language);
  const setLanguage = useAppStore((s) => s.setLanguage);
  const setSettingsOpen = useAppStore((s) => s.setSettingsOpen);
  const setQuickConnectOpen = useAppStore((s) => s.setQuickConnectOpen);
  const openSession = useAppStore((s) => s.openSession);

  const [newMenu, setNewMenu] = useState<{ x: number; y: number } | null>(null);
  const [tabMenu, setTabMenu] = useState<{
    id: string;
    x: number;
    y: number;
  } | null>(null);
  const [renameId, setRenameId] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState("");

  const isTerminalView = activeId !== HOME_TAB && activeId !== TRANSFER_TAB;
  const hasActiveSession = sessions.some((s) => s.id === activeId && !s.closed);
  const showNumbers =
    tabNumbering === "always" ||
    (tabNumbering === "asNeed" && sessions.length > 1);

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

  const tabMenuItems = (id: string): MenuItem[] => {
    const entry = sessions.find((s) => s.id === id);
    if (!entry) return [];
    return [
      {
        label: t("sessions.rename"),
        onSelect: () => {
          setRenameId(id);
          setRenameDraft(entry.title);
        },
      },
      ...(entry.closed && entry.lastConnect
        ? [
            {
              label: t("sessions.reconnect"),
              onSelect: () => reconnect(id),
            },
          ]
        : []),
      {
        label: t("sessions.closeTab"),
        onSelect: () => requestCloseSession(id),
      },
      {
        label: t("sessions.closeOthers"),
        disabled: sessions.length < 2,
        onSelect: () => closeOtherSessions(id),
      },
      {
        label: t("sessions.closeAll"),
        onSelect: () => closeAllSessions(),
      },
      { label: t("sessions.shiftCloseHint"), disabled: true },
    ];
  };

  return (
    <div id="tabbar" data-chrome={windowChrome}>
      {/* Home doubles as the brand once the OS title bar is gone, so it is a
          square icon slot rather than a labelled tab. */}
      <button
        type="button"
        className={
          activeId === HOME_TAB ? "tab-home-btn active" : "tab-home-btn"
        }
        title={t("app.name")}
        aria-label={t("app.name")}
        aria-pressed={activeId === HOME_TAB}
        onClick={goHome}
      >
        <HomeIcon size={16} />
      </button>
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
      <div className="tab-strip" role="tablist">
        {sessions.map((s, index) => {
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
              onContextMenu={(e) => {
                e.preventDefault();
                setTabMenu({ id: s.id, x: e.clientX, y: e.clientY });
              }}
            >
              <TerminalIcon />
              {s.production && (
                <span className="prod-dot" title={t("hosts.production")} />
              )}
              {showNumbers && <span className="tab-num">{index + 1}</span>}
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
                  if (e.shiftKey) closeSession(s.id);
                  else requestCloseSession(s.id);
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
      </div>
      {/* Empty header space doubles as the drag handle for the frameless
          window; it stays an explicit element so no tab or button becomes a
          drag target by accident. */}
      <div
        className="tab-drag"
        data-tauri-drag-region={
          windowChrome === "native" ? undefined : "true"
        }
      />
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
          className={forwardOpen ? "icon-btn active" : "icon-btn"}
          title={t("forward.open")}
          aria-label={t("forward.open")}
          aria-pressed={forwardOpen}
          onClick={toggleForward}
        >
          <ForwardIcon />
        </button>
        <button
          className={recordingsOpen ? "icon-btn active" : "icon-btn"}
          title={t("recordings.title")}
          aria-label={t("recordings.title")}
          aria-pressed={recordingsOpen}
          onClick={() => setRecordingsOpen(!recordingsOpen)}
        >
          <PlayIcon />
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
      {windowChrome === "custom" && <WindowControls />}
      {newMenu && (
        <ContextMenu
          x={newMenu.x}
          y={newMenu.y}
          items={newMenuItems()}
          onClose={() => setNewMenu(null)}
        />
      )}
      {tabMenu && (
        <ContextMenu
          x={tabMenu.x}
          y={tabMenu.y}
          items={tabMenuItems(tabMenu.id)}
          onClose={() => setTabMenu(null)}
        />
      )}
      {renameId && (
        <Modal
          title={t("sessions.renameTitle")}
          onClose={() => setRenameId(null)}
        >
          <form
            className="form-grid"
            onSubmit={(e) => {
              e.preventDefault();
              if (!renameDraft.trim()) return;
              renameSession(renameId, renameDraft);
              setRenameId(null);
            }}
          >
            <label className="field">
              <span>{t("sessions.renameLabel")}</span>
              <input
                autoFocus
                value={renameDraft}
                onChange={(e) => setRenameDraft(e.target.value)}
              />
            </label>
            <div className="actions">
              <button type="button" onClick={() => setRenameId(null)}>
                {t("common.cancel")}
              </button>
              <button
                type="submit"
                className="primary"
                disabled={!renameDraft.trim()}
              >
                {t("sessions.renameSave")}
              </button>
            </div>
          </form>
        </Modal>
      )}
    </div>
  );
}