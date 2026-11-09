import { lazy, Suspense, useRef } from "react";
import { useTranslation } from "react-i18next";
import { useAppStore } from "../store";
import { TerminalSearch } from "./TerminalSearch";

const TerminalPanel = lazy(() =>
  import("./TerminalPanel").then((module) => ({ default: module.TerminalPanel })),
);
const SftpPanel = lazy(() =>
  import("./SftpPanel").then((module) => ({ default: module.SftpPanel })),
);

export function TerminalArea() {
  const { t } = useTranslation();
  const sessions = useAppStore((s) => s.sessions);
  const activeId = useAppStore((s) => s.activeId);
  const splitId = useAppStore((s) => s.splitId);
  const splitDirection = useAppStore((s) => s.splitDirection);
  const splitRatio = useAppStore((s) => s.splitRatio);
  const setSplitRatio = useAppStore((s) => s.setSplitRatio);
  const closeSplit = useAppStore((s) => s.closeSplit);
  const hostRef = useRef<HTMLDivElement>(null);

  const split = splitId && splitId !== activeId ? splitId : null;
  const hostClass = ["", split ? "split" : "", split ? splitDirection : ""]
    .filter(Boolean)
    .join(" ");

  const onGutterDown = (event: React.PointerEvent<HTMLDivElement>) => {
    const host = hostRef.current;
    if (!host) return;
    event.preventDefault();
    const rect = host.getBoundingClientRect();
    const move = (moveEvent: PointerEvent) => {
      const ratio =
        splitDirection === "horizontal"
          ? (moveEvent.clientX - rect.left) / rect.width
          : (moveEvent.clientY - rect.top) / rect.height;
      setSplitRatio(ratio);
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  const onDrop = (event: React.DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    const id = event.dataTransfer.getData("text/ferriterm-session");
    const host = hostRef.current;
    if (!id || !host) return;
    const rect = host.getBoundingClientRect();
    const x = Math.abs((event.clientX - rect.left) / rect.width - 0.5);
    const y = Math.abs((event.clientY - rect.top) / rect.height - 0.5);
    useAppStore
      .getState()
      .splitWith(id, x >= y ? "horizontal" : "vertical");
  };

  return (
    <div
      ref={hostRef}
      id="terminal-host"
      className={hostClass}
      onDragOver={(event) => {
        if (event.dataTransfer.types.includes("text/ferriterm-session")) {
          event.preventDefault();
        }
      }}
      onDrop={onDrop}
      style={
        split
          ? ({ "--split-ratio": `${(splitRatio * 100).toFixed(2)}%` } as React.CSSProperties)
          : undefined
      }
    >
      <TerminalSearch />
      {sessions.length === 0 && (
        <div id="empty-hint">
          <p className="hint-mark">&gt;_</p>
          <p>{t("empty.hint")}</p>
          <p className="hint-sub">{t("empty.sub")}</p>
        </div>
      )}
      {split && (
        <div
          className="split-gutter"
          role="separator"
          aria-orientation={splitDirection === "horizontal" ? "vertical" : "horizontal"}
          onPointerDown={onGutterDown}
          onDoubleClick={() => setSplitRatio(0.5)}
          title={t("split.reset")}
        >
          <button
            className="split-close"
            title={t("split.close")}
            aria-label={t("split.close")}
            onPointerDown={(event) => event.stopPropagation()}
            onClick={(event) => {
              event.stopPropagation();
              closeSplit();
            }}
          >
            ×
          </button>
        </div>
      )}
      <Suspense fallback={null}>
        {sessions.map((s) => {
          const pane =
            s.id === activeId
              ? "primary"
              : s.id === split
                ? "secondary"
                : null;
          return s.kind === "sftp" ? (
            <SftpPanel key={s.id} session={s} pane={pane} />
          ) : (
            <TerminalPanel key={s.id} session={s} pane={pane} />
          );
        })}
      </Suspense>
    </div>
  );
}
