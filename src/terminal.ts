import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import "@xterm/xterm/css/xterm.css";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { api, onClosed, onOutput, type SessionKind } from "./api";
import { terminalTheme } from "./theme";
import i18n from "./i18n";
import { useAppStore } from "./store";

const views = new Map<string, TerminalView>();

interface SearchHit {
  row: number;
  col: number;
  length: number;
}

export interface SearchResult {
  index: number;
  count: number;
  error?: boolean;
}


export function terminalView(id: string): TerminalView | null {
  return views.get(id) ?? null;
}

export class TerminalView {
  readonly term: Terminal;
  readonly ready: Promise<void>;
  private fit = new FitAddon();
  private unlisten: UnlistenFn[] = [];
  private observer: ResizeObserver;
  private disposed = false;
  // Rust 侧并发执行 IPC command，互斥锁只保证串行不保证到达顺序；链式写入保 keystroke 顺序
  private writeChain: Promise<void> = Promise.resolve();
  private searchQuery = "";
  private searchHits: SearchHit[] = [];
  private searchIndex = -1;
  private searchRegex = false;
  private inputBuffer = "";
  private escapeState = 0;

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
    views.set(id, this);

    this.term.onData((data) => {
      this.trackInput(data);
      this.writeChain = this.writeChain.then(() => {
        useAppStore.getState().sendInput(this.kind, this.id, data);
      });
    });
    this.term.textarea?.addEventListener("focus", () => {
      useAppStore.getState().setFocusedSession(this.id);
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

  insert(text: string) {
    useAppStore.getState().setFocusedSession(this.id);
    this.writeChain = this.writeChain.then(() => {
      useAppStore.getState().sendInput(this.kind, this.id, text);
    });
    this.focus();
  }

  // Reconstruct the line the user is typing from raw terminal input so history
  // and the execution log do not need shell integration.
  private trackInput(data: string) {
    for (const ch of data) {
      if (this.escapeState === 2) {
        if (ch >= "@" && ch <= "~") this.escapeState = 0;
        continue;
      }
      if (this.escapeState === 1) {
        this.escapeState = ch === "[" || ch === "O" ? 2 : 0;
        continue;
      }
      if (ch === "\x1b") {
        this.escapeState = 1;
        continue;
      }
      if (ch === "\r" || ch === "\n") {
        const line = this.inputBuffer.trim();
        this.inputBuffer = "";
        if (line) useAppStore.getState().recordCommand(this.id, line);
      } else if (ch === "\x7f" || ch === "\b") {
        this.inputBuffer = this.inputBuffer.slice(0, -1);
      } else if (ch === "\x03" || ch === "\x15") {
        this.inputBuffer = "";
      } else if (ch >= " ") {
        this.inputBuffer += ch;
      }
      if (this.inputBuffer.length > 2000) {
        this.inputBuffer = this.inputBuffer.slice(-2000);
      }
    }
  }

  fitNow() {
    this.fit.fit();
  }

  find(query: string, backwards = false, regex = false): SearchResult {
    if (!query) {
      this.resetSearch();
      return { index: -1, count: 0 };
    }
    if (query !== this.searchQuery || regex !== this.searchRegex) {
      this.searchQuery = query;
      this.searchRegex = regex;
      const collected = this.collectHits(query, regex);
      this.searchHits = collected.hits;
      this.searchIndex = -1;
      if (collected.error) {
        this.term.clearSelection();
        return { index: -1, count: 0, error: true };
      }
    }
    if (this.searchHits.length === 0) {
      this.term.clearSelection();
      return { index: -1, count: 0 };
    }
    const step = backwards ? -1 : 1;
    const total = this.searchHits.length;
    this.searchIndex = (this.searchIndex + step + total) % total;
    const hit = this.searchHits[this.searchIndex];
    this.term.select(hit.col, hit.row, hit.length);
    this.term.scrollToLine(hit.row);
    return { index: this.searchIndex, count: total };
  }

  resetSearch() {
    this.searchQuery = "";
    this.searchHits = [];
    this.searchIndex = -1;
    this.searchRegex = false;
    this.term.clearSelection();
  }

  private collectHits(
    query: string,
    regex: boolean,
  ): { hits: SearchHit[]; error?: boolean } {
    let pattern: RegExp | null = null;
    if (regex) {
      try {
        pattern = new RegExp(query, "gi");
      } catch {
        return { hits: [], error: true };
      }
    }
    const buffer = this.term.buffer.active;
    const needle = query.toLowerCase();
    const hits: SearchHit[] = [];
    for (let row = 0; row < buffer.length; row += 1) {
      const line = buffer.getLine(row);
      if (!line) continue;
      const text = line.translateToString(true);
      if (pattern) {
        pattern.lastIndex = 0;
        let match = pattern.exec(text);
        while (match) {
          hits.push({ row, col: match.index, length: match[0].length });
          if (match[0].length === 0) pattern.lastIndex += 1;
          if (hits.length >= 5000) return { hits };
          match = pattern.exec(text);
        }
      } else {
        const haystack = text.toLowerCase();
        let from = 0;
        for (;;) {
          const at = haystack.indexOf(needle, from);
          if (at < 0) break;
          hits.push({ row, col: at, length: query.length });
          from = at + needle.length;
          if (hits.length >= 5000) return { hits };
        }
      }
    }
    return { hits };
  }

  dispose() {
    this.disposed = true;
    if (useAppStore.getState().focusedSessionId === this.id) {
      useAppStore.getState().setFocusedSession(null);
    }
    if (views.get(this.id) === this) views.delete(this.id);
    this.observer.disconnect();
    this.unlisten.forEach((fn) => fn());
    this.term.dispose();
  }
}