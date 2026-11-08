import { api, type HostEntry, type JumpHop, type SshConnectArgs } from "./api";

export type ConnectArgs = Omit<SshConnectArgs, "cols" | "rows">;

export interface HostSecret {
  password?: string;
  keyPath?: string;
  passphrase?: string;
}

/** Resolves the jump chain for a host, outermost hop first. */
export async function resolveJumpChain(
  host: HostEntry,
  hosts: HostEntry[],
): Promise<JumpHop[]> {
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
    const secret = await api.getCredential(next.id).catch(() => null);
    const hop: JumpHop = {
      host: next.host,
      port: next.port,
      username: next.username,
    };
    if (next.authMethod === "agent") {
      hop.agent = true;
    } else if (next.authMethod === "keyboard-interactive") {
      hop.keyboardInteractive = true;
    } else if (next.authMethod === "key" && next.keyPath) {
      hop.privateKey = next.keyPath;
      if (secret) hop.keyPassphrase = secret;
    } else if (secret) {
      hop.password = secret;
    } else {
      throw new Error(
        `jump host ${next.name || next.host} has no saved credential; connect to it once and remember the secret`,
      );
    }
    hops.unshift(hop);
    current = next;
  }
  return hops;
}

export async function buildConnectArgs(
  host: HostEntry,
  hosts: HostEntry[],
  secret: HostSecret = {},
  keepalive?: number,
): Promise<ConnectArgs> {
  const jump = await resolveJumpChain(host, hosts);
  const args: ConnectArgs = {
    host: host.host,
    port: host.port,
    username: host.username,
    hostId: host.id,
    x11: host.x11,
  };
  if (jump.length > 0) args.jump = jump;
  if (keepalive && keepalive > 0) args.keepalive = keepalive;
  if (host.authMethod === "agent") {
    args.agent = true;
  } else if (host.authMethod === "keyboard-interactive") {
    args.keyboardInteractive = true;
  } else if (host.authMethod === "key") {
    args.privateKey = secret.keyPath ?? host.keyPath;
    if (secret.passphrase) args.keyPassphrase = secret.passphrase;
  } else {
    args.password = secret.password;
  }
  return args;
}
