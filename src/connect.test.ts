import { describe, expect, it } from "vitest";

import {
  isAuthFailure,
  stripPendingSecrets,
  type PendingConnect,
} from "./connect";

const sshPending: PendingConnect = {
  type: "ssh",
  args: {
    host: "example.test",
    port: 22,
    username: "root",
    hostId: "host-1",
    authMethod: "password",
    x11: false,
    encoding: "utf-8",
    password: "hunter2",
    keyPassphrase: "secret",
  },
  saveCredential: {
    hostId: "host-1",
    credentialId: "cred-1",
    name: "Prod",
    username: "root",
    password: "hunter2",
  },
  saveSecret: { secretRef: "pp:host-1", secret: "secret" },
};

describe("isAuthFailure", () => {
  it("recognizes rejected or missing credentials", () => {
    expect(isAuthFailure("authentication failed")).toBe(true);
    expect(isAuthFailure("Permission denied (publickey,password).")).toBe(true);
    expect(isAuthFailure("credentials are required")).toBe(true);
    expect(isAuthFailure("the credential vault is locked")).toBe(true);
  });

  it("leaves transport errors retryable", () => {
    expect(isAuthFailure("connection refused")).toBe(false);
    expect(isAuthFailure("connection timed out")).toBe(false);
  });
});

describe("stripPendingSecrets", () => {
  it("drops one-shot secrets and deferred saves", () => {
    const stripped = stripPendingSecrets(sshPending);
    expect(stripped).toBeDefined();
    expect(stripped?.type).toBe("ssh");
    if (stripped?.type !== "ssh") return;
    expect(stripped.args.password).toBeUndefined();
    expect(stripped.args.keyPassphrase).toBeUndefined();
    expect(stripped.args.hostId).toBe("host-1");
    expect(stripped.saveCredential).toBeUndefined();
    expect(stripped.saveSecret).toBeUndefined();
  });

  it("does not mutate the original pending connection", () => {
    stripPendingSecrets(sshPending);
    if (sshPending.type !== "ssh") return;
    expect(sshPending.args.password).toBe("hunter2");
    expect(sshPending.saveCredential?.credentialId).toBe("cred-1");
  });

  it("passes non-SSH connections through unchanged", () => {
    const telnet: PendingConnect = { type: "telnet", host: "a", port: 23 };
    expect(stripPendingSecrets(telnet)).toBe(telnet);
  });

  it("maps an absent pending connection to undefined", () => {
    expect(stripPendingSecrets(null)).toBeUndefined();
    expect(stripPendingSecrets(undefined)).toBeUndefined();
  });
});
