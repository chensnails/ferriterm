import { lazy, Suspense, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { TabBar } from "./components/TabBar";
import { HomeView } from "./components/HomeView";
import { TerminalArea } from "./components/TerminalArea";
import { BrowserBanner } from "./components/BrowserBanner";
import { ExecutionLog } from "./components/ExecutionLog";
import { BroadcastDialog } from "./components/BroadcastDialog";
import { CommandPalette } from "./components/CommandPalette";
import { CommandRail } from "./components/CommandRail";
import { ConnectForm } from "./components/ConnectForm";
import { HostKeyPrompt } from "./components/HostKeyPrompt";
import { KeyboardInteractivePrompt } from "./components/KeyboardInteractivePrompt";
import { Modal } from "./components/Modal";
import { ShortcutsDialog } from "./components/ShortcutsDialog";
import { api, isTauri, onTransferUpdate } from "./api";
import { installShortcuts } from "./shortcuts";
import { HOME_TAB, TRANSFER_TAB, useAppStore } from "./store";

const MonitorPanel = lazy(() =>
  import("./components/MonitorPanel").then((module) => ({
    default: module.MonitorPanel,
  })),
);
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

const initialConnect = new URLSearchParams(window.location.search).get("connect");

export default function App() {
  const { t } = useTranslation();
  const broadcastRequested = useAppStore((s) => s.broadcastRequested);
  const monitorOpen = useAppStore((s) => s.monitorOpen);
  const forwardOpen = useAppStore((s) => s.forwardOpen);
  const transferOpen = useAppStore((s) => s.transferOpen);
  const activeId = useAppStore((s) => s.activeId);
  const settingsOpen = useAppStore((s) => s.settingsOpen);
  const setSettingsOpen = useAppStore((s) => s.setSettingsOpen);
  const quickConnectOpen = useAppStore((s) => s.quickConnectOpen);
  const setQuickConnectOpen = useAppStore((s) => s.setQuickConnectOpen);

  const isTerminalView = activeId !== HOME_TAB && activeId !== TRANSFER_TAB;

  useEffect(() => {
    const uninstall = installShortcuts();
    return uninstall;
  }, []);

  useEffect(() => {
    if (!isTauri) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void (async () => {
      try {
        const jobs = await api.transferList();
        if (!disposed) useAppStore.getState().setTransfers(jobs);
        const stop = await onTransferUpdate((job) =>
          useAppStore.getState().upsertTransfer(job),
        );
        if (disposed) stop();
        else unlisten = stop;
      } catch {
        // transfer events are best-effort
      }
    })();
    return () => {
      disposed = true;
      unlisten?.();
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
        <div id="content">
          {activeId === HOME_TAB && <HomeView />}
          {transferOpen && (
            <div className="view-transfer" hidden={activeId !== TRANSFER_TAB}>
              <Suspense fallback={null}>
                <TransferPanel />
              </Suspense>
            </div>
          )}
          <div id="workspace" hidden={!isTerminalView}>
            <TerminalArea />
            <CommandRail />
          </div>
        </div>
        <ExecutionLog />
      </div>
      <BrowserBanner />
      <Suspense fallback={null}>{monitorOpen && <MonitorPanel />}</Suspense>
      <CommandPalette />
      <Suspense fallback={null}>{forwardOpen && <ForwardDialog />}</Suspense>
      <HostKeyPrompt />
      <KeyboardInteractivePrompt />
      {broadcastRequested && <BroadcastDialog />}
      <ShortcutsDialog />
      <Suspense fallback={null}>
        {settingsOpen && (
          <SettingsDialog onClose={() => setSettingsOpen(false)} />
        )}
      </Suspense>
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