import type {
  SshAlgorithms,
  SshPreset,
  SshSecuritySettings,
} from "./api";

/**
 * SSH algorithm catalogue. Names must match russh 0.65 constants exactly
 * (`TryFrom<&str>` is used on the Rust side; unknown names are dropped).
 * Insecure `none` / `clear` cipher and MAC names are deliberately not
 * offered. `3des-cbc` is omitted because russh's `des` feature is disabled
 * in this build, so that name cannot actually be negotiated. KEX extension
 * pseudo-algorithms are never user-facing — ft-ssh re-appends them to the
 * KEXINIT proposal automatically.
 */
export const SSH_ALGORITHM_GROUPS = [
  {
    key: "kex",
    labelKey: "settings.sshGroupKex",
    items: [
      "mlkem768x25519-sha256",
      "curve25519-sha256",
      "curve25519-sha256@libssh.org",
      "ecdh-sha2-nistp256",
      "ecdh-sha2-nistp384",
      "ecdh-sha2-nistp521",
      "diffie-hellman-group-exchange-sha256",
      "diffie-hellman-group14-sha256",
      "diffie-hellman-group15-sha512",
      "diffie-hellman-group16-sha512",
      "diffie-hellman-group17-sha512",
      "diffie-hellman-group18-sha512",
      "diffie-hellman-group-exchange-sha1",
      "diffie-hellman-group14-sha1",
      "diffie-hellman-group1-sha1",
    ],
  },
  {
    key: "cipher",
    labelKey: "settings.sshGroupCipher",
    items: [
      "chacha20-poly1305@openssh.com",
      "aes256-gcm@openssh.com",
      "aes128-gcm@openssh.com",
      "aes256-ctr",
      "aes192-ctr",
      "aes128-ctr",
      "aes256-cbc",
      "aes192-cbc",
      "aes128-cbc",
    ],
  },
  {
    key: "mac",
    labelKey: "settings.sshGroupMac",
    items: [
      "hmac-sha2-512-etm@openssh.com",
      "hmac-sha2-256-etm@openssh.com",
      "hmac-sha2-512",
      "hmac-sha2-256",
      "hmac-sha1-etm@openssh.com",
      "hmac-sha1",
    ],
  },
  {
    key: "hostKey",
    labelKey: "settings.sshGroupHostKey",
    items: [
      "ssh-ed25519",
      "rsa-sha2-512",
      "rsa-sha2-256",
      "ecdsa-sha2-nistp256",
      "ecdsa-sha2-nistp384",
      "ecdsa-sha2-nistp521",
      "ssh-rsa",
    ],
  },
  {
    key: "compression",
    labelKey: "settings.sshGroupCompression",
    items: ["none", "zlib", "zlib@openssh.com"],
  },
] as const satisfies readonly {
  key: keyof SshAlgorithms;
  labelKey: string;
  items: readonly string[];
}[];

export type SshAlgorithmGroupKey = keyof SshAlgorithms;

/** Effective russh defaults, mirrored here only for the advanced UI preview. */
export const SSH_RECOMMENDED: SshAlgorithms = {
  kex: [
    "mlkem768x25519-sha256",
    "curve25519-sha256",
    "curve25519-sha256@libssh.org",
    "diffie-hellman-group-exchange-sha256",
    "diffie-hellman-group18-sha512",
    "diffie-hellman-group17-sha512",
    "diffie-hellman-group16-sha512",
    "diffie-hellman-group15-sha512",
    "diffie-hellman-group14-sha256",
  ],
  cipher: [
    "chacha20-poly1305@openssh.com",
    "aes256-gcm@openssh.com",
    "aes256-ctr",
    "aes192-ctr",
    "aes128-ctr",
  ],
  mac: [
    "hmac-sha2-512-etm@openssh.com",
    "hmac-sha2-256-etm@openssh.com",
    "hmac-sha2-512",
    "hmac-sha2-256",
  ],
  hostKey: [
    "ssh-ed25519",
    "ecdsa-sha2-nistp256",
    "ecdsa-sha2-nistp384",
    "ecdsa-sha2-nistp521",
    "rsa-sha2-512",
    "rsa-sha2-256",
    "ssh-rsa",
  ],
  compression: ["none"],
};

/** Old appliances: adds SHA-1 KEX, CBC ciphers, SHA-1 MACs and zlib. */
export const SSH_COMPATIBLE: SshAlgorithms = {
  kex: [
    ...SSH_RECOMMENDED.kex,
    "ecdh-sha2-nistp256",
    "ecdh-sha2-nistp384",
    "ecdh-sha2-nistp521",
    "diffie-hellman-group-exchange-sha1",
    "diffie-hellman-group14-sha1",
    "diffie-hellman-group1-sha1",
  ],
  cipher: [
    ...SSH_RECOMMENDED.cipher,
    "aes128-gcm@openssh.com",
    "aes256-cbc",
    "aes192-cbc",
    "aes128-cbc",
  ],
  mac: [
    ...SSH_RECOMMENDED.mac,
    "hmac-sha1-etm@openssh.com",
    "hmac-sha1",
  ],
  hostKey: SSH_RECOMMENDED.hostKey,
  compression: ["none", "zlib@openssh.com", "zlib"],
};

/** Modern only: AEAD ciphers, ETM MACs, PQ/curve25519 KEX, no compression. */
export const SSH_STRICT: SshAlgorithms = {
  kex: ["mlkem768x25519-sha256", "curve25519-sha256"],
  cipher: ["chacha20-poly1305@openssh.com", "aes256-gcm@openssh.com"],
  mac: ["hmac-sha2-512-etm@openssh.com", "hmac-sha2-256-etm@openssh.com"],
  hostKey: ["ssh-ed25519", "rsa-sha2-512", "rsa-sha2-256"],
  compression: ["none"],
};

export const SSH_PRESETS: Record<
  Exclude<SshPreset, "custom">,
  SshAlgorithms
> = {
  recommended: SSH_RECOMMENDED,
  compatible: SSH_COMPATIBLE,
  strict: SSH_STRICT,
};

export const DEFAULT_SSH_SECURITY: SshSecuritySettings = {
  preset: "recommended",
  ...SSH_RECOMMENDED,
};

const GROUP_KEYS: SshAlgorithmGroupKey[] = [
  "kex",
  "cipher",
  "mac",
  "hostKey",
  "compression",
];

/**
 * Drops unknown/duplicate names so persisted or imported data stays safe.
 * Catalogue order is preserved because it is preference-ranked (strongest
 * first) — the advanced UI uses checkboxes, so there is no user ordering.
 */
function cleanList(group: SshAlgorithmGroupKey, input: unknown): string[] {
  const allowed: readonly string[] = SSH_ALGORITHM_GROUPS.find(
    (item) => item.key === group,
  )!.items;
  if (!Array.isArray(input)) return [];
  const selected = new Set(
    input.filter((item): item is string => typeof item === "string"),
  );
  return allowed.filter((name) => selected.has(name));
}

/** Validates a stored settings blob with a complete fallback for every field. */
export function parseSshSecurity(raw: string | undefined): SshSecuritySettings {
  if (!raw) return { ...DEFAULT_SSH_SECURITY };
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return { ...DEFAULT_SSH_SECURITY };
  }
  if (!parsed || typeof parsed !== "object") {
    return { ...DEFAULT_SSH_SECURITY };
  }
  const value = parsed as Record<string, unknown>;
  const preset: SshPreset =
    value.preset === "compatible" ||
    value.preset === "strict" ||
    value.preset === "custom"
      ? value.preset
      : "recommended";
  const base: SshAlgorithms =
    preset === "custom" ? SSH_RECOMMENDED : SSH_PRESETS[preset];
  const result: SshSecuritySettings = { ...base, preset };
  for (const key of GROUP_KEYS) {
    const list = cleanList(key, value[key]);
    // An empty group would silently fall back to library defaults; keep the
    // preset's own list in that case so the UI and the wire agree.
    result[key] = list.length > 0 ? list : [...base[key]];
  }
  return result;
}

/**
 * `recommended` deliberately omits the payload so russh keeps its own safe
 * defaults — no behaviour change versus releases without this setting.
 */
export function connectAlgorithms(
  settings: SshSecuritySettings,
): SshAlgorithms | undefined {
  if (settings.preset === "recommended") return undefined;
  return {
    kex: [...settings.kex],
    cipher: [...settings.cipher],
    mac: [...settings.mac],
    hostKey: [...settings.hostKey],
    compression: [...settings.compression],
  };
}
