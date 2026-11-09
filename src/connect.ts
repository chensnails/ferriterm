import { type HostEntry, type JumpHop, type SshConnectArgs } from "./api";

export type ConnectArgs = Omit<SshConnectArgs, "cols" | "rows">;

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
