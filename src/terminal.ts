import {
  Terminal,
  type IDecoration,
  type IDisposable,
  type IMarker,
} from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import "@xterm/xterm/css/xterm.css";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { api, onClosed, onOutput, type SessionKind } from "./api";
import {
  registerTerminal,
  unregisterTerminal,
  type SearchResult,
  type TerminalHandle,
} from "./terminal-registry";
import { terminalTheme } from "./theme";
import i18n from "./i18n";
import { historyForHost, historyMatches } from "./history";
import { blendHex } from "./scheme";
import { useAppStore, type KeywordRule } from "./store";

interface SearchHit {
  row: number;
  col: number;
  length: number;
}

export const TERMINAL_FONT_SIZE = 14;
export const TERMINAL_FONT_FAMILY =
  '"JetBrains Mono", "Cascadia Code", Menlo, Consolas, "Courier New", monospace';

export class TerminalView implements TerminalHandle {
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
  private suggestEl: HTMLDivElement;
  private menuEl: HTMLDivElement;
  private matches: { text: string }[] = [];
  private suggestion = "";
  private suggestVisible = false;
  private keywords: KeywordRule[] = [];
  private keywordTimer: number | undefined;
  private keywordDecorations: { decoration: IDecoration; marker: IMarker }[] =
    [];
  private osc52: IDisposable;

  constructor(
    readonly id: string,
    private kind: SessionKind,
    private readonly container: HTMLElement,
    onSessionClosed: () => void,
  ) {
    this.term = new Terminal({
      allowProposedApi: true,
      cursorBlink: true,
      fontSize: TERMINAL_FONT_SIZE,
      fontFamily: TERMINAL_FONT_FAMILY,
      theme: terminalTheme(),
    });
    this.term.loadAddon(this.fit);
    this.term.loadAddon(new WebLinksAddon());
    this.term.open(container);
    this.fit.fit();
    registerTerminal(id, this);

    this.suggestEl = document.createElement("div");
    this.suggestEl.className = "term-suggest";
    this.suggestEl.setAttribute("aria-hidden", "true");
    this.suggestEl.hidden = true;
    this.menuEl = document.createElement("div");
    this.menuEl.className = "term-history";
    this.menuEl.hidden = true;
    container.append(this.suggestEl, this.menuEl);
    container.addEventListener("pointerdown", (event) => {
      if (!this.menuEl.contains(event.target as Node)) this.hideOverlays();
    });
    this.term.attachCustomKeyEventHandler((event) => this.handleKey(event));
    this.osc52 = this.term.parser.registerOscHandler(52, (data) =>
      this.handleOsc52(data),
    );

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
      this.hideOverlays();
      this.scheduleKeywords();
      api.resize(this.kind, this.id, cols, rows).catch(() => {});
    });
    const scrollSub = this.term.onScroll(() => this.scheduleKeywords());
    this.unlisten.push(() => scrollSub.dispose());
    this.keywords = useAppStore.getState().keywordHighlights;
    this.scheduleKeywords(true);

    // 先注册事件监听，再发起连接，避免丢失服务端首批输出
    this.ready = Promise.all([
      onOutput(this.id, (bytes) => {
        if (this.disposed) return;
        this.term.write(bytes, () => {
          if (this.disposed) return;
          this.refreshOverlays();
          this.scheduleKeywords();
        });
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
    this.scheduleKeywords(true);
  }

  setKeywords(rules: KeywordRule[]) {
    this.keywords = rules;
    this.scheduleKeywords(true);
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
    let sawEscape = false;
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
        sawEscape = true;
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
    // Cursor movement or escape sequences invalidate the ghost position.
    if (sawEscape) this.hideOverlays();
    else this.refreshOverlays();
  }

  private handleKey(event: KeyboardEvent): boolean {
    if (event.type !== "keydown" || !this.suggestVisible) return true;
    if (event.metaKey || event.ctrlKey || event.altKey) return true;
    if (event.key === "ArrowRight" || event.key === "Tab") {
      event.preventDefault();
      this.acceptSuggestion();
      return false;
    }
    return true;
  }

  private acceptSuggestion() {
    const extra = this.suggestion;
    if (!extra) return;
    this.inputBuffer += extra;
    this.hideOverlays();
    this.insert(extra);
  }

  private refreshOverlays() {
    const input = this.inputBuffer;
    if (!input.trim()) {
      this.hideOverlays();
      return;
    }
    const store = useAppStore.getState();
    if (store.focusedSessionId !== this.id) {
      this.hideOverlays();
      return;
    }
    const session = store.sessions.find((entry) => entry.id === this.id);
    const matches = historyMatches(
      historyForHost(store.commandHistory, this.id, session?.hostId),
      input,
    );
    if (matches.length === 0) {
      this.hideOverlays();
      return;
    }
    const metrics = this.cellMetrics();
    const cell = this.cursorCell();
    const buffer = this.term.buffer.active;
    if (!metrics || !cell || cell.row < 0 || cell.row >= this.term.rows) {
      this.hideOverlays();
      return;
    }
    const line = buffer.getLine(buffer.baseY + cell.row);
    if (!line || line.translateToString(true, cell.col).trim()) {
      this.hideOverlays();
      return;
    }
    this.matches = matches;
    this.suggestion = matches[0].text.slice(input.length);
    this.renderGhost(metrics, cell);
    this.renderMenu(metrics, cell);
  }

  private cellMetrics() {
    const screen = this.container.querySelector(".xterm-screen");
    if (!(screen instanceof HTMLElement)) return null;
    const rect = screen.getBoundingClientRect();
    const base = this.container.getBoundingClientRect();
    if (
      rect.width <= 0 ||
      rect.height <= 0 ||
      !this.term.cols ||
      !this.term.rows
    ) {
      return null;
    }
    return {
      left: rect.left - base.left,
      top: rect.top - base.top,
      cellW: rect.width / this.term.cols,
      cellH: rect.height / this.term.rows,
    };
  }

  private cursorCell() {
    const buffer = this.term.buffer.active;
    return { row: buffer.cursorY - buffer.baseY, col: buffer.cursorX };
  }

  private renderGhost(
    metrics: { left: number; top: number; cellW: number; cellH: number },
    cell: { row: number; col: number },
  ) {
    const fontSize = this.term.options.fontSize;
    this.suggestEl.textContent = this.suggestion;
    if (typeof fontSize === "number") {
      this.suggestEl.style.fontSize = `${fontSize}px`;
    }
    this.suggestEl.style.left = `${metrics.left + cell.col * metrics.cellW}px`;
    this.suggestEl.style.top = `${metrics.top + cell.row * metrics.cellH}px`;
    this.suggestEl.style.lineHeight = `${metrics.cellH}px`;
    this.suggestEl.hidden = false;
    this.suggestVisible = true;
  }

  private renderMenu(
    metrics: { left: number; top: number; cellW: number; cellH: number },
    cell: { row: number; col: number },
  ) {
    this.menuEl.replaceChildren();
    for (const match of this.matches) {
      const item = document.createElement("button");
      item.type = "button";
      item.className = "term-history-item";
      item.textContent = match.text;
      item.addEventListener("mousedown", (event) => event.preventDefault());
      item.addEventListener("click", () => this.replaceLine(match.text));
      this.menuEl.append(item);
    }
    this.menuEl.hidden = false;
    const startCol = Math.max(0, cell.col - this.inputBuffer.length);
    const left = metrics.left + startCol * metrics.cellW;
    const top = metrics.top + (cell.row + 1) * metrics.cellH;
    const maxLeft = Math.max(0, this.container.clientWidth - this.menuEl.offsetWidth);
    const maxTop = Math.max(0, this.container.clientHeight - this.menuEl.offsetHeight);
    this.menuEl.style.left = `${Math.min(left, maxLeft)}px`;
    this.menuEl.style.top = `${Math.min(top, maxTop)}px`;
  }

  private replaceLine(text: string) {
    this.writeChain = this.writeChain
      .then(() => useAppStore.getState().sendInput(this.kind, this.id, "\x15"))
      .then(() => useAppStore.getState().sendInput(this.kind, this.id, text));
    this.inputBuffer = text;
    this.hideOverlays();
    this.focus();
  }

  private hideOverlays() {
    this.suggestion = "";
    this.matches = [];
    this.suggestVisible = false;
    this.suggestEl.hidden = true;
    this.menuEl.hidden = true;
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

  // OSC 52 lets programs (tmux, vim, remote shells) write the local clipboard.
  private handleOsc52(data: string): boolean {
    if (!useAppStore.getState().osc52) return false;
    let payload = data;
    while (payload.length > 1 && payload[1] === ";") {
      if (!/[cps0-7]/.test(payload[0])) break;
      payload = payload.slice(2);
    }
    if (!payload || payload === "?") return true;
    if (payload.length > 1_400_000) return true;
    try {
      const binary = atob(payload);
      const bytes = Uint8Array.from(binary, (ch) => ch.charCodeAt(0));
      const text = new TextDecoder().decode(bytes);
      if (text) {
        void navigator.clipboard.writeText(text).catch(() => {
          useAppStore
            .getState()
            .setStatus(i18n.t("terminal.clipboardFailed"), true);
        });
      }
    } catch {
      // malformed base64 payload: swallow instead of printing it
    }
    return true;
  }

  private scheduleKeywords(immediate = false) {
    window.clearTimeout(this.keywordTimer);
    if (this.keywords.length === 0) {
      this.clearKeywordDecorations();
      return;
    }
    if (immediate) {
      this.renderKeywords();
      return;
    }
    this.keywordTimer = window.setTimeout(() => {
      this.keywordTimer = undefined;
      this.renderKeywords();
    }, 160);
  }

  private clearKeywordDecorations() {
    for (const item of this.keywordDecorations) {
      item.decoration.dispose();
      item.marker.dispose();
    }
    this.keywordDecorations = [];
  }

  private highlightColor(token: string): string {
    const style = getComputedStyle(document.documentElement);
    const pick = (name: string) => style.getPropertyValue(name).trim();
    const background = pick("--bg-base");
    const color = pick(token) || pick("--fg-muted") || pick("--accent");
    const hex = /^#[0-9a-f]{6}$/i;
    if (!hex.test(background) || !hex.test(color)) return color;
    return blendHex(background, color, 0.28);
  }

  private renderKeywords() {
    this.clearKeywordDecorations();
    if (this.disposed) return;
    const buffer = this.term.buffer.active;
    if (buffer.type !== "normal") return;
    const rules = this.keywords
      .filter((rule) => rule.text.trim())
      .map((rule) => ({
        text: rule.text.trim().toLowerCase(),
        color: this.highlightColor(rule.color),
      }));
    if (rules.length === 0) return;
    const cursorLine = buffer.baseY + buffer.cursorY;
    const lastRow = Math.min(buffer.length, buffer.baseY + this.term.rows);
    let count = 0;
    for (
      let row = buffer.baseY;
      row < lastRow && count < 200;
      row += 1
    ) {
      const line = buffer.getLine(row);
      if (!line) continue;
      const text = line.translateToString(true).toLowerCase();
      if (!text) continue;
      let from = 0;
      while (from < text.length && count < 200) {
        let best: { at: number; rule: (typeof rules)[number] } | null = null;
        for (const rule of rules) {
          const at = text.indexOf(rule.text, from);
          if (at < 0) continue;
          if (
            !best ||
            at < best.at ||
            (at === best.at && rule.text.length > best.rule.text.length)
          ) {
            best = { at, rule };
          }
        }
        if (!best) break;
        const marker = this.term.registerMarker(row - cursorLine);
        if (!marker) break;
        let decoration: IDecoration | undefined;
        try {
          decoration = this.term.registerDecoration({
            marker,
            x: best.at,
            width: best.rule.text.length,
            layer: "bottom",
            backgroundColor: best.rule.color,
          });
        } catch {
          decoration = undefined;
        }
        if (decoration) this.keywordDecorations.push({ decoration, marker });
        else marker.dispose();
        from = best.at + best.rule.text.length;
        count += 1;
      }
    }
  }

  dispose() {
    this.disposed = true;
    window.clearTimeout(this.keywordTimer);
    this.clearKeywordDecorations();
    this.osc52.dispose();
    if (useAppStore.getState().focusedSessionId === this.id) {
      useAppStore.getState().setFocusedSession(null);
    }
    unregisterTerminal(this.id, this);
    this.observer.disconnect();
    this.unlisten.forEach((fn) => fn());
    this.suggestEl.remove();
    this.menuEl.remove();
    this.term.dispose();
  }
}