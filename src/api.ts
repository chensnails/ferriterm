import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export const isTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export type SessionKind = "ssh" | "pty";

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
  jumpHostId: string;
  x11: boolean;
  parentId: string | null;
  isFolder: boolean;
  production: boolean;
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
  jumpHostId: string;
  x11: boolean;
  parentId: string | null;
  isFolder: boolean;
  production: boolean;
  notes: string;
  tagIds: string[];
}

export interface JumpHop {
  host: string;
  port: number;
  username: string;
  password?: string;
  privateKey?: string;
  keyPassphrase?: string;
  agent?: boolean;
  keyboardInteractive?: boolean;
}

export interface SshConnectArgs {
  host: string;
  port: number;
  username: string;
  password?: string;
  privateKey?: string;
  keyPassphrase?: string;
  agent?: boolean;
  keyboardInteractive?: boolean;
  x11?: boolean;
  jump?: JumpHop[];
  keepalive?: number;
  hostId?: string;
  cols: number;
  rows: number;
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

export interface SyncAction {
  kind: SyncActionKind;
  path: string;
  size: number;
}

export interface SyncPlan {
  actions: SyncAction[];
  uploadBytes: number;
  createDirs: number;
  copyFiles: number;
  deleteEntries: number;
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
    deleteExtra: boolean,
  ) =>
    invoke<SyncPlan>("sync_preview", {
      sourceSession,
      sourcePath,
      targetSession,
      targetPath,
      deleteExtra,
    }),
  syncApply: (
    sourceSession: string | null,
    sourcePath: string,
    targetSession: string | null,
    targetPath: string,
    deleteExtra: boolean,
  ) =>
    invoke<SyncPlan>("sync_apply", {
      sourceSession,
      sourcePath,
      targetSession,
      targetPath,
      deleteExtra,
    }),

  ptySpawn: (id: string, cols: number, rows: number) =>
    invoke<void>("pty_spawn", { id, cols, rows }),
  ptyWrite: (id: string, data: string) => invoke<void>("pty_write", { id, data }),
  ptyResize: (id: string, cols: number, rows: number) =>
    invoke<void>("pty_resize", { id, cols, rows }),
  ptyKill: (id: string) => invoke<void>("pty_kill", { id }),

  recordingStart: (id: string, title: string, cols: number, rows: number) =>
    invoke<string>("recording_start", { id, title, cols, rows }),
  recordingStop: (id: string) => invoke<string | null>("recording_stop", { id }),
  recordingList: () => invoke<RecordingMeta[]>("recording_list"),
  recordingRead: (name: string) => invoke<CastFile>("recording_read", { name }),

  importPreview: (format: ImportFormat | "auto", content: string) =>
    invoke<ImportPreview>("import_preview", { format, content }),

  saveCredential: (hostId: string, secret: string) =>
    invoke<void>("save_credential", { hostId, secret }),
  getCredential: (hostId: string) =>
    invoke<string | null>("get_credential", { hostId }),
  deleteCredential: (hostId: string) =>
    invoke<void>("delete_credential", { hostId }),

  getSettings: () => invoke<Record<string, string>>("get_settings"),
  setSetting: (key: string, value: string) =>
    invoke<void>("set_setting", { key, value }),

  monitorSample: () => invoke<MonitorSnapshot>("monitor_sample"),

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

  exportBackup: (passphrase: string) =>
    invoke<string>("export_backup", { passphrase }),
  importBackup: (payload: string, passphrase: string) =>
    invoke<ImportStats>("import_backup", { payload, passphrase }),
  syncRemoteTest: (config: RemoteConfig) =>
    invoke<string>("sync_remote_test", { config }),
  syncRemoteUpload: (config: RemoteConfig, passphrase: string) =>
    invoke<RemoteUpload>("sync_remote_upload", { config, passphrase }),
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

  listHosts: () => invoke<HostEntry[]>("list_hosts"),
  createHost: (input: HostInput) => invoke<void>("create_host", { input }),
  updateHost: (input: HostInput) => invoke<void>("update_host", { input }),
  deleteHost: (id: string) => invoke<void>("delete_host", { id }),
  listTags: () => invoke<Tag[]>("list_tags"),
  createTag: (tag: Tag) => invoke<void>("create_tag", { tag }),
  updateTag: (tag: Tag) => invoke<void>("update_tag", { tag }),
  deleteTag: (tagId: string) => invoke<void>("delete_tag", { tagId }),

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
