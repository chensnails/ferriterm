import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export const isTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export type SessionKind = "ssh" | "pty";

export interface SshConnectArgs {
  host: string;
  port: number;
  username: string;
  password: string;
  cols: number;
  rows: number;
}

export const api = {
  sshConnect: (id: string, args: SshConnectArgs) =>
    invoke<void>("ssh_connect", { id, args }),
  sshWrite: (id: string, data: string) => invoke<void>("ssh_write", { id, data }),
  sshResize: (id: string, cols: number, rows: number) =>
    invoke<void>("ssh_resize", { id, cols, rows }),
  sshDisconnect: (id: string) => invoke<void>("ssh_disconnect", { id }),

  ptySpawn: (id: string, cols: number, rows: number) =>
    invoke<void>("pty_spawn", { id, cols, rows }),
  ptyWrite: (id: string, data: string) => invoke<void>("pty_write", { id, data }),
  ptyResize: (id: string, cols: number, rows: number) =>
    invoke<void>("pty_resize", { id, cols, rows }),
  ptyKill: (id: string) => invoke<void>("pty_kill", { id }),

  write(kind: SessionKind, id: string, data: string) {
    return kind === "ssh" ? api.sshWrite(id, data) : api.ptyWrite(id, data);
  },
  resize(kind: SessionKind, id: string, cols: number, rows: number) {
    return kind === "ssh"
      ? api.sshResize(id, cols, rows)
      : api.ptyResize(id, cols, rows);
  },
  close(kind: SessionKind, id: string) {
    return kind === "ssh" ? api.sshDisconnect(id) : api.ptyKill(id);
  },
};

export const outputEvent = (id: string) => `terminal://${id}/output`;
export const closedEvent = (id: string) => `terminal://${id}/closed`;

export function onOutput(
  id: string,
  cb: (data: Uint8Array) => void,
): Promise<UnlistenFn> {
  return listen<number[]>(outputEvent(id), (e) => cb(new Uint8Array(e.payload)));
}

export function onClosed(id: string, cb: () => void): Promise<UnlistenFn> {
  return listen(closedEvent(id), () => cb());
}