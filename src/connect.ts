import {
  type HostEntry,
  type JumpHop,
  type SerialConfig,
  type SessionKind,
  type SshConnectArgs,
} from "./api";

export type ConnectArgs = Omit<SshConnectArgs, "cols" | "rows">;

/** What a session needs when its tab mounts. Serial and telnet carry their
    target directly; SSH-family tabs resolve credentials inside Rust. */
export type PendingConnect =
  | { type: "ssh"; args: ConnectArgs }
  | { type: "sftp"; args: ConnectArgs }
  | { type: "pty"; shell?: string; cwd?: string }
  | { type: "serial"; config: SerialConfig }
  | { type: "telnet"; host: string; port: number };

export interface HostConnectPlan {
  kind: SessionKind;
  pending: PendingConnect;
  /** SSH and SFTP first show the credential dialog. */
  needsCredentials: boolean;
}

/** Resolves the jump chain for a host, outermost hop first. */
export function resolveJumpChain(
  host: HostEntry,
  hosts: HostEntry[],
): JumpHop[] {
  const hops: JumpHop[] = [];
  const seen = new Set<string>([host.id]);
  let current = host;
  while (current.jumpHostId) {
    const next = hosts.find((item) => item.id === current.jumpHostId);
    if (!next) {
      throw new Error(`jump host ${current.jumpHostId} was not found`);
    }
    if (seen.has(next.id)) {
      throw new Error("jump chain contains a loop");
    }
    seen.add(next.id);
    const hop: JumpHop = {
      host: next.host,
      port: next.port,
      username: next.username,
      hostId: next.id,
      authMethod: next.authMethod,
    };
    if (next.authMethod === "agent") {
      hop.agent = true;
    } else if (next.authMethod === "keyboard-interactive") {
      hop.keyboardInteractive = true;
    } else if (next.authMethod === "key") {
      if (next.keyId) hop.keyId = next.keyId;
      else if (next.keyPath) hop.keyPath = next.keyPath;
    }
    hops.unshift(hop);
    current = next;
  }
  return hops;
}

/**
 * Builds connection arguments. Stored passwords and passphrases are resolved
 * inside the Rust process from the vault; the webview only names the host and
 * the managed key.
 */
export function buildConnectArgs(
  host: HostEntry,
  hosts: HostEntry[],
  keepalive?: number,
): ConnectArgs {
  const jump = resolveJumpChain(host, hosts);
  const args: ConnectArgs = {
    host: host.host,
    port: host.port,
    username: host.username,
    hostId: host.id,
    authMethod: host.authMethod,
    x11: host.x11,
  };
  if (jump.length > 0) args.jump = jump;
  if (keepalive && keepalive > 0) args.keepalive = keepalive;
  if (host.authMethod === "agent") {
    args.agent = true;
  } else if (host.authMethod === "keyboard-interactive") {
    args.keyboardInteractive = true;
  } else if (host.authMethod === "key") {
    if (host.keyId) args.keyId = host.keyId;
    else if (host.keyPath) args.keyPath = host.keyPath;
  }
  return args;
}

/** Maps a saved host onto the session its protocol opens. */
export function planHostConnect(
  host: HostEntry,
  hosts: HostEntry[],
  keepalive?: number,
): HostConnectPlan {
  const options = host.protocolOptions;
  switch (host.protocol) {
    case "telnet":
      return {
        kind: "telnet",
        pending: { type: "telnet", host: host.host, port: host.port },
        needsCredentials: false,
      };
    case "serial": {
      const config: SerialConfig = {
        path: options.serialPath,
        baudRate: options.baudRate,
        dataBits: options.dataBits,
        parity: options.parity,
        stopBits: options.stopBits,
        flowControl: options.flowControl,
      };
      return {
        kind: "serial",
        pending: { type: "serial", config },
        needsCredentials: false,
      };
    }
    case "local":
      return {
        kind: "pty",
        pending: {
          type: "pty",
          shell: options.shell || undefined,
          cwd: options.cwd || undefined,
        },
        needsCredentials: false,
      };
    case "sftp":
      return {
        kind: "sftp",
        pending: { type: "sftp", args: buildConnectArgs(host, hosts, keepalive) },
        needsCredentials: true,
      };
    default:
      return {
        kind: "ssh",
        pending: { type: "ssh", args: buildConnectArgs(host, hosts, keepalive) },
        needsCredentials: true,
      };
  }
}
