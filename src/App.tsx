import { lazy, Suspense, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { ActivityRail } from "./components/ActivityRail";
import { Dock } from "./components/Dock";
import { RightStrip } from "./components/RightStrip";
import { StatusBar } from "./components/StatusBar";
import { TabBar } from "./components/TabBar";
import { HomeView } from "./components/HomeView";
import { HostTree } from "./components/HostTree";
import { NetTools } from "./components/NetTools";
import { ToolList } from "./components/ToolList";
import { TerminalArea } from "./components/TerminalArea";
import { BrowserBanner } from "./components/BrowserBanner";
import { BroadcastDialog } from "./components/BroadcastDialog";
import { CloseConfirmDialog } from "./components/CloseConfirmDialog";
import { CommandPalette } from "./components/CommandPalette";
import { ConnectForm } from "./components/ConnectForm";
import { HostKeyPrompt } from "./components/HostKeyPrompt";
import { KeyboardInteractivePrompt } from "./components/KeyboardInteractivePrompt";
import { Modal } from "./components/Modal";
import { PromptDialog } from "./components/PromptDialog";
import { ShortcutsDialog } from "./components/ShortcutsDialog";
import { api, isTauri, onForwardError, onTransferUpdate } from "./api";
import i18n from "./i18n";
import { installShortcuts } from "./shortcuts";
import { HOME_TAB, TRANSFER_TAB, useAppStore } from "./store";
import { resolveWindowChrome } from "./window-chrome";

const ForwardDialog = lazy(() =>
  import("./components/ForwardDialog").then((module) => ({
    default: module.ForwardDialog,
  })),
);
const TransferPanel = lazy(() =>
  import("./components/TransferPanel").then((module) => ({
    default: module.TransferPanel,
  })),
);
const SettingsDialog = lazy(() =>
  import("./components/SettingsDialog").then((module) => ({
    default: module.SettingsDialog,
  })),
);
const RecordingsDialog = lazy(() =>
  import("./components/RecordingsDialog").then((module) => ({
    default: module.RecordingsDialog,
  })),
);

const initialConnect = new URLSearchParams(window.location.search).get("connect");

export default function App() {
  const { t } = useTranslation();
  const broadcastRequested = useAppStore((s) => s.broadcastRequested);
  const forwardOpen = useAppStore((s) => s.forwardOpen);
  const transferOpen = useAppStore((s) => s.transferOpen);
  const activeId = useAppStore((s) => s.activeId);
  const settingsOpen = useAppStore((s) => s.settingsOpen);
  const setSettingsOpen = useAppStore((s) => s.setSettingsOpen);
  const recordingsOpen = useAppStore((s) => s.recordingsOpen);
  const quickConnectOpen = useAppStore((s) => s.quickConnectOpen);
  const setQuickConnectOpen = useAppStore((s) => s.setQuickConnectOpen);
  const sidebarOpen = useAppStore((s) => s.sidebarOpen);
  const sidebarView = useAppStore((s) => s.sidebarView);

  const isTerminalView = activeId !== HOME_TAB && activeId !== TRANSFER_TAB;

  useEffect(() => {
    const uninstall = installShortcuts();
    return uninstall;
  }, []);

  // The frame shape is discovered once: macOS keeps its traffic lights, Windows
  // drops the OS frame when the build allows it, everything else keeps it.
  const windowChrome = useAppStore((s) => s.windowChrome);
  const setWindowChrome = useAppStore((s) => s.setWindowChrome);

  useEffect(() => {
    void resolveWindowChrome().then(setWindowChrome);
  }, [setWindowChrome]);

  useEffect(() => {
    document.documentElement.dataset.chrome = windowChrome;
  }, [windowChrome]);

  useEffect(() => {
    if (!isTauri) return;
    let disposed = false;
    const stops: (() => void)[] = [];
    void (async () => {
      try {
        const jobs = await api.transferList();
        if (!disposed) useAppStore.getState().setTransfers(jobs);
      } catch {
        // the transfer list is best-effort
      }
      try {
        const stop = await onTransferUpdate((job) =>
          useAppStore.getState().upsertTransfer(job),
        );
        if (disposed) stop();
        else stops.push(stop);
      } catch {
        // transfer events are best-effort
      }
      try {
        const stop = await onForwardError((error) => {
          const message = i18n.t("forward.autoStartError", {
            message: error.message,
          });
          const state = useAppStore.getState();
          state.setStatus(message, true);
          state.logEvent(message);
          if (!state.execLogOpen) state.toggleExecLog();
        });
        if (disposed) stop();
        else stops.push(stop);
      } catch {
        // forwarding diagnostics are best-effort
      }
    })();
    return () => {
      disposed = true;
      stops.forEach((stop) => stop());
    };
  }, []);

  useEffect(() => {
    if (initialConnect) {
      useAppStore.setState({ newWindowHostId: initialConnect });
      window.history.replaceState({}, "", window.location.pathname);
    }
    const {
      loadHosts,
      loadTags,
      loadLanguagePacks,
      loadSettings,
      restoreLayout,
      setStatus,
    } = useAppStore.getState();
    void (async () => {
      try {
        await loadLanguagePacks();
        await Promise.all([loadSettings(), loadHosts(), loadTags()]);
        if (!initialConnect) await restoreLayout();
      } catch (err) {
        setStatus(String(err), true);
      }
    })();
  }, []);

  useEffect(() => {
    if (initialConnect) return;
    let timer: number | undefined;
    const unsubscribe = useAppStore.subscribe((state, previous) => {
      if (state.sessions === previous.sessions) return;
      window.clearTimeout(timer);
      timer = window.setTimeout(() => useAppStore.getState().saveLayout(), 400);
    });
    return () => {
      unsubscribe();
      window.clearTimeout(timer);
    };
  }, []);

  return (
    <>
      <div id="app">
        <TabBar />
        <div id="body">
          <ActivityRail />
          <div id="body-main">
            <aside id="host-sidebar" hidden={!sidebarOpen}>
              {sidebarView === "tools" ? <ToolList /> : <HostTree />}
            </aside>
            <div id="content">
              {activeId === HOME_TAB && sidebarView === "tools" && <NetTools />}
              {activeId === HOME_TAB && sidebarView === "hosts" && <HomeView />}
              {transferOpen && (
                <div className="view-transfer" hidden={activeId !== TRANSFER_TAB}>
                  <Suspense fallback={null}>
                    <TransferPanel />
                  </Suspense>
                </div>
              )}
              <div id="workspace" hidden={!isTerminalView}>
                <TerminalArea />
              </div>
            </div>
            <Dock />
          </div>
          <RightStrip />
        </div>
        <StatusBar />
      </div>
      <BrowserBanner />
      <CommandPalette />
      <Suspense fallback={null}>{forwardOpen && <ForwardDialog />}</Suspense>
      <HostKeyPrompt />
      <KeyboardInteractivePrompt />
      {broadcastRequested && <BroadcastDialog />}
      <ShortcutsDialog />
      <CloseConfirmDialog />
      <PromptDialog />
      <Suspense fallback={null}>
        {settingsOpen && (
          <SettingsDialog onClose={() => setSettingsOpen(false)} />
        )}
      </Suspense>
      <Suspense fallback={null}>{recordingsOpen && <RecordingsDialog />}</Suspense>
      {quickConnectOpen && (
        <Modal
          title={t("connect.quick")}
          onClose={() => setQuickConnectOpen(false)}
        >
          <ConnectForm
            heading={false}
            onConnected={() => setQuickConnectOpen(false)}
          />
        </Modal>
      )}
    </>
  );
}