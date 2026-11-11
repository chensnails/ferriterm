import {
  lazy,
  Suspense,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from "react";
import { useTranslation } from "react-i18next";
import { useAppStore, type DockCardId } from "../store";
import { CommandRail } from "./CommandRail";
import { ExecutionLog } from "./ExecutionLog";
import { AiPanel } from "./AiPanel";
import { IconButton, Island, PanelHeader } from "./ui";
import { CloseIcon } from "./icons";

const MonitorPanel = lazy(() =>
  import("./MonitorPanel").then((module) => ({
    default: module.MonitorPanel,
  })),
);

/**
 * Right dock: every companion panel is a card in here instead of a modal, an
 * inline rail or a bottom drawer. Cards stack vertically and both the dock
 * width and each card height are draggable and persisted.
 */
export function Dock() {
  const { t } = useTranslation();

  const dockWidth = useAppStore((s) => s.dockWidth);
  const setDockWidth = useAppStore((s) => s.setDockWidth);
  const dockHeights = useAppStore((s) => s.dockHeights);
  const setDockCardHeight = useAppStore((s) => s.setDockCardHeight);

  const commandsOpen = useAppStore((s) => s.commandsOpen);
  const monitorOpen = useAppStore((s) => s.monitorOpen);
  const execLogOpen = useAppStore((s) => s.execLogOpen);
  const aiOpen = useAppStore((s) => s.aiOpen);
  const toggleCommands = useAppStore((s) => s.toggleCommands);
  const toggleMonitor = useAppStore((s) => s.toggleMonitor);
  const toggleExecLog = useAppStore((s) => s.toggleExecLog);
  const toggleAi = useAppStore((s) => s.toggleAi);
  const clearExecLog = useAppStore((s) => s.clearExecLog);
  const logCount = useAppStore((s) => s.execLog.length);

  const openIds: DockCardId[] = [];
  if (commandsOpen) openIds.push("commands");
  if (monitorOpen) openIds.push("monitor");
  if (execLogOpen) openIds.push("log");
  if (aiOpen) openIds.push("ai");

  if (openIds.length === 0) return null;

  const close: Record<DockCardId, () => void> = {
    commands: toggleCommands,
    monitor: toggleMonitor,
    log: toggleExecLog,
    ai: toggleAi,
  };

  const trackDrag = (
    axis: "x" | "y",
    base: number,
    apply: (next: number) => void,
    max: number,
  ) => (event: ReactPointerEvent<HTMLDivElement>) => {
    event.preventDefault();
    const start = axis === "x" ? event.clientX : event.clientY;
    document.body.classList.add(`dock-resizing-${axis}`);
    const move = (moveEvent: PointerEvent) => {
      const delta =
        axis === "x"
          ? // the dock grows when the grip is dragged towards the left
            base - (moveEvent.clientX - start)
          : base + (moveEvent.clientY - start);
      apply(Math.min(max, delta));
    };
    const stop = () => {
      document.body.classList.remove(`dock-resizing-${axis}`);
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
      window.removeEventListener("pointercancel", stop);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop, { once: true });
    window.addEventListener("pointercancel", stop, { once: true });
  };

  const cards: Record<
    DockCardId,
    { title: string; actions?: ReactNode; body: ReactNode }
  > = {
    commands: { title: t("commands.title"), body: <CommandRail /> },
    monitor: {
      title: t("monitor.title"),
      body: (
        <div className="dock-scroll">
          <Suspense fallback={null}>
            <MonitorPanel />
          </Suspense>
        </div>
      ),
    },
    log: {
      title: t("log.title"),
      actions: (
        <button
          type="button"
          className="link-btn"
          onClick={clearExecLog}
          disabled={logCount === 0}
        >
          {t("log.clear")}
        </button>
      ),
      body: <ExecutionLog />,
    },
    ai: {
      title: t("ai.title"),
      body: <AiPanel />,
    },
  };

  return (
    <aside
      id="dock"
      style={{ width: dockWidth }}
      aria-label={t("dock.title")}
    >
      <div
        className="dock-gutter"
        role="separator"
        aria-orientation="vertical"
        aria-label={t("dock.resizeWidth")}
        tabIndex={0}
        onPointerDown={trackDrag("x", dockWidth, setDockWidth, 2000)}
      />
      {openIds.map((id) => {
        const card = cards[id];
        // Heights are weights, not pixels: the cards always fill the column,
        // so a lone card spans it and only a stack has draggable dividers.
        return (
          <Island
            key={id}
            className="dock-card"
            style={{ flex: `${dockHeights[id]} 1 0`, minHeight: 0 }}
          >
            <PanelHeader
              compact
              title={card.title}
              actions={
                <>
                  {card.actions}
                  <IconButton
                    label={t("common.close")}
                    size="sm"
                    onClick={close[id]}
                  >
                    <CloseIcon size={12} />
                  </IconButton>
                </>
              }
            />
            <div className="dock-card-body">{card.body}</div>
            {openIds.length > 1 && (
              <div
                className="dock-resizer"
                role="separator"
                aria-orientation="horizontal"
                aria-label={t("dock.resizeHeight")}
                onPointerDown={trackDrag(
                  "y",
                  dockHeights[id],
                  (next) => setDockCardHeight(id, next),
                  900,
                )}
              />
            )}
          </Island>
        );
      })}
    </aside>
  );
}
