import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import type { CastEvent } from "./api";
import { TERMINAL_FONT_FAMILY, TERMINAL_FONT_SIZE } from "./terminal";
import { terminalTheme } from "./theme";

export interface PlayerState {
  position: number;
  duration: number;
  playing: boolean;
}

// Replays asciicast v2 output events at their recorded pace. Only server
// output is stored by the recorder, so the replay is read-only by design.
export class CastPlayer {
  readonly term: Terminal;
  private timer: number | null = null;
  private speed = 1;
  private position = 0;
  private index = 0;
  private basePosition = 0;
  private startedAt = 0;
  private disposed = false;

  constructor(
    container: HTMLElement,
    cols: number,
    rows: number,
    private readonly events: CastEvent[],
    readonly duration: number,
    private readonly onState: (state: PlayerState) => void,
  ) {
    this.term = new Terminal({
      cols,
      rows,
      disableStdin: true,
      cursorBlink: false,
      fontSize: TERMINAL_FONT_SIZE,
      fontFamily: TERMINAL_FONT_FAMILY,
      theme: terminalTheme(),
    });
    this.term.open(container);
    this.seek(0);
  }

  play() {
    if (this.disposed || this.events.length === 0) return;
    if (this.position >= this.duration) this.seek(0);
    if (this.timer !== null) return;
    this.basePosition = this.position;
    this.startedAt = performance.now();
    this.timer = window.setInterval(() => this.tick(), 80);
    this.emit();
  }

  pause() {
    if (this.timer === null) return;
    this.position = this.currentPosition();
    window.clearInterval(this.timer);
    this.timer = null;
    this.emit();
  }

  toggle() {
    if (this.timer === null) this.play();
    else this.pause();
  }

  setSpeed(speed: number) {
    this.position = this.currentPosition();
    this.basePosition = this.position;
    this.startedAt = performance.now();
    this.speed = speed;
    this.emit();
  }

  restart() {
    this.seek(0);
    this.play();
  }

  seek(seconds: number) {
    if (this.disposed) return;
    const target = Math.min(Math.max(0, seconds), this.duration);
    this.term.reset();
    this.index = 0;
    while (
      this.index < this.events.length &&
      this.events[this.index].time <= target
    ) {
      this.term.write(this.events[this.index].data);
      this.index += 1;
    }
    this.position = target;
    this.basePosition = target;
    this.startedAt = performance.now();
    this.emit();
  }

  dispose() {
    this.disposed = true;
    if (this.timer !== null) window.clearInterval(this.timer);
    this.timer = null;
    this.term.dispose();
  }

  private currentPosition() {
    if (this.timer === null) return this.position;
    const elapsed = (performance.now() - this.startedAt) / 1000;
    return Math.min(this.duration, this.basePosition + elapsed * this.speed);
  }

  private tick() {
    const now = this.currentPosition();
    while (
      this.index < this.events.length &&
      this.events[this.index].time <= now
    ) {
      this.term.write(this.events[this.index].data);
      this.index += 1;
    }
    this.position = now;
    if (this.index >= this.events.length) {
      this.position = this.duration;
      if (this.timer !== null) window.clearInterval(this.timer);
      this.timer = null;
    }
    this.emit();
  }

  private emit() {
    if (this.disposed) return;
    this.onState({
      position: this.position,
      duration: this.duration,
      playing: this.timer !== null,
    });
  }
}
