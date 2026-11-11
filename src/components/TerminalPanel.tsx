import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { TerminalView } from "../terminal";
import { readClipboardText, writeClipboardText } from "../clipboard";
import i18n from "../i18n";
import { useAppStore, type SessionMeta } from "../store";
import { ContextMenu, type MenuItem } from "./ContextMenu";

export function TerminalPanel({
  session,
  pane,
}: {
  session: SessionMeta;
  pane: "primary" | "secondary" | null;
}) {
  const { t } = useTranslation();
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<TerminalView | null>(null);
  const themeVersion = useAppStore((s) => s.themeVersion);
  const keywordHighlights = useAppStore((s) => s.keywordHighlights);
  const terminalFontFamily = useAppStore((s) => s.terminalFontFamily);
  const terminalFontSize = useAppStore((s) => s.terminalFontSize);
  const terminalScrollback = useAppStore((s) => s.terminalScrollback);
  const terminalCursorBlink = useAppStore((s) => s.terminalCursorBlink);
  const terminalCursorStyle = useAppStore((s) => s.terminalCursorStyle);
  const terminalRightClickPaste = useAppStore((s) => s.terminalRightClickPaste);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const { consumePending, markClosed } = useAppStore.getState();
    const view = new TerminalView(session.id, session.kind, container, () =>
      markClosed(session.id),
    );
    viewRef.current = view;

    const pending = consumePending(session.id);
    if (pending) view.start(pending);

    return () => {
      view.dispose();
      viewRef.current = null;
    };
  }, [session.id, session.kind]);

  useEffect(() => {
    if (!pane) return;
    const raf = requestAnimationFrame(() => {
      viewRef.current?.fitNow();
      if (pane === "primary") viewRef.current?.focus();
    });
    return () => cancelAnimationFrame(raf);
  }, [pane]);

  useEffect(() => {
    viewRef.current?.refreshTheme();
  }, [themeVersion]);

  useEffect(() => {
    viewRef.current?.setKeywords(keywordHighlights);
  }, [keywordHighlights]);

  useEffect(() => {
    viewRef.current?.refreshAppearance();
  }, [
    terminalFontFamily,
    terminalFontSize,
    terminalScrollback,
    terminalCursorBlink,
    terminalCursorStyle,
  ]);

  const menuItems = (): MenuItem[] => {
    const view = viewRef.current;
    const selection = view?.term.getSelection() ?? "";
    return [
      {
        label: t("terminal.copy"),
        disabled: !selection,
        onSelect: () => {
          if (!selection) return;
          writeClipboardText(selection).catch((err) => {
            useAppStore.getState().setStatus(String(err), true);
          });
        },
      },
      {
        label: t("terminal.paste"),
        onSelect: () => {
          readClipboardText()
            .then((text) => {
              if (text) viewRef.current?.insert(text);
            })
            .catch((err) => {
              useAppStore.getState().setStatus(String(err), true);
            });
        },
      },
      {
        label: t("terminal.selectAll"),
        onSelect: () => {
          viewRef.current?.term.selectAll();
          viewRef.current?.focus();
        },
      },
      {
        label: t("terminal.clear"),
        onSelect: () => {
          viewRef.current?.term.clear();
          viewRef.current?.focus();
        },
      },
      {
        label: t("terminal.search"),
        onSelect: () => {
          useAppStore.getState().setFocusedSession(session.id);
          useAppStore.getState().openSearch();
        },
      },
      {
        label: t("terminal.split"),
        onSelect: () => {
          const store = useAppStore.getState();
          store.activate(session.id);
          store.splitActive();
        },
      },
      {
        label: t("terminal.newTab"),
        onSelect: () =>
          useAppStore
            .getState()
            .openSession("pty", i18n.t("session.localTitle"), { type: "pty" }),
      },
    ];
  };

  return (
    <>
      <div
        ref={containerRef}
        className={`terminal-panel${pane ? ` pane-${pane}` : ""}`}
        data-testid="terminal"
        onPointerDown={() =>
          useAppStore.getState().setFocusedSession(session.id)
        }
        onContextMenu={(event) => {
          event.preventDefault();
          useAppStore.getState().setFocusedSession(session.id);
          if (terminalRightClickPaste) {
            setMenu(null);
            void readClipboardText()
              .then((text) => {
                if (text) viewRef.current?.insert(text);
              })
              .catch((err) => {
                useAppStore.getState().setStatus(String(err), true);
              });
            return;
          }
          setMenu({ x: event.clientX, y: event.clientY });
        }}
      />
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          items={menuItems()}
          onClose={() => setMenu(null)}
        />
      )}
    </>
  );
}