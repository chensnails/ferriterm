import { lazy, Suspense, useEffect } from "react";
import { Sidebar } from "./components/Sidebar";
import { TabBar } from "./components/TabBar";
import { TerminalArea } from "./components/TerminalArea";
import { BrowserBanner } from "./components/BrowserBanner";
import { ExecutionLog } from "./components/ExecutionLog";
import { BroadcastDialog } from "./components/BroadcastDialog";
import { CommandPalette } from "./components/CommandPalette";
import { CommandRail } from "./components/CommandRail";
import { HostKeyPrompt } from "./components/HostKeyPrompt";
import { KeyboardInteractivePrompt } from "./components/KeyboardInteractivePrompt";
import { ShortcutsDialog } from "./components/ShortcutsDialog";
import { api, isTauri, onTransferUpdate } from "./api";
import { installShortcuts } from "./shortcuts";
import { useAppStore } from "./store";

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

const initialConnect = new URLSearchParams(window.location.search).get("connect");

export default function App() {
  const broadcastRequested = useAppStore((s) => s.broadcastRequested);
  const monitorOpen = useAppStore((s) => s.monitorOpen);
  const forwardOpen = useAppStore((s) => s.forwardOpen);
  const transferOpen = useAppStore((s) => s.transferOpen);

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
        <Sidebar />
        <main id="main">
          <TabBar />
          <div id="workspace">
            <TerminalArea />
            <CommandRail />
          </div>
          <ExecutionLog />
        </main>
      </div>
      <BrowserBanner />
      <Suspense fallback={null}>{monitorOpen && <MonitorPanel />}</Suspense>
      <CommandPalette />
      <Suspense fallback={null}>{forwardOpen && <ForwardDialog />}</Suspense>
      <HostKeyPrompt />
      <KeyboardInteractivePrompt />
      {broadcastRequested && <BroadcastDialog />}
      <Suspense fallback={null}>{transferOpen && <TransferPanel />}</Suspense>
      <ShortcutsDialog />
    </>
  );
}