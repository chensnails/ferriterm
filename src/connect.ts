import {
  type HostEntry,
  type JumpHop,
  type SerialConfig,
  type SessionKind,
  type SshConnectArgs,
} from "./api";

export type ConnectArgs = Omit<SshConnectArgs, "cols" | "rows">;

/** A password typed in the connect dialog, kept out of the vault until the
    transport has actually authenticated with it. */
export interface PendingCredentialSave {
  hostId: string;
  credentialId: string;
  name: string;
  username: string;
  password: string;
}

/** Key passphrase typed for a connection, stored only after success. */
export interface PendingSecretSave {
  secretRef: string;
  secret: string;
}

/** What a session needs when its tab mounts. Serial and telnet carry their
    target directly; SSH-family tabs resolve credentials inside Rust. */
export type PendingConnect =
  | {
      type: "ssh";
      args: ConnectArgs;
      saveCredential?: PendingCredentialSave;
      saveSecret?: PendingSecretSave;
    }
  | {
      type: "sftp";
      args: ConnectArgs;
      saveCredential?: PendingCredentialSave;
      saveSecret?: PendingSecretSave;
    }
  | { type: "pty"; shell?: string; cwd?: string }
  | { type: "serial"; config: SerialConfig }
  | { type: "telnet"; host: string; port: number };

/** Auth failures must stop silent retries: the user has to supply a credential
    in the connect dialog instead of replaying the secret that just failed. */
export function isAuthFailure(error: string): boolean {
  return /authentication failed|credentials are required|permission denied|credential vault is locked/i.test(
    error,
  );
}

/** Drops one-shot secrets and deferred saves from a pending connection. Used
    after a connect attempt so reconnects and host deletion cannot replay a
    password that was never verified. */
export function stripPendingSecrets(
  pending: PendingConnect | null | undefined,
): PendingConnect | undefined {
  if (!pending) return undefined;
  if (pending.type !== "ssh" && pending.type !== "sftp") return pending;
  const { password: _password, keyPassphrase: _keyPassphrase, ...args } =
    pending.args;
  return { ...pending, args, saveCredential: undefined, saveSecret: undefined };
}

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
    encoding: host.protocolOptions.encoding ?? "utf-8",
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
