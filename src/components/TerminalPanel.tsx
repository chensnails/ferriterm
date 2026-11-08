import { useEffect, useRef } from "react";
import { TerminalView } from "../terminal";
import { api } from "../api";
import i18n from "../i18n";
import { useAppStore, type SessionMeta } from "../store";

export function TerminalPanel({ session }: { session: SessionMeta }) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<TerminalView | null>(null);
  const activeId = useAppStore((s) => s.activeId);
  const theme = useAppStore((s) => s.theme);
  const isActive = activeId === session.id;

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const { consumePending, markClosed, setStatus } = useAppStore.getState();
    const view = new TerminalView(session.id, session.kind, container, () =>
      markClosed(session.id),
    );
    viewRef.current = view;

    const pending = consumePending(session.id);
    if (pending) {
      void view.ready.then(() => {
        const { cols, rows } = view.term;
        const connect =
          pending.type === "ssh"
            ? api.sshConnect(session.id, { ...pending.args, cols, rows })
            : api.ptySpawn(session.id, cols, rows);
        connect
          .then(() => setStatus(""))
          .catch((err) => {
            setStatus(String(err), true);
            view.term.writeln(
              `\r\n\x1b[31m${i18n.t("terminal.connectFailed", { error: String(err) })}\x1b[0m`,
            );
            markClosed(session.id);
          });
      });
    }

    return () => {
      view.dispose();
      viewRef.current = null;
    };
  }, [session.id, session.kind]);

  useEffect(() => {
    if (!isActive) return;
    const raf = requestAnimationFrame(() => {
      viewRef.current?.fitNow();
      viewRef.current?.focus();
    });
    return () => cancelAnimationFrame(raf);
  }, [isActive]);

  useEffect(() => {
    viewRef.current?.refreshTheme();
  }, [theme]);

  return (
    <div
      ref={containerRef}
      className={isActive ? "terminal-panel active" : "terminal-panel"}
    />
  );
}