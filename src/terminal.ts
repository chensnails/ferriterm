import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import "@xterm/xterm/css/xterm.css";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { api, onClosed, onOutput, type SessionKind } from "./api";

export class TerminalView {
  readonly term: Terminal;
  private fit = new FitAddon();
  private unlisten: UnlistenFn[] = [];
  private observer: ResizeObserver;
  private disposed = false;

  constructor(
    readonly id: string,
    private kind: SessionKind,
    container: HTMLElement,
    onSessionClosed: () => void,
  ) {
    this.term = new Terminal({
      cursorBlink: true,
      fontSize: 14,
      fontFamily:
        '"JetBrains Mono", "Cascadia Code", Menlo, Consolas, "Courier New", monospace',
      theme: {
        background: "#0f1117",
        foreground: "#d4d4d8",
        cursor: "#7dd3fc",
        selectionBackground: "#33415588",
      },
    });
    this.term.loadAddon(this.fit);
    this.term.loadAddon(new WebLinksAddon());
    this.term.open(container);
    this.fit.fit();

    this.term.onData((data) => {
      api.write(this.kind, this.id, data).catch(console.error);
    });
    this.term.onResize(({ cols, rows }) => {
      api.resize(this.kind, this.id, cols, rows).catch(() => {});
    });

    // 先注册事件监听，再发起连接，避免丢失服务端首批输出
    Promise.all([
      onOutput(this.id, (bytes) => {
        if (!this.disposed) this.term.write(bytes);
      }),
      onClosed(this.id, () => {
        if (this.disposed) return;
        this.term.writeln("\r\n\x1b[90m[会话已断开]\x1b[0m");
        onSessionClosed();
      }),
    ]).then((fns) => {
      if (this.disposed) fns.forEach((fn) => fn());
      else this.unlisten.push(...fns);
    });

    this.observer = new ResizeObserver(() => {
      if (!this.disposed) this.fit.fit();
    });
    this.observer.observe(container);
  }

  focus() {
    this.term.focus();
  }

  fitNow() {
    this.fit.fit();
  }

  dispose() {
    this.disposed = true;
    this.observer.disconnect();
    this.unlisten.forEach((fn) => fn());
    this.term.dispose();
  }
}