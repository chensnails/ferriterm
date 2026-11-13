import { Channel, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export const isTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export type SessionKind = "ssh" | "pty" | "serial" | "telnet" | "sftp";

export type HostProtocol = "ssh" | "sftp" | "telnet" | "serial" | "local";

export interface ProtocolOptions {
  serialPath: string;
  baudRate: number;
  dataBits: number;
  parity: SerialParity;
  stopBits: number;
  flowControl: SerialFlowControl;
  shell: string;
  cwd: string;
  encoding: string;
}

export const DEFAULT_PROTOCOL_OPTIONS: ProtocolOptions = {
  serialPath: "",
  baudRate: 115_200,
  dataBits: 8,
  parity: "none",
  stopBits: 1,
  flowControl: "none",
  shell: "",
  cwd: "",
  encoding: "utf-8",
};

export interface Tag {
  id: string;
  name: string;
  color: string;
}

export interface HostEntry {
  id: string;
  name: string;
  host: string;
  port: number;
  username: string;
  authMethod: string;
  keyPath: string;
  keyId: string;
  /** Named password credential resolved inside Rust; empty = ask on connect. */
  passwordId: string;
  jumpHostId: string;
  x11: boolean;
  parentId: string | null;
  isFolder: boolean;
  production: boolean;
  protocol: HostProtocol;
  protocolOptions: ProtocolOptions;
  icon: string;
  sortOrder: number;
  notes: string;
  tags: Tag[];
}

export interface HostInput {
  id: string;
  name: string;
  host: string;
  port: number;
  username: string;
  authMethod: string;
  keyPath: string;
  keyId: string;
  passwordId: string;
  jumpHostId: string;
  x11: boolean;
  parentId: string | null;
  isFolder: boolean;
  production: boolean;
  protocol: HostProtocol;
  protocolOptions: ProtocolOptions;
  icon: string;
  notes: string;
  tagIds: string[];
}

export interface JumpHop {
  host: string;
  port: number;
  username: string;
  hostId?: string;
  keyId?: string;
  keyPath?: string;
  authMethod?: string;
  agent?: boolean;
  keyboardInteractive?: boolean;
}

/** Algorithm name lists sent to russh. Empty group = keep library default. */
export interface SshAlgorithms {
  kex: string[];
  cipher: string[];
  mac: string[];
  hostKey: string[];
  compression: string[];
}

export type SshPreset = "recommended" | "compatible" | "strict" | "custom";

export interface SshSecuritySettings extends SshAlgorithms {
  preset: SshPreset;
}

export interface SshConnectArgs {
  host: string;
  port: number;
  username: string;
  /** Ephemeral only; stored secrets are resolved by the Rust process. */
  password?: string;
  keyPath?: string;
  keyPassphrase?: string;
  keyId?: string;
  authMethod?: string;
  agent?: boolean;
  keyboardInteractive?: boolean;
  x11?: boolean;
  jump?: JumpHop[];
  keepalive?: number;
  hostId?: string;
  /** Global SSH algorithm policy; omitted when using russh's safe defaults. */
  algorithms?: SshAlgorithms;
  /** Charset for shell output and SFTP names; UTF-8 when omitted. */
  encoding?: string;
  cols: number;
  rows: number;
}

export type VaultMode = "local" | "master";

export interface UpdateInfo {
  currentVersion: string;
  version: string;
  notes: string;
  available: boolean;
  size: number | null;
}

export interface UpdateProgress {
  received: number;
  total: number | null;
}

export interface VaultStatus {
  mode: VaultMode;
  locked: boolean;
  entries: number;
}

export interface VaultEntryView {
  secretRef: string;
  kind: string;
  label: string;
  hostId: string | null;
  updatedAt: string;
}

export interface CredentialStatus {
  password: boolean;
  passphrase: boolean;
  key: boolean;
  keyPassphrase: boolean;
  locked: boolean;
}

export interface SshKey {
  id: string;
  name: string;
  algorithm: string;
  fingerprint: string;
  publicKey: string;
  comment: string;
  sourcePath: string;
  hasPassphrase: boolean;
  createdAt: string;
  updatedAt: string;
  /** Operator note; metadata only, never sent to a remote host. */
  notes: string;
  /** Optional ISO date (YYYY-MM-DD) used for rotation reminders. */
  expiresAt: string;
}

export interface KeyCandidate {
  path: string;
  name: string;
  algorithm: string;
  fingerprint: string;
  comment: string;
  encrypted: boolean;
}

export interface PasswordCredential {
  id: string;
  name: string;
  username: string;
  notes: string;
  createdAt: string;
  updatedAt: string;
  /** Whether the vault currently holds a password for this credential. */
  hasSecret: boolean;
}

export interface KnownHostEntry {
  host: string;
  port: number;
  algorithm: string;
  fingerprint: string;
  /** `host` is an opaque hash token because the line is hashed. */
  hashed: boolean;
}

export interface AuditRecord {
  id: number;
  at: string;
  action: string;
  target: string;
  detail: string;
}

export type ImportFormat = "ssh_config" | "xshell" | "finalshell";

export interface RecordingMeta {
  name: string;
  title: string;
  timestamp: number;
  size: number;
  width: number;
  height: number;
  duration: number;
}

/** A plain-text session log, the default capture of every session. */
export interface SessionLogMeta {
  name: string;
  title: string;
  timestamp: number;
  size: number;
  lines: number;
}

/** What a session writes to disk: a text log, an asciicast replay, or both. */
export type CaptureMode = "log" | "record" | "both";

export interface CapturePaths {
  log: string | null;
  cast: string | null;
}

export interface CastEvent {
  time: number;
  data: string;
}

export interface CastFile {
  header: { width: number; height: number; timestamp: number; title: string };
  events: CastEvent[];
  duration: number;
}

export interface ImportedHost {
  name: string;
  host: string;
  port: number;
  username: string;
}

export interface ImportPreview {
  format: ImportFormat;
  hosts: ImportedHost[];
}

export interface GpuInfo {
  name: string;
  utilization: number | null;
  memoryUsed: number | null;
  memoryTotal: number | null;
}

/** One mounted filesystem; `used` already excludes reserved blocks. */
export interface DiskUsage {
  mount: string;
  used: number;
  total: number;
}

/** Bytes per second over the last sampling interval. */
export interface NetInterface {
  name: string;
  rxRate: number;
  txRate: number;
  /** Bytes since boot; used for the tooltip totals and to rank adapters. */
  rxTotal: number;
  txTotal: number;
  /** The adapter that carries this host's traffic — the only one shown in
   *  compact readouts. */
  primary: boolean;
}

export interface ProcessInfo {
  pid: number;
  name: string;
  /** Percent of one core. */
  cpu: number;
  mem: number;
}

export interface MonitorSnapshot {
  cpuUsage: number;
  cpuBrand: string;
  cpuCores: number;
  memUsed: number;
  memTotal: number;
  swapUsed: number;
  swapTotal: number;
  load1: number;
  load5: number;
  load15: number;
  uptimeSecs: number;
  hostName: string;
  osName: string;
  kernelVersion: string;
  gpus: GpuInfo[];
  disks: DiskUsage[];
  net: NetInterface[];
  processes: ProcessInfo[];
}

/** A remote Linux host sampled over an open SSH session. GPU probing is not
 *  attempted remotely, so there is no brand or GPU list here. */
export interface RemoteSnapshot {
  cpuUsage: number;
  cpuCores: number;
  memUsed: number;
  memTotal: number;
  swapUsed: number;
  swapTotal: number;
  load1: number;
  load5: number;
  load15: number;
  uptimeSecs: number;
  hostName: string;
  osName: string;
  kernelVersion: string;
  disks: DiskUsage[];
  net: NetInterface[];
  processes: ProcessInfo[];
  /** Round trip of the probe itself. */
  latencyMs: number;
}

export interface CommandItem {
  id: string;
  groupId: string;
  name: string;
  template: string;
}

export interface CommandGroup {
  id: string;
  name: string;
  commands: CommandItem[];
}

export interface KeyBundleStats {
  imported: number;
  skipped: number;
}

export interface ImportStats {
  hostsAdded: number;
  hostsSkipped: number;
  tagsAdded: number;
  hostsUpdated: number;
  hostsRemoved: number;
  secretsImported: number;
  sshKeysImported: number;
  macrosImported: number;
  forwardRulesImported: number;
  settingsApplied: number;
}

/** The five data types a backup bundle can carry. */
export type SyncScope =
  | "hosts"
  | "secrets"
  | "highlights"
  | "macros"
  | "shortcuts";

export const SYNC_SCOPES: SyncScope[] = [
  "hosts",
  "secrets",
  "highlights",
  "macros",
  "shortcuts",
];

export type RemoteConfig =
  | { kind: "gist"; token: string; gistId: string; filename: string }
  | {
      kind: "webdav";
      url: string;
      username: string;
      password: string;
      filename: string;
    };

export interface RemoteUpload {
  location: string;
  bytes: number;
  id: string;
}

/** An unattended or manual upload either landed or was refused because the
 * remote bundle changed on another device. */
export type UploadOutcome =
  | ({ state: "uploaded" } & RemoteUpload)
  | { state: "conflict" };

/** Per-type counts carried by a version record (no decryption needed). */
export interface BackupSummary {
  hosts: number;
  tags: number;
  secrets: number;
  sshKeys: number;
  macros: number;
  highlights: number;
  shortcuts: number;
}

export type VersionSource =
  | "local"
  | "webdav"
  | "gist"
  | "restorePoint";

export interface VersionEntry {
  id: string;
  createdAt: number;
  scope: SyncScope[];
  summary: BackupSummary;
  source: VersionSource;
  bytes: number;
  note: string;
}

export interface VersionRestoreResult {
  stats: ImportStats;
  restorePointId: string;
  restorePointAt: number;
}

export interface RemoteMountInfo {
  id: string;
  label: string;
  kind: string;
}

export type RemoteMountConfig =
  | { kind: "webdav"; url: string; username: string; password: string }
  | {
      kind: "s3";
      endpoint: string;
      region: string;
      bucket: string;
      accessKey: string;
      secretKey: string;
      prefix: string;
    }
  | {
      kind: "ftp";
      host: string;
      port: number | null;
      username: string;
      password: string;
      /** Directory that `/` maps to; empty means the login directory. */
      path: string;
    };

export interface LanguagePack {
  id: string;
  name: string;
  version: string;
  kind: string;
  locale: string;
  strings: Record<string, string>;
}

export interface ThemePack {
  id: string;
  name: string;
  version: string;
  kind: string;
  base: string;
  tokens: Record<string, string>;
}

export interface CommandPackCommand {
  name: string;
  template: string;
}

export interface CommandPackGroup {
  name: string;
  commands: CommandPackCommand[];
}

export interface CommandPack {
  id: string;
  name: string;
  version: string;
  kind: string;
  groups: CommandPackGroup[];
}

export interface PluginManifest {
  id: string;
  name: string;
  version: string;
  kind: string;
  apiVersion: number;
  description: string;
  permissions: string[];
  entry: string;
}

export interface BuiltinPlugin {
  id: string;
  name: string;
  version: string;
  kind: string;
  description: string;
  permission: string;
}

export interface PluginScan {
  plugins: PluginManifest[];
  packs: LanguagePack[];
  themes: ThemePack[];
  commandPacks: CommandPack[];
  errors: string[];
  builtin: BuiltinPlugin[];
}

export interface MarketEntry {
  id: string;
  name: string;
  version: string;
  kind: string;
  description: string;
  author: string;
  homepage: string;
  manifestUrl: string;
}

export interface MarketIndex {
  apiVersion: number;
  updated: string;
  plugins: MarketEntry[];
}

export type SerialParity = "none" | "odd" | "even";
export type SerialFlowControl = "none" | "rtscts" | "xonxoff";

export interface SerialConfig {
  path: string;
  baudRate: number;
  dataBits: number;
  parity: SerialParity;
  stopBits: number;
  flowControl: SerialFlowControl;
}

export interface SerialPortInfo {
  path: string;
  name: string;
}

export interface WasmRunResult {
  output: string;
  logs: string[];
  fuelUsed: number;
}

export interface HostKeyChallenge {
  requestId: number;
  sessionId: string;
  host: string;
  port: number;
  fingerprint: string;
  algorithm: string;
  changed: boolean;
}

export interface KeyboardPromptInfo {
  text: string;
  echo: boolean;
}

export interface KeyboardInteractiveChallenge {
  requestId: number;
  sessionId: string;
  name: string;
  instructions: string;
  prompts: KeyboardPromptInfo[];
}

export type ForwardKind = "local" | "remote" | "dynamic";

export interface ForwardRequest {
  id: string;
  sessionId: string;
  kind?: ForwardKind;
  bindHost?: string;
  bindPort: number;
  destHost?: string;
  destPort?: number;
}

export interface ForwardInfo {
  id: string;
  sessionId: string;
  kind: ForwardKind;
  bindHost: string;
  bindPort: number;
  destHost: string;
  destPort: number;
  connections: number;
  bytesIn: number;
  bytesOut: number;
}

export interface ForwardRule {
  id: string;
  hostId: string;
  kind: ForwardKind;
  bindHost: string;
  bindPort: number;
  destHost: string;
  destPort: number;
}

export interface VfsEntry {
  name: string;
  path: string;
  size: number;
  mode: number;
  uid: number;
  gid: number;
  mtime: number;
  isDir: boolean;
  isSymlink: boolean;
  /** Present when the host let us read `/etc/passwd`; otherwise show the ids. */
  owner?: string | null;
  group?: string | null;
}

export interface TextFileContent {
  content: string;
  mtime: number;
  size: number;
  truncated: boolean;
}

export type TransferStatus =
  | "pending"
  | "running"
  | "paused"
  | "done"
  | "failed"
  | "cancelled";

export interface TransferJob {
  id: string;
  name: string;
  sourcePath: string;
  targetPath: string;
  size: number;
  transferred: number;
  status: TransferStatus;
  error: string | null;
  speed: number;
  etaSeconds: number | null;
  isDirectory: boolean;
  filesDone: number;
  filesTotal: number;
  /** Entries the copy deliberately left behind (symbolic links). */
  skipped: number;
  /** Both ends are remote: the bytes pass through this machine. */
  relay: boolean;
}

export interface TransferRequest {
  id: string;
  sourceSession?: string | null;
  sourcePath: string;
  targetSession?: string | null;
  targetPath: string;
  directory?: boolean;
}

export type SyncActionKind =
  | "createDir"
  | "copyFile"
  | "deleteFile"
  | "deleteDir";

/** Which tree an action writes to. One-way sync only writes to the target. */
export type SyncSide = "target" | "source";

export interface SyncAction {
  kind: SyncActionKind;
  side: SyncSide;
  path: string;
  size: number;
}

export type ConflictReason = "contentDiffers" | "typeMismatch";

export interface SyncConflict {
  path: string;
  reason: ConflictReason;
  sourceSize: number;
  sourceMtime: number;
  targetSize: number;
  targetMtime: number;
}

export interface SyncPlan {
  sourceRoot: string;
  targetRoot: string;
  twoWay: boolean;
  actions: SyncAction[];
  upFiles: number;
  upBytes: number;
  downFiles: number;
  downBytes: number;
  createDirs: number;
  deleteEntries: number;
  conflicts: SyncConflict[];
}

export const api = {
  sshConnect: (id: string, args: SshConnectArgs) =>
    invoke<void>("ssh_connect", { id, args }),
  sshWrite: (id: string, data: string) => invoke<void>("ssh_write", { id, data }),
  sshResize: (id: string, cols: number, rows: number) =>
    invoke<void>("ssh_resize", { id, cols, rows }),
  sshDisconnect: (id: string) => invoke<void>("ssh_disconnect", { id }),
  sshHostKeyRespond: (requestId: number, accept: boolean, remember: boolean) =>
    invoke<boolean>("ssh_host_key_respond", { requestId, accept, remember }),
  sshKeyboardInteractiveRespond: (requestId: number, responses: string[] | null) =>
    invoke<boolean>("ssh_keyboard_interactive_respond", { requestId, responses }),

  forwardStart: (request: ForwardRequest) =>
    invoke<ForwardInfo>("forward_start", { request }),
  forwardStop: (id: string) => invoke<boolean>("forward_stop", { id }),
  forwardList: () => invoke<ForwardInfo[]>("forward_list"),
  listForwardRules: (hostId?: string) =>
    invoke<ForwardRule[]>("list_forward_rules", { hostId }),
  saveForwardRule: (rule: ForwardRule) =>
    invoke<void>("save_forward_rule", { rule }),
  deleteForwardRule: (id: string) =>
    invoke<void>("delete_forward_rule", { id }),

  vfsList: (
    sessionId: string | null,
    path: string,
    showHidden: boolean,
  ) => invoke<VfsEntry[]>("vfs_list", { sessionId, path, showHidden }),
  vfsStat: (sessionId: string | null, path: string) =>
    invoke<VfsEntry>("vfs_stat", { sessionId, path }),
  vfsHome: (sessionId: string | null) =>
    invoke<string>("vfs_home", { sessionId }),
  vfsMkdir: (sessionId: string | null, path: string) =>
    invoke<void>("vfs_mkdir", { sessionId, path }),
  vfsRemove: (sessionId: string | null, path: string, recursive: boolean) =>
    invoke<void>("vfs_remove", { sessionId, path, recursive }),
  vfsRename: (sessionId: string | null, from: string, to: string) =>
    invoke<void>("vfs_rename", { sessionId, from, to }),
  vfsChmod: (sessionId: string | null, path: string, mode: number) =>
    invoke<void>("vfs_chmod", { sessionId, path, mode }),
  vfsReadText: (sessionId: string | null, path: string, maxBytes?: number) =>
    invoke<TextFileContent>("vfs_read_text", { sessionId, path, maxBytes }),
  vfsWriteText: (
    sessionId: string | null,
    path: string,
    content: string,
    expectedMtime: number | null,
  ) =>
    invoke<number>("vfs_write_text", {
      sessionId,
      path,
      content,
      expectedMtime,
    }),

  transferEnqueue: (request: TransferRequest) =>
    invoke<string>("transfer_enqueue", { request }),
  transferList: () => invoke<TransferJob[]>("transfer_list"),
  setTransferConcurrency: (concurrency: number) =>
    invoke<number>("transfer_set_concurrency", { concurrency }),
  transferPause: (id: string) => invoke<void>("transfer_pause", { id }),
  transferResume: (id: string) => invoke<void>("transfer_resume", { id }),
  transferCancel: (id: string) => invoke<void>("transfer_cancel", { id }),
  transferRetry: (id: string) => invoke<void>("transfer_retry", { id }),
  transferRemove: (id: string) => invoke<void>("transfer_remove", { id }),
  transferClearFinished: () => invoke<void>("transfer_clear_finished"),

  syncPreview: (
    sourceSession: string | null,
    sourcePath: string,
    targetSession: string | null,
    targetPath: string,
    twoWay: boolean,
    deleteExtra: boolean,
  ) =>
    invoke<SyncPlan>("sync_preview", {
      sourceSession,
      sourcePath,
      targetSession,
      targetPath,
      twoWay,
      deleteExtra,
    }),
  syncApply: (
    sourceSession: string | null,
    targetSession: string | null,
    plan: SyncPlan,
  ) =>
    invoke<SyncPlan>("sync_apply", {
      sourceSession,
      targetSession,
      plan,
    }),

  ptySpawn: (id: string, cols: number, rows: number, shell?: string, cwd?: string) =>
    invoke<void>("pty_spawn", {
      id,
      cols,
      rows,
      shell: shell || null,
      cwd: cwd || null,
    }),
  ptyWrite: (id: string, data: string) => invoke<void>("pty_write", { id, data }),
  ptyResize: (id: string, cols: number, rows: number) =>
    invoke<void>("pty_resize", { id, cols, rows }),
  ptyKill: (id: string) => invoke<void>("pty_kill", { id }),

  serialListPorts: () => invoke<SerialPortInfo[]>("serial_list_ports"),
  serialOpen: (id: string, config: SerialConfig) =>
    invoke<void>("serial_open", { id, config }),
  serialWrite: (id: string, data: string) =>
    invoke<void>("serial_write", { id, data }),
  serialClose: (id: string) => invoke<void>("serial_close", { id }),

  telnetOpen: (id: string, host: string, port: number, cols: number, rows: number) =>
    invoke<void>("telnet_open", { id, host, port, cols, rows }),
  telnetWrite: (id: string, data: string) =>
    invoke<void>("telnet_write", { id, data }),
  telnetResize: (id: string, cols: number, rows: number) =>
    invoke<void>("telnet_resize", { id, cols, rows }),
  telnetClose: (id: string) => invoke<void>("telnet_close", { id }),

  recordingStart: (
    id: string,
    title: string,
    cols: number,
    rows: number,
    mode?: CaptureMode,
  ) => invoke<CapturePaths>("recording_start", { id, title, cols, rows, mode }),
  recordingSetMode: (
    id: string,
    title: string,
    cols: number,
    rows: number,
    mode: CaptureMode,
  ) =>
    invoke<CapturePaths>("recording_set_mode", { id, title, cols, rows, mode }),
  recordingStop: (id: string) => invoke<CapturePaths | null>("recording_stop", { id }),
  recordingList: () => invoke<RecordingMeta[]>("recording_list"),
  recordingRead: (name: string) => invoke<CastFile>("recording_read", { name }),
  sessionLogList: () => invoke<SessionLogMeta[]>("session_log_list"),
  sessionLogRead: (name: string) => invoke<string>("session_log_read", { name }),

  importPreview: (format: ImportFormat | "auto", content: string) =>
    invoke<ImportPreview>("import_preview", { format, content }),

  saveHostCredential: (
    hostId: string,
    password?: string | null,
    passphrase?: string | null,
  ) =>
    invoke<void>("save_host_credential", { hostId, password, passphrase }),
  deleteCredential: (hostId: string) =>
    invoke<void>("delete_credential", { hostId }),
  credentialStatus: (hostId: string, keyId?: string, passwordId?: string) =>
    invoke<CredentialStatus>("credential_status", { hostId, keyId, passwordId }),
  hostSetPasswordCredential: (hostId: string, passwordId: string) =>
    invoke<void>("host_set_password_credential", { hostId, passwordId }),
  passwordCredentialList: () =>
    invoke<PasswordCredential[]>("password_credential_list"),
  passwordCredentialSave: (input: {
    id: string;
    name: string;
    username?: string;
    notes?: string;
    secret?: string | null;
    hostId?: string | null;
  }) =>
    invoke<PasswordCredential>("password_credential_save", {
      id: input.id,
      name: input.name,
      username: input.username ?? "",
      notes: input.notes ?? "",
      secret: input.secret ?? null,
      hostId: input.hostId ?? null,
    }),
  passwordCredentialDelete: (id: string) =>
    invoke<void>("password_credential_delete", { id }),
  saveSecret: (secretRef: string, secret: string) =>
    invoke<void>("save_secret", { secretRef, secret }),
  secretSaved: (secretRef: string) =>
    invoke<boolean>("secret_saved", { secretRef }),
  deleteSecret: (secretRef: string) =>
    invoke<void>("delete_secret", { secretRef }),

  vaultStatus: () => invoke<VaultStatus>("vault_status"),
  updateCheck: () => invoke<UpdateInfo>("update_check"),
  updateInstall: () => invoke<string>("update_install"),
  vaultUnlock: (password: string) =>
    invoke<void>("vault_unlock", { password }),
  vaultLock: () => invoke<void>("vault_lock"),
  vaultEnableMasterPassword: (password: string) =>
    invoke<number>("vault_enable_master_password", { password }),
  vaultDisableMasterPassword: (password: string) =>
    invoke<void>("vault_disable_master_password", { password }),
  vaultChangeMasterPassword: (current: string, next: string) =>
    invoke<void>("vault_change_master_password", { current, next }),
  vaultReset: () => invoke<void>("vault_reset"),
  vaultListEntries: () => invoke<VaultEntryView[]>("vault_list_entries"),
  vaultDeleteEntry: (secretRef: string) =>
    invoke<void>("vault_delete_entry", { secretRef }),
  vaultClearEntries: () => invoke<number>("vault_clear_entries"),

  sshKeyList: () => invoke<SshKey[]>("ssh_key_list"),
  sshKeyScan: (dir?: string) => invoke<KeyCandidate[]>("ssh_key_scan", { dir }),
  sshKeyInspect: (text: string) =>
    invoke<{
      algorithm: string;
      fingerprint: string;
      publicKey: string;
      comment: string;
      encrypted: boolean;
    }>("ssh_key_inspect", { text }),
  sshKeyImport: (payload: {
    id: string;
    name: string;
    path?: string | null;
    text?: string | null;
    passphrase?: string | null;
  }) => invoke<SshKey>("ssh_key_import", payload),
  sshKeyGenerate: (payload: {
    id: string;
    name: string;
    algorithm: string;
    passphrase?: string | null;
    comment: string;
  }) => invoke<SshKey>("ssh_key_generate", payload),
  sshKeyDelete: (id: string) => invoke<void>("ssh_key_delete", { id }),
  sshKeyUpdateMetadata: (id: string, notes: string, expiresAt: string) =>
    invoke<SshKey>("ssh_key_update_metadata", { id, notes, expiresAt }),
  sshKeyRotate: (id: string, passphrase: string, comment: string) =>
    invoke<SshKey>("ssh_key_rotate", { id, passphrase, comment }),
  sshKeyExportPublic: () => invoke<string>("ssh_key_export_public"),
  sshKeyExportBundle: (passphrase: string) =>
    invoke<string>("ssh_key_export_bundle", { passphrase }),
  sshKeyImportBundle: (payload: string, passphrase: string) =>
    invoke<KeyBundleStats>("ssh_key_import_bundle", { payload, passphrase }),

  knownHostsList: () => invoke<KnownHostEntry[]>("known_hosts_list"),
  knownHostsRemove: (host: string, port: number) =>
    invoke<void>("known_hosts_remove", { host, port }),
  credentialAuditList: (limit?: number) =>
    invoke<AuditRecord[]>("credential_audit_list", { limit }),

  getSettings: () => invoke<Record<string, string>>("get_settings"),
  setSetting: (key: string, value: string) =>
    invoke<void>("set_setting", { key, value }),

  monitorSample: () => invoke<MonitorSnapshot>("monitor_sample"),
  monitorRemote: (sessionId: string) =>
    invoke<RemoteSnapshot>("monitor_remote", { sessionId }),
  monitorKillProcess: (sessionId: string, pid: number) =>
    invoke<void>("monitor_kill_process", { sessionId, pid }),

  listCommandGroups: () => invoke<CommandGroup[]>("list_command_groups"),
  createCommandGroup: (id: string, name: string) =>
    invoke<void>("create_command_group", { id, name }),
  updateCommandGroup: (id: string, name: string) =>
    invoke<void>("update_command_group", { id, name }),
  deleteCommandGroup: (id: string) =>
    invoke<void>("delete_command_group", { id }),
  createCommand: (id: string, groupId: string, name: string, template: string) =>
    invoke<void>("create_command", { id, groupId, name, template }),
  updateCommand: (id: string, name: string, template: string) =>
    invoke<void>("update_command", { id, name, template }),
  deleteCommand: (id: string) => invoke<void>("delete_command", { id }),

  exportBackup: (passphrase: string, scope: SyncScope[]) =>
    invoke<string>("export_backup", { passphrase, scope }),
  importBackup: (payload: string, passphrase: string) =>
    invoke<ImportStats>("import_backup", { payload, passphrase }),
  syncRemoteTest: (config: RemoteConfig) =>
    invoke<string>("sync_remote_test", { config }),
  syncRemoteUpload: (
    config: RemoteConfig,
    passphrase: string,
    scope: SyncScope[],
    force = false,
  ) =>
    invoke<UploadOutcome>("sync_remote_upload", {
      config,
      passphrase,
      scope,
      force,
    }),
  syncAutoUpload: (config: RemoteConfig, scope: SyncScope[]) =>
    invoke<UploadOutcome>("sync_auto_upload", { config, scope }),
  syncRemoteDownload: (config: RemoteConfig, passphrase: string) =>
    invoke<ImportStats>("sync_remote_download", { config, passphrase }),
  syncVersionsList: (config: RemoteConfig) =>
    invoke<VersionEntry[]>("sync_versions_list", { config }),
  syncVersionRestore: (
    config: RemoteConfig,
    passphrase: string,
    versionId: string,
  ) =>
    invoke<VersionRestoreResult>("sync_version_restore", {
      config,
      passphrase,
      versionId,
    }),

  remoteMountAdd: (id: string, label: string, config: RemoteMountConfig) =>
    invoke<void>("remote_mount_add", { id, label, config }),
  remoteMountRemove: (id: string) =>
    invoke<boolean>("remote_mount_remove", { id }),
  remoteMountList: () => invoke<RemoteMountInfo[]>("remote_mount_list"),

  listLanguagePacks: () => invoke<PluginScan>("list_language_packs"),
  pluginScan: () => invoke<PluginScan>("plugin_scan"),
  pluginOpenDir: () => invoke<void>("plugin_open_dir"),
  pluginImportCommands: (packId: string) =>
    invoke<number>("plugin_import_commands", { packId }),
  pluginRunWasm: (id: string, payload: string) =>
    invoke<WasmRunResult>("plugin_run_wasm", { id, payload }),
  pluginMarketFetch: (indexUrl?: string) =>
    invoke<MarketIndex>("plugin_market_fetch", { indexUrl: indexUrl ?? null }),
  pluginMarketInstall: (id: string, manifestUrl: string) =>
    invoke<PluginManifest>("plugin_market_install", { id, manifestUrl }),
  pluginMarketUninstall: (id: string) =>
    invoke<boolean>("plugin_market_uninstall", { id }),
  pluginInstallBuiltin: (id: string) =>
    invoke<PluginManifest>("plugin_install_builtin", { id }),

  listHosts: () => invoke<HostEntry[]>("list_hosts"),
  createHost: (input: HostInput) => invoke<void>("create_host", { input }),
  updateHost: (input: HostInput) => invoke<void>("update_host", { input }),
  deleteHost: (id: string) => invoke<void>("delete_host", { id }),
  listTags: () => invoke<Tag[]>("list_tags"),
  createTag: (tag: Tag) => invoke<void>("create_tag", { tag }),
  updateTag: (tag: Tag) => invoke<void>("update_tag", { tag }),
  deleteTag: (tagId: string) => invoke<void>("delete_tag", { tagId }),

  write(kind: SessionKind, id: string, data: string) {
    if (kind === "ssh" || kind === "sftp") return api.sshWrite(id, data);
    if (kind === "serial") return api.serialWrite(id, data);
    if (kind === "telnet") return api.telnetWrite(id, data);
    return api.ptyWrite(id, data);
  },
  resize(kind: SessionKind, id: string, cols: number, rows: number) {
    if (kind === "ssh" || kind === "sftp") return api.sshResize(id, cols, rows);
    if (kind === "telnet") return api.telnetResize(id, cols, rows);
    if (kind === "serial") return Promise.resolve();
    return api.ptyResize(id, cols, rows);
  },
  close(kind: SessionKind, id: string) {
    if (kind === "ssh" || kind === "sftp") return api.sshDisconnect(id);
    if (kind === "telnet") return api.telnetClose(id);
    if (kind === "serial") return api.serialClose(id);
    return api.ptyKill(id);
  },
};

export const outputEvent = (id: string) => `terminal://${id}/output`;
export const closedEvent = (id: string) => `terminal://${id}/closed`;
export const transferUpdateEvent = "transfer://update";
export const updateProgressEvent = "update://progress";

export const onUpdateProgress = (
  cb: (progress: UpdateProgress) => void,
): Promise<UnlistenFn> => listen<UpdateProgress>(updateProgressEvent, (e) => cb(e.payload));
export const forwardErrorEvent = "forward://error";

export interface ForwardError {
  sessionId: string;
  message: string;
}

export function onOutput(
  id: string,
  cb: (data: Uint8Array) => void,
): Promise<UnlistenFn> {
  return listen<number[]>(outputEvent(id), (e) => cb(new Uint8Array(e.payload)));
}

export function onClosed(id: string, cb: () => void): Promise<UnlistenFn> {
  return listen(closedEvent(id), () => cb());
}

export function onTransferUpdate(
  cb: (job: TransferJob) => void,
): Promise<UnlistenFn> {
  return listen<TransferJob>(transferUpdateEvent, (e) => cb(e.payload));
}

export function onForwardError(
  cb: (error: ForwardError) => void,
): Promise<UnlistenFn> {
  return listen<ForwardError>(forwardErrorEvent, (e) => cb(e.payload));
}
export const netToolOutputEvent = "net-tool://output";
export const netToolDoneEvent = "net-tool://done";

export type NetProbeKind = "ping" | "tcping" | "udp" | "traceroute";

export interface NetToolChunk {
  id: string;
  stream: "out" | "err";
  line: string;
}

/** Aggregate of one probe run; latency is absent when nothing came back. */
export interface ProbeSummary {
  sent: number;
  received: number;
  replied: number;
  min: number | null;
  avg: number | null;
  max: number | null;
}

export interface NetToolDone {
  id: string;
  code: number | null;
  cancelled: boolean;
  error: string | null;
  summary: ProbeSummary | null;
}

export interface GeoInfo {
  ip: string;
  version: string;
  country: string;
  countryCode: string;
  region: string;
  city: string;
  latitude: number;
  longitude: number;
  isp: string;
  org: string;
  asn: string;
  timezone: string;
  source: string;
}

/** Runs ping or traceroute on this machine and returns the job id. */
export interface NetProbeOptions {
  count?: number;
  hops?: number;
  port?: number;
  interval?: number;
  timeout?: number;
  size?: number;
}

export function startNetProbe(
  kind: NetProbeKind,
  target: string,
  options: NetProbeOptions = {},
): Promise<string> {
  return invoke<string>("net_tool_start", {
    kind,
    target,
    count: options.count,
    hops: options.hops,
    port: options.port,
    interval: options.interval,
    timeout: options.timeout,
    size: options.size,
  });
}

export function cancelNetProbe(id: string): Promise<void> {
  return invoke<void>("net_tool_cancel", { id });
}

export function geoipLookup(query: string): Promise<GeoInfo> {
  return invoke<GeoInfo>("geoip_lookup", { query });
}

export function onNetToolOutput(
  cb: (chunk: NetToolChunk) => void,
): Promise<UnlistenFn> {
  return listen<NetToolChunk>(netToolOutputEvent, (e) => cb(e.payload));
}

export function onNetToolDone(
  cb: (event: NetToolDone) => void,
): Promise<UnlistenFn> {
  return listen<NetToolDone>(netToolDoneEvent, (e) => cb(e.payload));
}

// ------------------------------------------------------------------- AI (BYOK)

export type AiKind = "openai" | "anthropic" | "azure";
export type AiScope = "none" | "command" | "selection" | "screen";
export type AiMaxTokensField = "auto" | "maxTokens" | "maxCompletionTokens";
export type AiReasoning = "off" | "low" | "medium" | "high";

export interface AiHeader {
  name: string;
  value: string;
}

/** Provider as returned to the UI; the API key itself never leaves Rust. */
export interface AiProvider {
  id: string;
  name: string;
  kind: AiKind;
  baseUrl: string;
  model: string;
  timeoutSecs: number;
  temperature: number;
  maxTokens: number;
  fastModel: string;
  preset: string;
  /** Which field carries the output cap; `auto` retries with the other one. */
  maxTokensField: AiMaxTokensField;
  /** Send temperature/top_p at all — reasoning models reject them. */
  sendSampling: boolean;
  topP: number | null;
  stop: string[];
  /** Merge the system prompt into the first user turn. */
  foldSystem: boolean;
  /** Ask for a usage block on the stream. */
  streamUsage: boolean;
  /** Send `response_format: json_object` on JSON-only calls. */
  jsonMode: boolean;
  reasoning: AiReasoning;
  /** Azure `api-version`. */
  apiVersion: string;
  extraHeaders: AiHeader[];
  proxy: string;
  hasKey: boolean;
}

export type AiProviderInput = Omit<AiProvider, "hasKey">;

export interface AiPreset {
  id: string;
  name: string;
  provider: AiProviderInput;
}

export interface AiReply {
  text: string;
  latencyMs: number;
  model: string;
  /** i18n keys for parameters dropped automatically by the vendor. */
  notes: string[];
}

export interface AiCommandReply {
  command: string;
  explanation: string;
  /** The model declared the goal met and deliberately returned no command. */
  done?: boolean;
  latencyMs: number;
  model: string;
  notes: string[];
}

export interface AiChatMessage {
  role: "user" | "assistant";
  content: string;
  /** The model's reasoning, split out of the stream when wrapped in <think:6124c78e>…</think:6124c78e>. */
  thinking?: string;
}

export interface AiStep {
  command: string;
  output: string;
}

/** One stored conversation, newest first in the panel's list. */
export interface AiSessionRow {
  id: string;
  title: string;
  providerId: string;
  model: string;
  createdAt: number;
  updatedAt: number;
}

/** A persisted message; `thinking` is the reasoning split out of the stream. */
export interface AiMessageRow {
  role: "user" | "assistant";
  content: string;
  thinking: string;
  createdAt: number;
}

export interface AiContext {
  selection?: string | null;
  lastCommand?: string | null;
  screen: string[];
  scope: AiScope;
  lines: number;
  sendOutput: boolean;
  redact: boolean;
  /** Steps the continuous agent has already taken; fed back to the model. */
  history?: AiStep[];
}

/** Settings persisted under the `aiSettings` key. */
export interface AiSettings {
  enabled: boolean;
  redact: boolean;
  scope: AiScope;
  screenLines: number;
  sendOutput: boolean;
  deniedHosts: string;
  firstSendConfirmed: boolean;
  nlPrefix: string;
  /** Last model chosen per provider id. Switching in the panel is remembered
   *  here so the provider's configured default is never rewritten. */
  modelByProvider: Record<string, string>;
}

export const DEFAULT_AI_SETTINGS: AiSettings = {
  enabled: true,
  redact: true,
  scope: "screen",
  screenLines: 40,
  sendOutput: false,
  deniedHosts: "",
  firstSendConfirmed: false,
  nlPrefix: "#",
  modelByProvider: {},
};

export function parseAiSettings(raw: string | undefined): AiSettings {
  if (!raw) return { ...DEFAULT_AI_SETTINGS, modelByProvider: {} };
  try {
    const parsed = JSON.parse(raw) as Partial<AiSettings>;
    const stored = parsed.modelByProvider;
    return {
      ...DEFAULT_AI_SETTINGS,
      ...parsed,
      modelByProvider:
        stored && typeof stored === "object" ? { ...stored } : {},
    };
  } catch {
    return { ...DEFAULT_AI_SETTINGS, modelByProvider: {} };
  }
}

export function aiListProviders(): Promise<AiProvider[]> {
  return invoke<AiProvider[]>("ai_list_providers");
}

export function aiPresets(): Promise<AiPreset[]> {
  return invoke<AiPreset[]>("ai_presets");
}

export function aiSaveProvider(provider: AiProviderInput): Promise<AiProvider> {
  return invoke<AiProvider>("ai_save_provider", { provider });
}

export function aiDeleteProvider(id: string): Promise<void> {
  return invoke<void>("ai_delete_provider", { id });
}

export function aiTestProvider(id: string): Promise<AiReply> {
  return invoke<AiReply>("ai_test_provider", { id });
}

/** Read the vendor catalogue; works before the provider is saved. */
export function aiListModels(
  provider: AiProviderInput,
  key?: string,
): Promise<string[]> {
  return invoke<string[]>("ai_list_models", { provider, key: key ?? null });
}

export function aiChat(
  providerId: string,
  messages: AiChatMessage[],
  context: AiContext | null,
  system?: string,
  model?: string,
): Promise<AiReply> {
  return invoke<AiReply>("ai_chat", {
    providerId,
    messages,
    context,
    system,
    model: model ?? null,
  });
}

/** Streaming chat: `onDelta` receives each fragment as it arrives. */
export function aiChatStream(
  providerId: string,
  messages: AiChatMessage[],
  context: AiContext | null,
  system: string | undefined,
  onDelta: (delta: string) => void,
  model?: string,
): Promise<AiReply> {
  const channel = new Channel<{ delta: string }>();
  channel.onmessage = (event) => onDelta(event.delta);
  return invoke<AiReply>("ai_chat_stream", {
    providerId,
    messages,
    context,
    system,
    model: model ?? null,
    channel,
  });
}

export function aiGenerateCommand(
  providerId: string,
  request: string,
  options: {
    host?: string | null;
    shell?: string | null;
    context: AiContext | null;
    /** Overrides the provider's default (and its fast command model). */
    model?: string | null;
  },
): Promise<AiCommandReply> {
  return invoke<AiCommandReply>("ai_generate_command", {
    providerId,
    request,
    host: options.host ?? null,
    shell: options.shell ?? null,
    context: options.context,
    model: options.model ?? null,
  });
}

export function aiExplain(
  providerId: string,
  command: string,
  code: number,
  host: string | null,
  outputTail: string,
  context: AiContext | null,
  model?: string,
): Promise<AiReply> {
  return invoke<AiReply>("ai_explain", {
    providerId,
    command,
    code,
    host,
    outputTail,
    context,
    model: model ?? null,
  });
}

// ------------------------------------------------------------------ history

export function aiSessions(): Promise<AiSessionRow[]> {
  return invoke<AiSessionRow[]>("ai_sessions");
}

export function aiSessionCreate(
  title: string,
  providerId: string,
  model: string,
): Promise<AiSessionRow> {
  return invoke<AiSessionRow>("ai_session_create", { title, providerId, model });
}

export function aiSessionMessages(id: string): Promise<AiMessageRow[]> {
  return invoke<AiMessageRow[]>("ai_session_messages", { id });
}

/** Append a final message; returns null when the session was deleted meanwhile. */
export function aiSessionAppend(
  id: string,
  role: "user" | "assistant",
  content: string,
  thinking: string,
): Promise<AiSessionRow | null> {
  return invoke<AiSessionRow | null>("ai_session_append", {
    id,
    role,
    content,
    thinking,
  });
}

export function aiSessionRename(id: string, title: string): Promise<void> {
  return invoke<void>("ai_session_rename", { id, title });
}

export function aiSessionDelete(id: string): Promise<boolean> {
  return invoke<boolean>("ai_session_delete", { id });
}

// ------------------------------------------------------------------ policy

/** How much rope the non-human execution paths get. */
export type PolicyMode = "auto" | "ask" | "plan" | "agent";
export type PolicyVerdict = "allow" | "ask" | "deny";
export type PolicyEffect = "allow" | "ask" | "deny";
export type PolicyRisk = "r0" | "r1" | "r2" | "r3" | "r4";

export type PolicyReason =
  | "rootDelete"
  | "systemPath"
  | "elevated"
  | "recursive"
  | "remotePipe"
  | "secretFile"
  | "diskDevice"
  | "forkBomb"
  | "privilegeChange"
  | "serviceControl"
  | "packageInstall"
  | "bulkDelete"
  | "writeRedirect"
  | "unknown";

export interface PolicyRule {
  id: string;
  /** Glob over the whole command line. */
  pattern: string;
  effect: PolicyEffect;
  /** Glob over host names, comma separated. Empty matches every host. */
  host: string;
  note: string;
  builtin: boolean;
}

export interface PolicyConfig {
  mode: PolicyMode;
  rules: PolicyRule[];
}

export interface PolicyDecision {
  verdict: PolicyVerdict;
  risk: PolicyRisk;
  reasons: PolicyReason[];
  rule: string | null;
  mode: PolicyMode;
  /** Irreversible: no rule can allow it. */
  hard: boolean;
}

export function policyGet(): Promise<PolicyConfig> {
  return invoke<PolicyConfig>("policy_get");
}

export function policySave(config: PolicyConfig): Promise<PolicyConfig> {
  return invoke<PolicyConfig>("policy_save", { config });
}

export function policyEvaluate(
  command: string,
  host: string,
  elevated: boolean,
): Promise<PolicyDecision> {
  return invoke<PolicyDecision>("policy_evaluate", { command, host, elevated });
}

export function policyAddRule(
  pattern: string,
  effect: PolicyEffect,
  host?: string,
  note?: string,
): Promise<PolicyConfig> {
  return invoke<PolicyConfig>("policy_add_rule", {
    pattern,
    effect,
    host: host ?? null,
    note: note ?? null,
  });
}

export function policyRemoveRule(id: string): Promise<PolicyConfig> {
  return invoke<PolicyConfig>("policy_remove_rule", { id });
}

/** How the gate ended up treating one command. */
export type AuditChoice =
  | "auto"
  | "once"
  | "session"
  | "always"
  | "cancelled"
  | "denied";

/** Who asked for the decision. */
export type AuditOrigin = "ui" | "cli" | "mcp" | "agent";

export interface AuditEntry {
  id: string;
  /** UTC, second precision, `YYYY-MM-DDTHH:MM:SSZ`. */
  ts: string;
  host: string;
  command: string;
  risk: PolicyRisk;
  reasons: PolicyReason[];
  rule: string | null;
  verdict: PolicyVerdict;
  choice: AuditChoice;
  mode: PolicyMode;
  /** Absent in records written before the field existed; treated as `ui`. */
  origin?: AuditOrigin;
  prevHash: string;
  hash: string;
}

export interface AuditVerifyReport {
  month: string;
  entries: number;
  ok: boolean;
  /** 1-based position of the first record that does not verify. */
  brokenAt: number | null;
  /** `hash` | `link` | `parse`. */
  brokenReason: string | null;
}

export interface RulePreview {
  risk: PolicyRisk;
  reasons: PolicyReason[];
  verdict: PolicyVerdict;
}

export function policyRecord(input: {
  command: string;
  host: string;
  decision: PolicyDecision;
  choice: AuditChoice;
  origin?: AuditOrigin;
}): Promise<AuditEntry> {
  return invoke<AuditEntry>("policy_record", { entry: input });
}

/** Absolute path of the CLI bundled next to the app, or its bare name. */
export function agentCliPath(): Promise<string> {
  return invoke<string>("agent_cli_path");
}

/** Whether the control service is meant to listen, and whether it does. */
export type AgentStatus = {
  enabled: boolean;
  running: boolean;
  /** Loopback port it answers on; 0 while stopped. */
  port: number;
};

export function agentStatus(): Promise<AgentStatus> {
  return invoke<AgentStatus>("agent_status");
}

/** Switches the control service on or off; the choice is persisted. */
export function agentServiceSet(enabled: boolean): Promise<AgentStatus> {
  return invoke<AgentStatus>("agent_service_set", { enabled });
}

export function policyAuditMonths(): Promise<string[]> {
  return invoke<string[]>("policy_audit_months");
}

export function policyAuditList(month: string): Promise<AuditEntry[]> {
  return invoke<AuditEntry[]>("policy_audit_list", { month });
}

export function policyAuditVerify(month?: string): Promise<AuditVerifyReport[]> {
  return invoke<AuditVerifyReport[]>("policy_audit_verify", {
    month: month ?? null,
  });
}

export function policyAuditPath(month: string): Promise<string> {
  return invoke<string>("policy_audit_path", { month });
}

export function policyPreview(
  pattern: string,
  host: string,
): Promise<RulePreview> {
  return invoke<RulePreview>("policy_preview", { pattern, host });
}
