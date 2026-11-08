import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import "@xterm/xterm/css/xterm.css";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { api, onClosed, onOutput, type SessionKind } from "./api";
import { terminalTheme } from "./theme";
import i18n from "./i18n";

export class TerminalView {
  readonly term: Terminal;
  readonly ready: Promise<void>;
  private fit = new FitAddon();
  private unlisten: UnlistenFn[] = [];
  private observer: ResizeObserver;
  private disposed = false;
  // Rust 侧并发执行 IPC command，互斥锁只保证串行不保证到达顺序；链式写入保 keystroke 顺序
  private writeChain: Promise<void> = Promise.resolve();

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
      theme: terminalTheme(),
    });
    this.term.loadAddon(this.fit);
    this.term.loadAddon(new WebLinksAddon());
    this.term.open(container);
    this.fit.fit();

    this.term.onData((data) => {
      this.writeChain = this.writeChain.then(() =>
        api.write(this.kind, this.id, data).catch(console.error),
      );
    });
    this.term.onResize(({ cols, rows }) => {
      api.resize(this.kind, this.id, cols, rows).catch(() => {});
    });

    // 先注册事件监听，再发起连接，避免丢失服务端首批输出
    this.ready = Promise.all([
      onOutput(this.id, (bytes) => {
        if (!this.disposed) this.term.write(bytes);
      }),
      onClosed(this.id, () => {
        if (this.disposed) return;
        this.term.writeln(`\r\n\x1b[90m${i18n.t("terminal.disconnected")}\x1b[0m`);
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

  refreshTheme() {
    this.term.options.theme = terminalTheme();
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