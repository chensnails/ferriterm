import { invoke } from "@tauri-apps/api/core";
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
  cols: number;
  rows: number;
}

export type VaultMode = "keychain" | "master";

export interface VaultStatus {
  mode: VaultMode;
  locked: boolean;
  keychainAvailable: boolean;
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
}

export interface KeyCandidate {
  path: string;
  name: string;
  algorithm: string;
  fingerprint: string;
  comment: string;
  encrypted: boolean;
}

export interface KnownHostEntry {
  host: string;
  port: number;
  algorithm: string;
  fingerprint: string;
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
  diskUsed: number;
  diskTotal: number;
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

export interface ImportStats {
  hostsAdded: number;
  hostsSkipped: number;
  tagsAdded: number;
}

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

export interface PluginScan {
  plugins: PluginManifest[];
  packs: LanguagePack[];
  themes: ThemePack[];
  commandPacks: CommandPack[];
  errors: string[];
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

  recordingStart: (id: string, title: string, cols: number, rows: number) =>
    invoke<string>("recording_start", { id, title, cols, rows }),
  recordingStop: (id: string) => invoke<string | null>("recording_stop", { id }),
  recordingList: () => invoke<RecordingMeta[]>("recording_list"),
  recordingRead: (name: string) => invoke<CastFile>("recording_read", { name }),

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
  credentialStatus: (hostId: string, keyId?: string) =>
    invoke<CredentialStatus>("credential_status", { hostId, keyId }),
  saveSecret: (secretRef: string, secret: string) =>
    invoke<void>("save_secret", { secretRef, secret }),
  secretSaved: (secretRef: string) =>
    invoke<boolean>("secret_saved", { secretRef }),
  deleteSecret: (secretRef: string) =>
    invoke<void>("delete_secret", { secretRef }),

  vaultStatus: () => invoke<VaultStatus>("vault_status"),
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

  exportBackup: (passphrase: string, includeSecrets: boolean) =>
    invoke<string>("export_backup", { passphrase, includeSecrets }),
  importBackup: (payload: string, passphrase: string) =>
    invoke<ImportStats>("import_backup", { payload, passphrase }),
  syncRemoteTest: (config: RemoteConfig) =>
    invoke<string>("sync_remote_test", { config }),
  syncRemoteUpload: (
    config: RemoteConfig,
    passphrase: string,
    includeSecrets: boolean,
  ) =>
    invoke<RemoteUpload>("sync_remote_upload", {
      config,
      passphrase,
      includeSecrets,
    }),
  syncRemoteDownload: (config: RemoteConfig, passphrase: string) =>
    invoke<ImportStats>("sync_remote_download", { config, passphrase }),

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

export type NetProbeKind = "ping" | "traceroute";

export interface NetToolChunk {
  id: string;
  stream: "out" | "err";
  line: string;
}

export interface NetToolDone {
  id: string;
  code: number | null;
  cancelled: boolean;
  error: string | null;
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
export function startNetProbe(
  kind: NetProbeKind,
  target: string,
  options: { count?: number; hops?: number } = {},
): Promise<string> {
  return invoke<string>("net_tool_start", {
    kind,
    target,
    count: options.count,
    hops: options.hops,
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
