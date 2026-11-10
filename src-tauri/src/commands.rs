use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use ft_core::{events, RecorderRegistry, SessionRegistry, TerminalSink};
use ft_ssh::{
    ConnectContext, CredentialProvider, HostKeyChallenge, HostKeyDecision, HostKeyPrompt,
    KeyboardInteractiveChallenge, KeyboardInteractivePrompt, KnownHostsStore, SecretKind,
    TofuVerifier,
};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::oneshot;

struct TauriSink {
    app: AppHandle,
    recorders: RecorderRegistry,
}

impl TerminalSink for TauriSink {
    fn output(&self, id: &str, data: Vec<u8>) {
        self.recorders.write(id, &data);
        let _ = self.app.emit(&events::output(id), data);
    }
    fn closed(&self, id: &str) {
        let _ = self.app.emit(&events::closed(id), ());
    }
}

pub struct PromptQueue<T> {
    pending: Arc<StdMutex<HashMap<u64, oneshot::Sender<T>>>>,
    next_id: Arc<AtomicU64>,
}

impl<T> Clone for PromptQueue<T> {
    fn clone(&self) -> Self {
        Self {
            pending: self.pending.clone(),
            next_id: self.next_id.clone(),
        }
    }
}

impl<T> Default for PromptQueue<T> {
    fn default() -> Self {
        Self {
            pending: Arc::default(),
            next_id: Arc::default(),
        }
    }
}

impl<T> PromptQueue<T> {
    fn register(&self, respond: oneshot::Sender<T>) -> u64 {
        let request_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut pending) = self.pending.lock() {
            pending.insert(request_id, respond);
        }
        request_id
    }

    fn resolve(&self, request_id: u64, value: T) -> bool {
        let respond = self
            .pending
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(&request_id));
        respond.is_some_and(|respond| respond.send(value).is_ok())
    }
}

pub type HostKeyState = PromptQueue<HostKeyDecision>;
pub type KeyboardInteractiveState = PromptQueue<Option<Vec<String>>>;

#[derive(Clone)]
pub struct SshRuntimeState {
    host_keys: HostKeyState,
    keyboard: KeyboardInteractiveState,
    known_hosts: Arc<KnownHostsStore>,
    forwards: Arc<ft_ssh::ForwardRegistry>,
    store: Arc<ft_store::Store>,
    sftp: Arc<ft_vfs::SftpRegistry>,
    transfers: ft_transfer::TransferManager,
}

impl SshRuntimeState {
    pub fn new(
        known_hosts: Arc<KnownHostsStore>,
        forwards: Arc<ft_ssh::ForwardRegistry>,
        store: Arc<ft_store::Store>,
        sftp: Arc<ft_vfs::SftpRegistry>,
        transfers: ft_transfer::TransferManager,
    ) -> Self {
        Self {
            host_keys: HostKeyState::default(),
            keyboard: KeyboardInteractiveState::default(),
            known_hosts,
            forwards,
            store,
            sftp,
            transfers,
        }
    }
}

struct TauriHostKeyPrompt {
    app: AppHandle,
    state: HostKeyState,
    session_id: String,
}

impl HostKeyPrompt for TauriHostKeyPrompt {
    fn prompt(&self, challenge: HostKeyChallenge, respond: oneshot::Sender<HostKeyDecision>) {
        let request_id = self.state.register(respond);
        let _ = self.app.emit(
            "hostkey://verify",
            serde_json::json!({
                "requestId": request_id,
                "sessionId": self.session_id,
                "host": challenge.host,
                "port": challenge.port,
                "fingerprint": challenge.fingerprint,
                "algorithm": challenge.algorithm,
                "changed": challenge.changed,
            }),
        );
    }
}

struct TauriKeyboardPrompt {
    app: AppHandle,
    state: KeyboardInteractiveState,
    session_id: String,
}

impl KeyboardInteractivePrompt for TauriKeyboardPrompt {
    fn prompt(
        &self,
        challenge: KeyboardInteractiveChallenge,
        respond: oneshot::Sender<Option<Vec<String>>>,
    ) {
        let request_id = self.state.register(respond);
        let prompts: Vec<serde_json::Value> = challenge
            .prompts
            .iter()
            .map(|prompt| {
                serde_json::json!({
                    "text": prompt.text,
                    "echo": prompt.echo,
                })
            })
            .collect();
        let _ = self.app.emit(
            "auth://keyboard-interactive",
            serde_json::json!({
                "requestId": request_id,
                "sessionId": self.session_id,
                "name": challenge.name,
                "instructions": challenge.instructions,
                "prompts": prompts,
            }),
        );
    }
}

#[tauri::command]
pub fn ssh_host_key_respond(
    state: State<'_, SshRuntimeState>,
    request_id: u64,
    accept: bool,
    remember: bool,
) -> bool {
    state
        .host_keys
        .resolve(request_id, HostKeyDecision { accept, remember })
}

#[tauri::command]
pub fn ssh_keyboard_interactive_respond(
    state: State<'_, SshRuntimeState>,
    request_id: u64,
    responses: Option<Vec<String>>,
) -> bool {
    state.keyboard.resolve(request_id, responses)
}

#[tauri::command]
pub async fn ssh_connect(
    app: AppHandle,
    state: State<'_, SessionRegistry>,
    recorders: State<'_, RecorderRegistry>,
    runtime: State<'_, SshRuntimeState>,
    credentials: State<'_, Arc<ft_security::CredentialStore>>,
    id: String,
    args: ft_ssh::SshConnectArgs,
) -> Result<(), String> {
    let sink = Arc::new(TauriSink {
        app: app.clone(),
        recorders: recorders.inner().clone(),
    });
    let host_key_prompt = Arc::new(TauriHostKeyPrompt {
        app: app.clone(),
        state: runtime.host_keys.clone(),
        session_id: id.clone(),
    });
    let keyboard_prompt = Arc::new(TauriKeyboardPrompt {
        app: app.clone(),
        state: runtime.keyboard.clone(),
        session_id: id.clone(),
    });
    let verifier = Arc::new(TofuVerifier::new(
        runtime.known_hosts.clone(),
        Some(host_key_prompt),
    ));
    let router = Arc::new(ft_ssh::ForwardRouter::new(
        runtime.forwards.clone(),
        id.clone(),
    ));
    let provider = Arc::new(VaultProvider {
        credentials: credentials.inner().clone(),
    });
    let ctx = ConnectContext::new(verifier)
        .with_keyboard_interactive(keyboard_prompt)
        .with_remote_forwards(router)
        .with_credentials(provider);
    let host_id = args.host_id.clone();
    let audit_target = args
        .host_id
        .clone()
        .unwrap_or_else(|| format!("{}@{}", args.username, args.host));
    let result = ft_ssh::connect(sink, &state, id.clone(), args, ctx).await;
    let detail = result.as_ref().err().cloned().unwrap_or_default();
    let _ = runtime.store.append_audit(
        if result.is_ok() {
            "ssh.connect"
        } else {
            "ssh.connect.failed"
        },
        &audit_target,
        &detail,
    );
    result?;
    if let Some(host_id) = host_id {
        auto_start_forwards(&app, &state, &runtime, &host_id, &id).await;
    }
    Ok(())
}

async fn auto_start_forwards(
    app: &AppHandle,
    sessions: &SessionRegistry,
    runtime: &SshRuntimeState,
    host_id: &str,
    session_id: &str,
) {
    let rules = match runtime.store.list_forward_rules(Some(host_id)) {
        Ok(rules) => rules,
        Err(error) => {
            let _ = app.emit(
                "forward://error",
                serde_json::json!({ "sessionId": session_id, "message": error.to_string() }),
            );
            return;
        }
    };
    for rule in rules {
        let request = ft_ssh::ForwardRequest {
            id: format!("{}@{}", rule.id, session_id),
            session_id: session_id.to_string(),
            kind: Some(match rule.kind.as_str() {
                "remote" => ft_ssh::ForwardKind::Remote,
                "dynamic" => ft_ssh::ForwardKind::Dynamic,
                _ => ft_ssh::ForwardKind::Local,
            }),
            bind_host: Some(rule.bind_host.clone()),
            bind_port: rule.bind_port,
            dest_host: rule.dest_host.clone(),
            dest_port: rule.dest_port,
        };
        if let Err(error) = ft_ssh::start_forward(sessions, &runtime.forwards, request).await {
            let _ = app.emit(
                "forward://error",
                serde_json::json!({ "sessionId": session_id, "message": error }),
            );
        }
    }
}

#[tauri::command]
pub fn list_forward_rules(
    runtime: State<'_, SshRuntimeState>,
    host_id: Option<String>,
) -> Result<Vec<ft_store::ForwardRule>, String> {
    runtime
        .store
        .list_forward_rules(host_id.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_forward_rule(
    runtime: State<'_, SshRuntimeState>,
    rule: ft_store::ForwardRule,
) -> Result<(), String> {
    runtime
        .store
        .save_forward_rule(&rule)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_forward_rule(runtime: State<'_, SshRuntimeState>, id: String) -> Result<(), String> {
    runtime
        .store
        .delete_forward_rule(&id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ssh_write(
    state: State<'_, SessionRegistry>,
    id: String,
    data: String,
) -> Result<(), String> {
    ft_ssh::write(&state, &id, data).await
}

#[tauri::command]
pub async fn ssh_resize(
    state: State<'_, SessionRegistry>,
    id: String,
    cols: u32,
    rows: u32,
) -> Result<(), String> {
    ft_ssh::resize(&state, &id, cols, rows).await
}

#[tauri::command]
pub async fn ssh_disconnect(
    state: State<'_, SessionRegistry>,
    runtime: State<'_, SshRuntimeState>,
    remote: State<'_, Arc<ft_monitor::RemoteMonitor>>,
    id: String,
) -> Result<(), String> {
    runtime.forwards.stop_session(&id).await;
    runtime.sftp.invalidate(&id).await;
    remote.forget(&id);
    ft_ssh::disconnect(&state, &id).await
}

#[tauri::command]
pub async fn forward_start(
    sessions: State<'_, SessionRegistry>,
    runtime: State<'_, SshRuntimeState>,
    request: ft_ssh::ForwardRequest,
) -> Result<ft_ssh::ForwardInfo, String> {
    ft_ssh::start_forward(&sessions, &runtime.forwards, request).await
}

#[tauri::command]
pub async fn forward_stop(runtime: State<'_, SshRuntimeState>, id: String) -> Result<bool, String> {
    Ok(runtime.forwards.stop(&id).await)
}

#[tauri::command]
pub async fn forward_list(
    runtime: State<'_, SshRuntimeState>,
) -> Result<Vec<ft_ssh::ForwardInfo>, String> {
    Ok(runtime.forwards.list().await)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn pty_spawn(
    app: AppHandle,
    state: State<'_, SessionRegistry>,
    recorders: State<'_, RecorderRegistry>,
    id: String,
    cols: u16,
    rows: u16,
    shell: Option<String>,
    cwd: Option<String>,
) -> Result<(), String> {
    let sink = Arc::new(TauriSink {
        app,
        recorders: recorders.inner().clone(),
    });
    ft_pty::spawn(sink, &state, id, cols, rows, shell, cwd).await
}

#[tauri::command]
pub async fn pty_write(
    state: State<'_, SessionRegistry>,
    id: String,
    data: String,
) -> Result<(), String> {
    ft_pty::write(&state, &id, data).await
}

#[tauri::command]
pub async fn pty_resize(
    state: State<'_, SessionRegistry>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    ft_pty::resize(&state, &id, cols, rows).await
}

#[tauri::command]
pub async fn pty_kill(state: State<'_, SessionRegistry>, id: String) -> Result<(), String> {
    ft_pty::kill(&state, &id).await
}

#[tauri::command]
pub fn recording_start(
    recorders: State<'_, RecorderRegistry>,
    id: String,
    title: String,
    cols: u16,
    rows: u16,
) -> Result<String, String> {
    recorders
        .start(&id, &title, cols, rows)
        .map(|path| path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn recording_stop(recorders: State<'_, RecorderRegistry>, id: String) -> Option<String> {
    recorders
        .stop(&id)
        .map(|path| path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn recording_list(
    recorders: State<'_, RecorderRegistry>,
) -> Result<Vec<ft_core::RecordingMeta>, String> {
    recorders.list()
}

#[tauri::command]
pub fn recording_read(
    recorders: State<'_, RecorderRegistry>,
    name: String,
) -> Result<ft_core::CastFile, String> {
    recorders.read(&name)
}

#[tauri::command]
pub fn import_preview(format: String, content: String) -> Result<ft_import::ImportPreview, String> {
    let format = if format == "auto" {
        None
    } else {
        Some(ft_import::parse_format(&format).ok_or("unsupported import format")?)
    };
    ft_import::preview(format, &content).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn monitor_sample(
    state: State<'_, tokio::sync::Mutex<ft_monitor::Monitor>>,
) -> Result<ft_monitor::MonitorSnapshot, String> {
    let mut monitor = state.lock().await;
    Ok(monitor.sample().await)
}

/// Samples a connected SSH session with the fixed read-only probe. The output
/// is capped and the call times out so a stalled host cannot wedge the panel.
#[tauri::command]
pub async fn monitor_remote(
    sessions: State<'_, SessionRegistry>,
    remote: State<'_, Arc<ft_monitor::RemoteMonitor>>,
    session_id: String,
) -> Result<ft_monitor::RemoteSnapshot, String> {
    let output = ft_ssh::exec(
        &sessions,
        &session_id,
        ft_monitor::REMOTE_PROBE,
        std::time::Duration::from_secs(6),
        64 * 1024,
    )
    .await?;
    remote.sample(&session_id, &output)
}

#[derive(serde::Deserialize)]
struct BackupPayload {
    #[serde(default)]
    tags: Vec<ft_store::Tag>,
    #[serde(default)]
    hosts: Vec<ft_store::Host>,
    #[serde(default)]
    secrets: Vec<BackupSecret>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupSecret {
    secret_ref: String,
    kind: String,
    label: String,
    host_id: Option<String>,
    secret: String,
}

#[tauri::command]
pub fn list_language_packs(plugins: State<'_, ft_plugin::PluginRegistry>) -> ft_plugin::PluginScan {
    plugins.scan()
}

#[tauri::command]
pub fn plugin_scan(plugins: State<'_, ft_plugin::PluginRegistry>) -> ft_plugin::PluginScan {
    plugins.scan()
}

#[tauri::command]
pub fn plugin_open_dir(plugins: State<'_, ft_plugin::PluginRegistry>) -> Result<(), String> {
    let dir = plugins.dir().to_path_buf();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    open_in_file_manager(&dir)
}

fn open_in_file_manager(path: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("open").arg(path).status();
    #[cfg(target_os = "windows")]
    let status = std::process::Command::new("explorer").arg(path).status();
    #[cfg(all(unix, not(target_os = "macos")))]
    let status = std::process::Command::new("xdg-open").arg(path).status();
    match status {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!("file manager exited with {status}")),
        Err(err) => Err(err.to_string()),
    }
}

#[tauri::command]
pub fn plugin_import_commands(
    plugins: State<'_, ft_plugin::PluginRegistry>,
    store: State<'_, Arc<ft_store::Store>>,
    pack_id: String,
) -> Result<usize, String> {
    let pack = plugins
        .scan()
        .command_packs
        .into_iter()
        .find(|pack| pack.id == pack_id)
        .ok_or_else(|| format!("command pack {pack_id} not found"))?;
    let mut imported = 0;
    for (group_index, group) in pack.groups.iter().enumerate() {
        let group_id = format!("{}-g{group_index}", pack.id);
        let commands: Vec<(String, String, String)> = group
            .commands
            .iter()
            .enumerate()
            .map(|(command_index, command)| {
                (
                    format!("{group_id}-c{command_index}"),
                    command.name.clone(),
                    command.template.clone(),
                )
            })
            .collect();
        imported += store
            .import_command_group(&group_id, &group.name, &commands)
            .map_err(|e| e.to_string())?;
    }
    Ok(imported)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WasmRunResult {
    output: String,
    logs: Vec<String>,
    fuel_used: u64,
}

#[tauri::command]
pub async fn plugin_run_wasm(
    plugins: State<'_, ft_plugin::PluginRegistry>,
    id: String,
    payload: String,
) -> Result<WasmRunResult, String> {
    let (_, bytes) = plugins.load_wasm(&id)?;
    tauri::async_runtime::spawn_blocking(move || {
        ft_plugin::run_wasm(
            &bytes,
            payload.as_bytes(),
            &ft_plugin::WasmLimits::default(),
        )
    })
    .await
    .map_err(|err| err.to_string())?
    .map(|run| WasmRunResult {
        output: run.output,
        logs: run.logs,
        fuel_used: run.fuel_used,
    })
}

const MARKET_INDEX_CAP: usize = 512 * 1024;
const MARKET_MANIFEST_CAP: usize = 256 * 1024;

/// Fetches a marketplace document. Only https URLs are accepted, and the body
/// is read in chunks so an oversized or endless response cannot exhaust memory.
async fn fetch_market_text(url: &str, cap: usize) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent(concat!("Ferriterm/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())?;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("marketplace request failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("marketplace returned HTTP {}", response.status()));
    }
    if response
        .content_length()
        .is_some_and(|len| len as usize > cap)
    {
        return Err("marketplace response is too large".to_string());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if body.len() + chunk.len() > cap {
            return Err("marketplace response is too large".to_string());
        }
        body.extend_from_slice(&chunk);
    }
    String::from_utf8(body).map_err(|_| "marketplace response is not UTF-8".to_string())
}

#[tauri::command]
pub async fn plugin_market_fetch(
    index_url: Option<String>,
) -> Result<ft_plugin::MarketIndex, String> {
    let url = index_url
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| ft_plugin::DEFAULT_MARKET_URL.to_string());
    let raw = fetch_market_text(&url, MARKET_INDEX_CAP).await?;
    ft_plugin::parse_market_index(&raw)
}

#[tauri::command]
pub async fn plugin_market_install(
    plugins: State<'_, ft_plugin::PluginRegistry>,
    id: String,
    manifest_url: String,
) -> Result<ft_plugin::PluginManifest, String> {
    let raw = fetch_market_text(&manifest_url, MARKET_MANIFEST_CAP).await?;
    plugins.install(&id, &raw)
}

#[tauri::command]
pub fn plugin_market_uninstall(
    plugins: State<'_, ft_plugin::PluginRegistry>,
    id: String,
) -> Result<bool, String> {
    plugins.remove(&id)
}

#[tauri::command]
pub fn plugin_install_builtin(
    plugins: State<'_, ft_plugin::PluginRegistry>,
    id: String,
) -> Result<ft_plugin::PluginManifest, String> {
    plugins.install_builtin(&id)
}

fn serial_plugin_installed(plugins: &ft_plugin::PluginRegistry) -> bool {
    plugins.scan().plugins.iter().any(|plugin| {
        plugin.kind == "serial" && plugin.permissions.iter().any(|perm| perm == "serial")
    })
}

fn telnet_plugin_installed(plugins: &ft_plugin::PluginRegistry) -> bool {
    plugins.scan().plugins.iter().any(|plugin| {
        plugin.kind == "telnet" && plugin.permissions.iter().any(|perm| perm == "telnet")
    })
}

#[tauri::command]
pub fn serial_list_ports() -> Result<Vec<ft_serial::SerialPortInfo>, String> {
    ft_serial::list_ports()
}
#[tauri::command]
pub async fn serial_open(
    app: AppHandle,
    plugins: State<'_, ft_plugin::PluginRegistry>,
    state: State<'_, ft_serial::SerialRegistry>,
    recorders: State<'_, RecorderRegistry>,
    id: String,
    config: ft_serial::SerialConfig,
) -> Result<(), String> {
    if !serial_plugin_installed(&plugins) {
        return Err("serial support requires the Serial Ports plugin".to_string());
    }
    let sink = Arc::new(TauriSink {
        app,
        recorders: recorders.inner().clone(),
    });
    ft_serial::open(sink, &state, id, config).await
}

#[tauri::command]
pub async fn serial_write(
    state: State<'_, ft_serial::SerialRegistry>,
    id: String,
    data: String,
) -> Result<(), String> {
    ft_serial::write(&state, &id, data).await
}

#[tauri::command]
pub async fn serial_close(
    state: State<'_, ft_serial::SerialRegistry>,
    id: String,
) -> Result<(), String> {
    ft_serial::close(&state, &id).await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn telnet_open(
    app: AppHandle,
    plugins: State<'_, ft_plugin::PluginRegistry>,
    state: State<'_, ft_telnet::TelnetRegistry>,
    recorders: State<'_, RecorderRegistry>,
    id: String,
    host: String,
    port: u16,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    if !telnet_plugin_installed(&plugins) {
        return Err("telnet support requires the Telnet plugin".to_string());
    }
    let sink = Arc::new(TauriSink {
        app,
        recorders: recorders.inner().clone(),
    });
    ft_telnet::open(sink, &state, id, host, port, cols, rows).await
}

#[tauri::command]
pub async fn telnet_write(
    state: State<'_, ft_telnet::TelnetRegistry>,
    id: String,
    data: String,
) -> Result<(), String> {
    ft_telnet::write(&state, &id, data).await
}

#[tauri::command]
pub async fn telnet_resize(
    state: State<'_, ft_telnet::TelnetRegistry>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    ft_telnet::resize(&state, &id, cols, rows).await
}

#[tauri::command]
pub async fn telnet_close(
    state: State<'_, ft_telnet::TelnetRegistry>,
    id: String,
) -> Result<(), String> {
    ft_telnet::close(&state, &id).await
}

#[tauri::command]
pub fn export_backup(
    backups: State<'_, ft_sync::BackupStore>,
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    passphrase: String,
    include_secrets: bool,
) -> Result<String, String> {
    let payload = backup_payload(&store, Some(&credentials), include_secrets)?;
    backups
        .export(&passphrase, &payload)
        .map(|path| path.display().to_string())
        .map_err(|e| e.to_string())
}

fn backup_payload(
    store: &ft_store::Store,
    credentials: Option<&ft_security::CredentialStore>,
    include_secrets: bool,
) -> Result<Vec<u8>, String> {
    let hosts = store.list_hosts().map_err(|e| e.to_string())?;
    let tags = store.list_tags().map_err(|e| e.to_string())?;
    let mut secrets = Vec::new();
    if include_secrets {
        if let Some(credentials) = credentials {
            let labels = store
                .list_vault_entries()
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|entry| (entry.secret_ref.clone(), entry))
                .collect::<std::collections::HashMap<_, _>>();
            for reference in credentials.refs() {
                if reference.starts_with("sync:") {
                    continue;
                }
                let Some(secret) = credentials.get(&reference).map_err(|e| e.to_string())? else {
                    continue;
                };
                let meta = labels.get(&reference);
                secrets.push(BackupSecret {
                    kind: meta
                        .map(|entry| entry.kind.clone())
                        .unwrap_or_else(|| infer_kind(&reference).to_string()),
                    label: meta
                        .map(|entry| entry.label.clone())
                        .unwrap_or_else(|| reference.clone()),
                    host_id: meta
                        .and_then(|entry| entry.host_id.clone())
                        .or_else(|| infer_host_id(&reference)),
                    secret_ref: reference,
                    secret,
                });
            }
        }
    }
    let exported_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    serde_json::to_vec(&serde_json::json!({
        "version": 1,
        "exportedAt": exported_at,
        "tags": tags,
        "hosts": hosts,
        "secrets": secrets,
    }))
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn import_backup(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    payload: String,
    passphrase: String,
) -> Result<ft_store::ImportStats, String> {
    let plain = ft_sync::open(&passphrase, &payload).map_err(|e| e.to_string())?;
    let parsed: BackupPayload =
        serde_json::from_slice(&plain).map_err(|e| format!("invalid backup payload: {e}"))?;
    let stats = store
        .import_snapshot(&parsed.hosts, &parsed.tags)
        .map_err(|e| e.to_string())?;
    import_backup_secrets(&store, &credentials, &parsed.secrets)?;
    Ok(stats)
}

fn import_backup_secrets(
    store: &ft_store::Store,
    credentials: &ft_security::CredentialStore,
    secrets: &[BackupSecret],
) -> Result<(), String> {
    for entry in secrets {
        if entry.secret.is_empty() {
            continue;
        }
        credentials
            .set(&entry.secret_ref, &entry.secret)
            .map_err(|e| e.to_string())?;
        store
            .upsert_vault_entry(
                &entry.secret_ref,
                &entry.kind,
                &entry.label,
                entry.host_id.as_deref(),
            )
            .map_err(|e| e.to_string())?;
    }
    if !secrets.is_empty() {
        let _ = store.append_audit(
            "backup.import.secrets",
            "backup",
            &secrets.len().to_string(),
        );
    }
    Ok(())
}

#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RemoteConfig {
    #[serde(rename_all = "camelCase")]
    Gist {
        token: String,
        #[serde(default)]
        gist_id: String,
        #[serde(default)]
        filename: String,
    },
    #[serde(rename_all = "camelCase")]
    Webdav {
        url: String,
        #[serde(default)]
        username: String,
        #[serde(default)]
        password: String,
        #[serde(default)]
        filename: String,
    },
}

impl RemoteConfig {
    /// Fills empty credentials from the vault so the webview never needs to
    /// hold the stored token or password.
    fn resolve_secrets(mut self, credentials: &Credentials) -> Result<Self, String> {
        match &mut self {
            RemoteConfig::Gist { token, .. } if token.trim().is_empty() => {
                *token = credentials
                    .get("sync:gist-token")
                    .map_err(|e| e.to_string())?
                    .unwrap_or_default();
            }
            RemoteConfig::Webdav { password, .. } if password.is_empty() => {
                *password = credentials
                    .get("sync:webdav-password")
                    .map_err(|e| e.to_string())?
                    .unwrap_or_default();
            }
            _ => {}
        }
        Ok(self)
    }

    fn gist(&self) -> Option<ft_sync::GistConfig> {
        match self {
            RemoteConfig::Gist {
                token,
                gist_id,
                filename,
            } => {
                let mut config = ft_sync::GistConfig::new(token, gist_id);
                if !filename.trim().is_empty() {
                    config.filename = filename.trim().to_string();
                }
                Some(config)
            }
            RemoteConfig::Webdav { .. } => None,
        }
    }

    fn webdav(&self) -> Option<ft_sync::WebdavConfig> {
        match self {
            RemoteConfig::Webdav {
                url,
                username,
                password,
                filename,
            } => {
                let mut config = ft_sync::WebdavConfig::new(url, username, password);
                if !filename.trim().is_empty() {
                    config.filename = filename.trim().to_string();
                }
                Some(config)
            }
            RemoteConfig::Gist { .. } => None,
        }
    }
}

#[tauri::command]
pub async fn sync_remote_test(
    credentials: State<'_, Credentials>,
    config: RemoteConfig,
) -> Result<String, String> {
    let config = config.resolve_secrets(&credentials)?;
    if let Some(gist) = config.gist() {
        ft_sync::gist_test(&gist)
            .await
            .map(|login| format!("GitHub user {login}"))
            .map_err(|e| e.to_string())
    } else if let Some(webdav) = config.webdav() {
        ft_sync::webdav_test(&webdav)
            .await
            .map_err(|e| e.to_string())
    } else {
        Err("unsupported sync provider".to_string())
    }
}

#[tauri::command]
pub async fn sync_remote_upload(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    config: RemoteConfig,
    passphrase: String,
    include_secrets: bool,
) -> Result<ft_sync::RemoteUpload, String> {
    let config = config.resolve_secrets(&credentials)?;
    let payload = backup_payload(&store, Some(&credentials), include_secrets)?;
    let sealed = ft_sync::seal(&passphrase, &payload).map_err(|e| e.to_string())?;
    if let Some(gist) = config.gist() {
        ft_sync::gist_upload(&gist, &sealed)
            .await
            .map_err(|e| e.to_string())
    } else if let Some(webdav) = config.webdav() {
        ft_sync::webdav_upload(&webdav, &sealed)
            .await
            .map_err(|e| e.to_string())
    } else {
        Err("unsupported sync provider".to_string())
    }
}

#[tauri::command]
pub async fn sync_remote_download(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    config: RemoteConfig,
    passphrase: String,
) -> Result<ft_store::ImportStats, String> {
    let config = config.resolve_secrets(&credentials)?;
    let sealed = if let Some(gist) = config.gist() {
        ft_sync::gist_download(&gist).await
    } else if let Some(webdav) = config.webdav() {
        ft_sync::webdav_download(&webdav).await
    } else {
        return Err("unsupported sync provider".to_string());
    }
    .map_err(|e| e.to_string())?;
    let plain = ft_sync::open(&passphrase, &sealed).map_err(|e| e.to_string())?;
    let parsed: BackupPayload =
        serde_json::from_slice(&plain).map_err(|e| format!("invalid backup payload: {e}"))?;
    let stats = store
        .import_snapshot(&parsed.hosts, &parsed.tags)
        .map_err(|e| e.to_string())?;
    import_backup_secrets(&store, &credentials, &parsed.secrets)?;
    Ok(stats)
}

type Credentials = Arc<ft_security::CredentialStore>;

/// Reads secrets from the vault inside the Rust process. The webview never
/// receives stored plaintext.
struct VaultProvider {
    credentials: Credentials,
}

impl VaultProvider {
    fn lookup(&self, references: &[String]) -> Option<String> {
        for reference in references {
            if let Ok(Some(secret)) = self.credentials.get(reference) {
                if !secret.is_empty() {
                    return Some(secret);
                }
            }
        }
        None
    }
}

impl CredentialProvider for VaultProvider {
    fn host_secret(&self, host_id: &str, kind: SecretKind) -> Option<String> {
        match kind {
            SecretKind::Password => self.lookup(&[format!("pwd:{host_id}"), host_id.to_string()]),
            SecretKind::Passphrase => self.lookup(&[format!("pp:{host_id}"), host_id.to_string()]),
        }
    }

    fn key_material(&self, key_id: &str) -> Option<(String, Option<String>)> {
        let text = self
            .credentials
            .get(&format!("key:{key_id}"))
            .ok()
            .flatten()?;
        let passphrase = self
            .credentials
            .get(&format!("keypp:{key_id}"))
            .ok()
            .flatten()
            .filter(|value| !value.is_empty());
        Some((text, passphrase))
    }

    fn locked(&self) -> bool {
        self.credentials.is_locked()
    }
}

fn password_ref(host_id: &str) -> String {
    format!("pwd:{host_id}")
}

fn passphrase_ref(host_id: &str) -> String {
    format!("pp:{host_id}")
}

fn key_ref(key_id: &str) -> String {
    format!("key:{key_id}")
}

fn key_passphrase_ref(key_id: &str) -> String {
    format!("keypp:{key_id}")
}

fn infer_kind(reference: &str) -> &'static str {
    if reference.starts_with("pwd:") {
        "password"
    } else if reference.starts_with("pp:") {
        "passphrase"
    } else if reference.starts_with("keypp:") {
        "keyPassphrase"
    } else if reference.starts_with("key:") {
        "key"
    } else if reference.starts_with("sync:") {
        "token"
    } else {
        "password"
    }
}

fn infer_host_id(reference: &str) -> Option<String> {
    reference
        .strip_prefix("pwd:")
        .or_else(|| reference.strip_prefix("pp:"))
        .map(str::to_string)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialStatus {
    pub password: bool,
    pub passphrase: bool,
    pub key: bool,
    pub key_passphrase: bool,
    pub locked: bool,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultEntryView {
    pub secret_ref: String,
    pub kind: String,
    pub label: String,
    pub host_id: Option<String>,
    pub updated_at: String,
}

#[tauri::command]
pub fn credential_status(
    credentials: State<'_, Credentials>,
    host_id: String,
    key_id: Option<String>,
) -> CredentialStatus {
    CredentialStatus {
        password: credentials.exists(&password_ref(&host_id)) || credentials.exists(&host_id),
        passphrase: credentials.exists(&passphrase_ref(&host_id)) || credentials.exists(&host_id),
        key: key_id
            .as_deref()
            .filter(|id| !id.is_empty())
            .is_some_and(|id| credentials.exists(&key_ref(id))),
        key_passphrase: key_id
            .as_deref()
            .filter(|id| !id.is_empty())
            .is_some_and(|id| credentials.exists(&key_passphrase_ref(id))),
        locked: credentials.is_locked(),
    }
}

/// Legacy entry point kept for older payloads; stores a password under the
/// new reference scheme.
#[tauri::command]
pub fn save_credential(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    host_id: String,
    secret: String,
) -> Result<(), String> {
    apply_host_secrets(&store, &credentials, &host_id, Some(secret), None)
}

#[tauri::command]
pub fn delete_credential(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    host_id: String,
) -> Result<(), String> {
    for reference in [
        password_ref(&host_id),
        passphrase_ref(&host_id),
        host_id.clone(),
    ] {
        credentials.delete(&reference).map_err(|e| e.to_string())?;
        let _ = store.delete_vault_entry(&reference);
    }
    let _ = store.append_audit("secret.delete", &host_id, "");
    Ok(())
}

/// Saves or clears a host's password/passphrase. `None` leaves the slot
/// untouched; an empty string removes it.
#[tauri::command]
pub fn save_host_credential(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    host_id: String,
    password: Option<String>,
    passphrase: Option<String>,
) -> Result<(), String> {
    apply_host_secrets(&store, &credentials, &host_id, password, passphrase)
}

fn apply_host_secrets(
    store: &ft_store::Store,
    credentials: &ft_security::CredentialStore,
    host_id: &str,
    password: Option<String>,
    passphrase: Option<String>,
) -> Result<(), String> {
    let host = store
        .list_hosts()
        .ok()
        .and_then(|hosts| hosts.into_iter().find(|host| host.id == host_id));
    let label = host
        .as_ref()
        .map(|host| host.name.clone())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| host_id.to_string());
    let refs = [
        (password_ref(host_id), "password", password),
        (passphrase_ref(host_id), "passphrase", passphrase),
    ];
    for (reference, kind, value) in refs {
        match value {
            None => {}
            Some(secret) if secret.is_empty() => {
                credentials.delete(&reference).map_err(|e| e.to_string())?;
                let _ = store.delete_vault_entry(&reference);
            }
            Some(secret) => {
                credentials
                    .set(&reference, &secret)
                    .map_err(|e| e.to_string())?;
                store
                    .upsert_vault_entry(&reference, kind, &label, Some(host_id))
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    let _ = store.append_audit("secret.save", host_id, "");
    Ok(())
}

#[tauri::command]
pub fn save_secret(
    credentials: State<'_, Credentials>,
    secret_ref: String,
    secret: String,
) -> Result<(), String> {
    credentials
        .set(&secret_ref, &secret)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn secret_saved(credentials: State<'_, Credentials>, secret_ref: String) -> bool {
    credentials.exists(&secret_ref)
}

#[tauri::command]
pub fn delete_secret(
    credentials: State<'_, Credentials>,
    secret_ref: String,
) -> Result<(), String> {
    credentials.delete(&secret_ref).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn vault_status(credentials: State<'_, Credentials>) -> ft_security::VaultStatus {
    credentials.status()
}

#[tauri::command]
pub fn vault_unlock(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    password: String,
) -> Result<(), String> {
    let result = credentials.unlock(&password);
    let detail = result
        .as_ref()
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    let _ = store.append_audit(
        if result.is_ok() {
            "vault.unlock"
        } else {
            "vault.unlock.failed"
        },
        "vault",
        &detail,
    );
    result.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn vault_lock(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
) -> Result<(), String> {
    credentials.lock();
    let _ = store.append_audit("vault.lock", "vault", "");
    Ok(())
}

#[tauri::command]
pub fn vault_enable_master_password(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    password: String,
) -> Result<usize, String> {
    let migrated = credentials
        .enable_master_password(&password, &known_secret_refs(&store))
        .map_err(|e| e.to_string())?;
    let _ = store.append_audit("vault.enable", "vault", &format!("{migrated} secrets"));
    Ok(migrated)
}

#[tauri::command]
pub fn vault_disable_master_password(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    password: String,
) -> Result<(), String> {
    credentials
        .disable_master_password(&password)
        .map_err(|e| e.to_string())?;
    let _ = store.append_audit("vault.disable", "vault", "");
    Ok(())
}

#[tauri::command]
pub fn vault_change_master_password(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    current: String,
    next: String,
) -> Result<(), String> {
    credentials
        .change_master_password(&current, &next)
        .map_err(|e| e.to_string())?;
    let _ = store.append_audit("vault.password.change", "vault", "");
    Ok(())
}

/// Destroys the master password and every stored secret. Unrecoverable.
#[tauri::command]
pub fn vault_reset(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
) -> Result<(), String> {
    credentials.reset().map_err(|e| e.to_string())?;
    let _ = store.clear_vault_entries();
    let _ = store.append_audit("vault.reset", "vault", "all secrets removed");
    Ok(())
}

#[tauri::command]
pub fn vault_list_entries(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
) -> Result<Vec<VaultEntryView>, String> {
    let hosts = store.list_hosts().unwrap_or_default();
    let keys = store.list_ssh_keys().unwrap_or_default();
    let refs = credentials.refs();
    let mut views: Vec<VaultEntryView> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for entry in store.list_vault_entries().unwrap_or_default() {
        if !refs.iter().any(|item| item == &entry.secret_ref) {
            continue;
        }
        seen.insert(entry.secret_ref.clone());
        views.push(VaultEntryView {
            secret_ref: entry.secret_ref,
            kind: entry.kind,
            label: entry.label,
            host_id: entry.host_id,
            updated_at: entry.updated_at,
        });
    }
    for reference in refs {
        if !seen.insert(reference.clone()) {
            continue;
        }
        let host_id = infer_host_id(&reference);
        let key_id = reference
            .strip_prefix("keypp:")
            .or_else(|| reference.strip_prefix("key:"))
            .map(str::to_string);
        let label = host_id
            .as_ref()
            .and_then(|id| hosts.iter().find(|host| &host.id == id))
            .map(|host| host.name.clone())
            .or_else(|| {
                key_id
                    .as_ref()
                    .and_then(|id| keys.iter().find(|key| &key.id == id))
                    .map(|key| key.name.clone())
            })
            .unwrap_or_else(|| reference.clone());
        views.push(VaultEntryView {
            kind: infer_kind(&reference).to_string(),
            label,
            host_id,
            updated_at: String::new(),
            secret_ref: reference,
        });
    }
    Ok(views)
}

#[tauri::command]
pub fn vault_delete_entry(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    secret_ref: String,
) -> Result<(), String> {
    credentials.delete(&secret_ref).map_err(|e| e.to_string())?;
    let _ = store.delete_vault_entry(&secret_ref);
    let _ = store.append_audit("secret.delete", &secret_ref, "");
    Ok(())
}

#[tauri::command]
pub fn vault_clear_entries(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
) -> Result<usize, String> {
    let mut refs = credentials.refs();
    for entry in store.list_vault_entries().unwrap_or_default() {
        if !refs.contains(&entry.secret_ref) {
            refs.push(entry.secret_ref);
        }
    }
    let mut removed = 0;
    for reference in refs {
        if credentials.delete(&reference).is_ok() {
            removed += 1;
        }
    }
    let _ = store.clear_vault_entries();
    let _ = store.append_audit("vault.clear", "vault", &format!("{removed} secrets"));
    Ok(removed)
}

fn known_secret_refs(store: &ft_store::Store) -> Vec<String> {
    let mut refs = Vec::new();
    if let Ok(hosts) = store.list_hosts() {
        for host in hosts {
            refs.push(host.id.clone());
            refs.push(password_ref(&host.id));
            refs.push(passphrase_ref(&host.id));
        }
    }
    if let Ok(keys) = store.list_ssh_keys() {
        for key in keys {
            refs.push(key_ref(&key.id));
            refs.push(key_passphrase_ref(&key.id));
        }
    }
    refs.push("sync:gist-token".to_string());
    refs.push("sync:webdav-password".to_string());
    refs
}

#[tauri::command]
pub fn ssh_key_list(
    store: State<'_, Arc<ft_store::Store>>,
) -> Result<Vec<ft_store::SshKey>, String> {
    store.list_ssh_keys().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn ssh_key_scan(dir: Option<String>) -> Vec<ft_ssh::KeyCandidate> {
    ft_ssh::scan_key_candidates(dir.as_deref())
}

#[tauri::command]
pub fn ssh_key_inspect(text: String) -> Result<ft_ssh::KeyInfo, String> {
    ft_ssh::inspect_private_key(&text)
}

#[tauri::command]
pub fn ssh_key_import(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    id: String,
    name: String,
    path: Option<String>,
    text: Option<String>,
    passphrase: Option<String>,
) -> Result<ft_store::SshKey, String> {
    let raw = match (path.as_deref(), text.as_deref()) {
        (Some(path), _) if !path.trim().is_empty() => ft_ssh::read_key_file(path)?,
        (_, Some(text)) if !text.trim().is_empty() => text.to_string(),
        _ => return Err("provide a key file path or pasted key content".to_string()),
    };
    let info = ft_ssh::inspect_private_key(&raw)?;
    let fallback_name = path
        .as_deref()
        .and_then(|path| std::path::Path::new(path).file_name())
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| info.fingerprint.chars().take(19).collect());
    let key = ft_store::SshKey {
        id: id.clone(),
        name: if name.trim().is_empty() {
            fallback_name
        } else {
            name.trim().to_string()
        },
        algorithm: info.algorithm,
        fingerprint: info.fingerprint,
        public_key: info.public_key,
        comment: info.comment,
        source_path: path.unwrap_or_default(),
        has_passphrase: info.encrypted,
        created_at: String::new(),
        updated_at: String::new(),
        notes: String::new(),
        expires_at: String::new(),
    };
    credentials
        .set(&key_ref(&id), &raw)
        .map_err(|e| e.to_string())?;
    if let Some(passphrase) = passphrase.filter(|value| !value.is_empty()) {
        credentials
            .set(&key_passphrase_ref(&id), &passphrase)
            .map_err(|e| e.to_string())?;
    }
    store.save_ssh_key(&key).map_err(|e| e.to_string())?;
    store
        .upsert_vault_entry(&key_ref(&id), "key", &key.name, None)
        .map_err(|e| e.to_string())?;
    if key.has_passphrase {
        let _ =
            store.upsert_vault_entry(&key_passphrase_ref(&id), "keyPassphrase", &key.name, None);
    }
    let _ = store.append_audit("key.import", &key.name, &key.fingerprint);
    Ok(key)
}

#[tauri::command]
pub fn ssh_key_generate(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    id: String,
    name: String,
    algorithm: String,
    passphrase: Option<String>,
    comment: String,
) -> Result<ft_store::SshKey, String> {
    let passphrase = passphrase.filter(|value| !value.is_empty());
    let (private, info) = ft_ssh::generate_key(&algorithm, passphrase.as_deref(), &comment)?;
    let key = ft_store::SshKey {
        id: id.clone(),
        name: if name.trim().is_empty() {
            format!("{} key", info.algorithm)
        } else {
            name.trim().to_string()
        },
        algorithm: info.algorithm,
        fingerprint: info.fingerprint,
        public_key: info.public_key,
        comment: info.comment,
        source_path: String::new(),
        has_passphrase: info.encrypted,
        created_at: String::new(),
        updated_at: String::new(),
        notes: String::new(),
        expires_at: String::new(),
    };
    credentials
        .set(&key_ref(&id), &private)
        .map_err(|e| e.to_string())?;
    if let Some(passphrase) = passphrase {
        credentials
            .set(&key_passphrase_ref(&id), &passphrase)
            .map_err(|e| e.to_string())?;
    }
    store.save_ssh_key(&key).map_err(|e| e.to_string())?;
    let _ = store.upsert_vault_entry(&key_ref(&id), "key", &key.name, None);
    if key.has_passphrase {
        let _ =
            store.upsert_vault_entry(&key_passphrase_ref(&id), "keyPassphrase", &key.name, None);
    }
    let _ = store.append_audit("key.generate", &key.name, &key.algorithm);
    Ok(key)
}

#[tauri::command]
pub fn ssh_key_delete(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    id: String,
) -> Result<(), String> {
    let label = store
        .list_ssh_keys()
        .ok()
        .and_then(|keys| keys.into_iter().find(|key| key.id == id))
        .map(|key| key.name)
        .unwrap_or_else(|| id.clone());
    for reference in [key_ref(&id), key_passphrase_ref(&id)] {
        if let Err(err) = credentials.delete(&reference) {
            let _ = store.append_audit("key.delete.failed", &label, &err.to_string());
        }
        let _ = store.delete_vault_entry(&reference);
    }
    store.delete_ssh_key(&id).map_err(|e| e.to_string())?;
    let _ = store.append_audit("key.delete", &label, "");
    Ok(())
}

/// Bundle format for managed SSH keys. Versioned so an older build can reject
/// a newer layout instead of silently importing partial data. Private keys are
/// only present in the passphrase-sealed variant.
const KEY_BUNDLE_FORMAT: &str = "ferriterm-ssh-keys";
const KEY_BUNDLE_VERSION: u32 = 1;

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeyBundleEntry {
    name: String,
    algorithm: String,
    fingerprint: String,
    public_key: String,
    comment: String,
    source_path: String,
    has_passphrase: bool,
    notes: String,
    expires_at: String,
    #[serde(default)]
    private_key: String,
    #[serde(default)]
    passphrase: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeyBundle {
    format: String,
    version: u32,
    exported_at: String,
    keys: Vec<KeyBundleEntry>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyBundleStats {
    pub imported: usize,
    pub skipped: usize,
}

/// Accepts only a plain ISO date (YYYY-MM-DD) or an empty value, so a typo can
/// never be stored as an unparseable reminder.
fn normalize_expiry(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        return String::new();
    }
    let bytes = value.as_bytes();
    let shape_ok = bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit());
    if !shape_ok {
        return String::new();
    }
    // Range-check the components: a bundle from another install can carry any
    // string, and "2027-13-45" must not become a stored reminder.
    let month: u32 = value[5..7].parse().unwrap_or(0);
    let day: u32 = value[8..10].parse().unwrap_or(0);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return String::new();
    }
    value.to_string()
}

fn key_bundle_entries(
    store: &ft_store::Store,
    credentials: Option<&ft_security::CredentialStore>,
) -> Result<Vec<KeyBundleEntry>, String> {
    let keys = store.list_ssh_keys().map_err(|e| e.to_string())?;
    let mut entries = Vec::with_capacity(keys.len());
    for key in keys {
        let (private_key, passphrase) = if let Some(credentials) = credentials {
            let private = credentials
                .get(&key_ref(&key.id))
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            let passphrase = credentials
                .get(&key_passphrase_ref(&key.id))
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            (private, passphrase)
        } else {
            (String::new(), String::new())
        };
        let include_private = credentials.is_some();
        // A metadata-only entry is still useful even when the vault is locked;
        // a private-key entry without its key material is not.
        if include_private && private_key.trim().is_empty() {
            continue;
        }
        entries.push(KeyBundleEntry {
            name: key.name,
            algorithm: key.algorithm,
            fingerprint: key.fingerprint,
            public_key: key.public_key,
            comment: key.comment,
            source_path: key.source_path,
            has_passphrase: key.has_passphrase,
            notes: key.notes,
            expires_at: key.expires_at,
            private_key,
            passphrase,
        });
    }
    Ok(entries)
}

fn utc_now() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Format without pulling in a date crate: day-number to civil date (Howard
    // Hinnant's algorithm), good enough for a human-readable export stamp.
    let days = (seconds / 86_400) as i64;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    format!("{year:04}-{month:02}-{day:02}")
}

fn key_bundle_payload(
    store: &ft_store::Store,
    credentials: Option<&ft_security::CredentialStore>,
) -> Result<Vec<u8>, String> {
    let bundle = KeyBundle {
        format: KEY_BUNDLE_FORMAT.to_string(),
        version: KEY_BUNDLE_VERSION,
        exported_at: utc_now(),
        keys: key_bundle_entries(store, credentials)?,
    };
    serde_json::to_vec_pretty(&bundle).map_err(|e| e.to_string())
}

/// Plain, unencrypted JSON: public keys plus metadata only. Safe to paste into
/// an issue tracker or a config repo. Never contains private key material.
#[tauri::command]
pub fn ssh_key_export_public(
    store: State<'_, Arc<ft_store::Store>>,
) -> Result<String, String> {
    let payload = key_bundle_payload(&store, None)?;
    String::from_utf8(payload).map_err(|e| e.to_string())
}

/// Passphrase-sealed backup containing private keys. Reuses the same
/// PBKDF2 + ChaCha20-Poly1305 FTBAK1 envelope as the full sync backup, so no
/// new crypto is introduced and the vault cipher stays untouched.
#[tauri::command]
pub fn ssh_key_export_bundle(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    passphrase: String,
) -> Result<String, String> {
    if passphrase.trim().is_empty() {
        return Err("a passphrase is required to export private keys".to_string());
    }
    let payload = key_bundle_payload(&store, Some(&credentials))?;
    let sealed = ft_sync::seal(&passphrase, &payload).map_err(|e| e.to_string())?;
    let _ = store.append_audit("key.export", "bundle", "");
    Ok(sealed)
}

#[tauri::command]
pub fn ssh_key_import_bundle(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    payload: String,
    passphrase: String,
) -> Result<KeyBundleStats, String> {
    let plain = ft_sync::open(&passphrase, &payload).map_err(|e| e.to_string())?;
    let bundle: KeyBundle =
        serde_json::from_slice(&plain).map_err(|e| format!("invalid key bundle: {e}"))?;
    if bundle.format != KEY_BUNDLE_FORMAT || bundle.version != KEY_BUNDLE_VERSION {
        return Err("unsupported key bundle format or version".to_string());
    }
    let existing: HashSet<String> = store
        .list_ssh_keys()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|key| key.fingerprint)
        .collect();
    let mut imported = 0usize;
    let mut skipped = 0usize;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    for (index, entry) in bundle.keys.into_iter().enumerate() {
        if entry.private_key.trim().is_empty() || existing.contains(&entry.fingerprint) {
            skipped += 1;
            continue;
        }
        let info = match ft_ssh::inspect_private_key(&entry.private_key) {
            Ok(info) => info,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        let id = format!("kb-{stamp}-{index}");
        let key = ft_store::SshKey {
            id: id.clone(),
            name: if entry.name.trim().is_empty() {
                format!("{} key", info.algorithm)
            } else {
                entry.name.trim().to_string()
            },
            algorithm: info.algorithm,
            fingerprint: info.fingerprint,
            public_key: info.public_key,
            comment: info.comment,
            source_path: entry.source_path,
            has_passphrase: info.encrypted,
            created_at: String::new(),
            updated_at: String::new(),
            notes: entry.notes.chars().take(2000).collect(),
            expires_at: normalize_expiry(&entry.expires_at),
        };
        credentials
            .set(&key_ref(&id), &entry.private_key)
            .map_err(|e| e.to_string())?;
        if !entry.passphrase.is_empty() {
            credentials
                .set(&key_passphrase_ref(&id), &entry.passphrase)
                .map_err(|e| e.to_string())?;
        }
        store.save_ssh_key(&key).map_err(|e| e.to_string())?;
        let _ = store.upsert_vault_entry(&key_ref(&id), "key", &key.name, None);
        if key.has_passphrase {
            let _ = store.upsert_vault_entry(
                &key_passphrase_ref(&id),
                "keyPassphrase",
                &key.name,
                None,
            );
        }
        imported += 1;
    }
    let _ = store.append_audit("key.import.bundle", "bundle", &imported.to_string());
    Ok(KeyBundleStats { imported, skipped })
}

#[tauri::command]
pub fn ssh_key_update_metadata(
    store: State<'_, Arc<ft_store::Store>>,
    id: String,
    notes: String,
    expires_at: String,
) -> Result<ft_store::SshKey, String> {
    let notes: String = notes.trim().chars().take(2000).collect();
    let expires_at = normalize_expiry(&expires_at);
    let changed = store
        .update_ssh_key_metadata(&id, &notes, &expires_at)
        .map_err(|e| e.to_string())?;
    if !changed {
        return Err("key not found".to_string());
    }
    let key = store
        .list_ssh_keys()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|key| key.id == id)
        .ok_or_else(|| "key not found".to_string())?;
    let _ = store.append_audit("key.metadata", &key.name, &expires_at);
    Ok(key)
}

/// Regenerates a managed key in place. The id is deliberately preserved so
/// every host that references `keyId` keeps working; only the vault secret and
/// the public fingerprint change. Requires the new key's passphrase when the
/// old one was encrypted, otherwise the vault would hold an unusable key.
#[tauri::command]
pub fn ssh_key_rotate(
    store: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    id: String,
    passphrase: String,
    comment: String,
) -> Result<ft_store::SshKey, String> {
    let current = store
        .list_ssh_keys()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|key| key.id == id)
        .ok_or_else(|| "key not found".to_string())?;
    let algorithm = match current.algorithm.as_str() {
        "ssh-ed25519" | "ed25519" => "ed25519",
        "ssh-rsa" | "rsa-sha2-256" | "rsa-sha2-512" => "rsa",
        "ecdsa-sha2-nistp256" | "ecdsa-sha2-nistp384" | "ecdsa-sha2-nistp521" => "ecdsa",
        other => {
            return Err(format!(
                "rotation is not supported for key algorithm {other}"
            ))
        }
    };
    let new_passphrase = passphrase.trim();
    if current.has_passphrase && new_passphrase.is_empty() {
        return Err("the existing key is encrypted; enter a passphrase for the new key".to_string());
    }
    let new_comment = if comment.trim().is_empty() {
        current.comment.clone()
    } else {
        comment.trim().to_string()
    };
    let (private, info) = ft_ssh::generate_key(
        algorithm,
        if new_passphrase.is_empty() {
            None
        } else {
            Some(new_passphrase)
        },
        &new_comment,
    )?;
    let mut key = current;
    key.algorithm = info.algorithm;
    key.fingerprint = info.fingerprint;
    key.public_key = info.public_key;
    key.comment = info.comment;
    key.has_passphrase = info.encrypted;
    // Rotation starts a fresh validity period; notes are operator-owned and
    // survive, but an old expiry date would immediately flag the new key.
    key.expires_at = String::new();
    credentials
        .set(&key_ref(&key.id), &private)
        .map_err(|e| e.to_string())?;
    if new_passphrase.is_empty() {
        let _ = credentials.delete(&key_passphrase_ref(&key.id));
        let _ = store.delete_vault_entry(&key_passphrase_ref(&key.id));
    } else {
        credentials
            .set(&key_passphrase_ref(&key.id), new_passphrase)
            .map_err(|e| e.to_string())?;
        let _ = store.upsert_vault_entry(
            &key_passphrase_ref(&key.id),
            "keyPassphrase",
            &key.name,
            None,
        );
    }
    store.save_ssh_key(&key).map_err(|e| e.to_string())?;
    let _ = store.upsert_vault_entry(&key_ref(&key.id), "key", &key.name, None);
    let _ = store.append_audit("key.rotate", &key.name, &key.fingerprint);
    Ok(key)
}

#[tauri::command]
pub fn known_hosts_list(
    runtime: State<'_, SshRuntimeState>,
) -> Result<Vec<ft_ssh::HostKeyEntry>, String> {
    runtime.known_hosts.entries().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn known_hosts_remove(
    store: State<'_, Arc<ft_store::Store>>,
    runtime: State<'_, SshRuntimeState>,
    host: String,
    port: u16,
) -> Result<(), String> {
    runtime
        .known_hosts
        .remove(&host, port)
        .map_err(|e| e.to_string())?;
    let _ = store.append_audit("known_hosts.remove", &host, &port.to_string());
    Ok(())
}

#[tauri::command]
pub fn credential_audit_list(
    store: State<'_, Arc<ft_store::Store>>,
    limit: Option<usize>,
) -> Result<Vec<ft_store::AuditRecord>, String> {
    store
        .list_audit(limit.unwrap_or(100))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_command_groups(
    state: State<'_, Arc<ft_store::Store>>,
) -> Result<Vec<ft_store::CommandGroup>, String> {
    state.list_command_groups().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_command_group(
    state: State<'_, Arc<ft_store::Store>>,
    id: String,
    name: String,
) -> Result<(), String> {
    state
        .create_command_group(&id, &name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_command_group(
    state: State<'_, Arc<ft_store::Store>>,
    id: String,
    name: String,
) -> Result<(), String> {
    state
        .update_command_group(&id, &name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_command_group(
    state: State<'_, Arc<ft_store::Store>>,
    id: String,
) -> Result<(), String> {
    state.delete_command_group(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_command(
    state: State<'_, Arc<ft_store::Store>>,
    id: String,
    group_id: String,
    name: String,
    template: String,
) -> Result<(), String> {
    state
        .create_command(&id, &group_id, &name, &template)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_command(
    state: State<'_, Arc<ft_store::Store>>,
    id: String,
    name: String,
    template: String,
) -> Result<(), String> {
    state
        .update_command(&id, &name, &template)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_command(state: State<'_, Arc<ft_store::Store>>, id: String) -> Result<(), String> {
    state.delete_command(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_settings(
    state: State<'_, Arc<ft_store::Store>>,
) -> Result<std::collections::BTreeMap<String, String>, String> {
    state.list_settings().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_setting(
    state: State<'_, Arc<ft_store::Store>>,
    key: String,
    value: String,
) -> Result<(), String> {
    state.set_setting(&key, &value).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_hosts(state: State<'_, Arc<ft_store::Store>>) -> Result<Vec<ft_store::Host>, String> {
    state.list_hosts().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_host(
    state: State<'_, Arc<ft_store::Store>>,
    input: ft_store::HostInput,
) -> Result<(), String> {
    state.create_host(&input).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_host(
    state: State<'_, Arc<ft_store::Store>>,
    input: ft_store::HostInput,
) -> Result<(), String> {
    state.update_host(&input).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_host(
    state: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    id: String,
) -> Result<(), String> {
    for reference in [password_ref(&id), passphrase_ref(&id), id.clone()] {
        // Best effort: a failing vault delete must not block removing the host.
        if let Err(err) = credentials.delete(&reference) {
            let _ = state.append_audit("secret.delete.failed", &id, &err.to_string());
        }
        let _ = state.delete_vault_entry(&reference);
    }
    state.delete_host(&id).map_err(|e| e.to_string())?;
    let _ = state.append_audit("host.delete", &id, "");
    Ok(())
}

#[tauri::command]
pub fn list_tags(state: State<'_, Arc<ft_store::Store>>) -> Result<Vec<ft_store::Tag>, String> {
    state.list_tags().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_tag(
    state: State<'_, Arc<ft_store::Store>>,
    tag: ft_store::Tag,
) -> Result<(), String> {
    state.create_tag(&tag).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_tag(
    state: State<'_, Arc<ft_store::Store>>,
    tag: ft_store::Tag,
) -> Result<(), String> {
    state.update_tag(&tag).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_tag(state: State<'_, Arc<ft_store::Store>>, tag_id: String) -> Result<(), String> {
    state.delete_tag(&tag_id).map_err(|e| e.to_string())
}

async fn resolve_vfs(
    sessions: &SessionRegistry,
    runtime: &SshRuntimeState,
    session_id: Option<&str>,
) -> Result<Arc<dyn ft_vfs::Vfs>, String> {
    match session_id {
        None | Some("") => Ok(Arc::new(ft_vfs::LocalFs)),
        Some(id) => {
            if let Some(mount) = id.strip_prefix('@') {
                return remote_mounts()
                    .get(mount)
                    .ok_or_else(|| format!("remote mount {mount} is not connected"));
            }
            let connection = runtime.sftp.get(sessions, id).await?;
            Ok(Arc::new(ft_vfs::SftpVfs::new(connection)))
        }
    }
}

fn remote_mounts() -> &'static ft_vfs::RemoteMounts {
    static MOUNTS: std::sync::OnceLock<ft_vfs::RemoteMounts> = std::sync::OnceLock::new();
    MOUNTS.get_or_init(ft_vfs::RemoteMounts::default)
}

/// Rebuilds an end point from the session id a job was saved with. This is what
/// lets a job resumed after a restart find its host again.
struct SftpResolver {
    sessions: Arc<ft_core::SessionRegistry>,
    sftp: Arc<ft_vfs::SftpRegistry>,
}

impl ft_transfer::EndpointResolver for SftpResolver {
    fn resolve<'a>(
        &'a self,
        session: Option<&'a str>,
    ) -> ft_vfs::VfsFuture<'a, (Arc<dyn ft_vfs::Vfs>, bool)> {
        Box::pin(async move {
            match session {
                None | Some("") => Ok((Arc::new(ft_vfs::LocalFs) as Arc<dyn ft_vfs::Vfs>, false)),
                Some(id) => {
                    if let Some(mount) = id.strip_prefix('@') {
                        let vfs = remote_mounts()
                            .get(mount)
                            .ok_or_else(|| format!("remote mount {mount} is not connected"))?;
                        return Ok((vfs, true));
                    }
                    let connection = self.sftp.get(self.sessions.as_ref(), id).await?;
                    let vfs: Arc<dyn ft_vfs::Vfs> = Arc::new(ft_vfs::SftpVfs::new(connection));
                    Ok((vfs, true))
                }
            }
        })
    }
}

/// Keeps the queue in the same database as the hosts. Failure is swallowed on
/// purpose: a full disk must not stop a transfer that is already running.
struct StoreJobLog {
    store: Arc<ft_store::Store>,
}

impl ft_transfer::JobLog for StoreJobLog {
    fn save<'a>(&'a self, job: &'a ft_transfer::TransferJob) -> ft_vfs::VfsFuture<'a, ()> {
        Box::pin(async move {
            let Ok(payload) = serde_json::to_string(job) else {
                return Ok(());
            };
            let store = self.store.clone();
            let id = job.id.clone();
            let _ =
                tokio::task::spawn_blocking(move || store.save_transfer_job(&id, &payload)).await;
            Ok(())
        })
    }

    fn delete<'a>(&'a self, id: &'a str) -> ft_vfs::VfsFuture<'a, ()> {
        Box::pin(async move {
            let store = self.store.clone();
            let id = id.to_string();
            let _ = tokio::task::spawn_blocking(move || store.delete_transfer_job(&id)).await;
            Ok(())
        })
    }
}

pub fn transfer_manager(
    store: Arc<ft_store::Store>,
    sessions: ft_core::SessionRegistry,
    sftp: Arc<ft_vfs::SftpRegistry>,
) -> ft_transfer::TransferManager {
    ft_transfer::TransferManager::with_hooks(
        ft_transfer::DEFAULT_CONCURRENCY,
        Some(Arc::new(SftpResolver {
            sessions: Arc::new(sessions),
            sftp,
        })),
        Some(Arc::new(StoreJobLog { store })),
    )
}

/// The queue a previous run left behind. Finished rows are pruned as they are
/// read, so the table only ever holds work that still needs doing.
pub fn restored_transfers(store: &ft_store::Store) -> Vec<ft_transfer::TransferJob> {
    let mut jobs = Vec::new();
    for payload in store.list_transfer_jobs().unwrap_or_default() {
        match serde_json::from_str::<ft_transfer::TransferJob>(&payload) {
            Ok(job) if !job.status.is_finished() => jobs.push(job),
            Ok(job) => {
                let _ = store.delete_transfer_job(&job.id);
            }
            // A row we cannot read is a row we cannot act on either.
            Err(_) => {}
        }
    }
    jobs
}

#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RemoteMountConfig {
    #[serde(rename_all = "camelCase")]
    Webdav {
        url: String,
        #[serde(default)]
        username: String,
        #[serde(default)]
        password: String,
    },
    #[serde(rename = "s3", rename_all = "camelCase")]
    S3 {
        endpoint: String,
        #[serde(default)]
        region: String,
        bucket: String,
        access_key: String,
        secret_key: String,
        #[serde(default)]
        prefix: String,
    },
}

#[tauri::command]
pub fn remote_mount_add(
    id: String,
    label: String,
    config: RemoteMountConfig,
) -> Result<(), String> {
    let (kind, vfs): (String, Arc<dyn ft_vfs::Vfs>) = match config {
        RemoteMountConfig::Webdav {
            url,
            username,
            password,
        } => (
            "webdav".to_string(),
            Arc::new(ft_vfs::WebdavVfs::new(&url, &username, &password)?),
        ),
        RemoteMountConfig::S3 {
            endpoint,
            region,
            bucket,
            access_key,
            secret_key,
            prefix,
        } => (
            "s3".to_string(),
            Arc::new(ft_vfs::S3Vfs::new(
                &endpoint,
                &region,
                &bucket,
                &access_key,
                &secret_key,
                &prefix,
            )?),
        ),
    };
    remote_mounts().add(
        ft_vfs::RemoteMountInfo {
            id: id.clone(),
            label: if label.trim().is_empty() { id } else { label },
            kind,
        },
        vfs,
    )
}

#[tauri::command]
pub fn remote_mount_remove(id: String) -> bool {
    remote_mounts().remove(&id)
}

#[tauri::command]
pub fn remote_mount_list() -> Vec<ft_vfs::RemoteMountInfo> {
    remote_mounts().list()
}

#[tauri::command]
pub async fn vfs_list(
    sessions: State<'_, SessionRegistry>,
    runtime: State<'_, SshRuntimeState>,
    session_id: Option<String>,
    path: String,
    show_hidden: Option<bool>,
) -> Result<Vec<ft_vfs::FileEntry>, String> {
    let vfs = resolve_vfs(&sessions, &runtime, session_id.as_deref()).await?;
    vfs.list(&path, show_hidden.unwrap_or(false)).await
}

#[tauri::command]
pub async fn vfs_stat(
    sessions: State<'_, SessionRegistry>,
    runtime: State<'_, SshRuntimeState>,
    session_id: Option<String>,
    path: String,
) -> Result<ft_vfs::FileEntry, String> {
    let vfs = resolve_vfs(&sessions, &runtime, session_id.as_deref()).await?;
    vfs.stat(&path).await
}

#[tauri::command]
pub async fn vfs_home(
    sessions: State<'_, SessionRegistry>,
    runtime: State<'_, SshRuntimeState>,
    session_id: Option<String>,
) -> Result<String, String> {
    let vfs = resolve_vfs(&sessions, &runtime, session_id.as_deref()).await?;
    vfs.home().await
}

#[tauri::command]
pub async fn vfs_mkdir(
    sessions: State<'_, SessionRegistry>,
    runtime: State<'_, SshRuntimeState>,
    session_id: Option<String>,
    path: String,
) -> Result<(), String> {
    let vfs = resolve_vfs(&sessions, &runtime, session_id.as_deref()).await?;
    vfs.mkdir(&path).await
}

#[tauri::command]
pub async fn vfs_remove(
    sessions: State<'_, SessionRegistry>,
    runtime: State<'_, SshRuntimeState>,
    session_id: Option<String>,
    path: String,
    recursive: bool,
) -> Result<(), String> {
    let vfs = resolve_vfs(&sessions, &runtime, session_id.as_deref()).await?;
    if recursive {
        vfs.remove_dir_all(&path).await
    } else {
        vfs.remove_file(&path).await
    }
}

#[tauri::command]
pub async fn vfs_rename(
    sessions: State<'_, SessionRegistry>,
    runtime: State<'_, SshRuntimeState>,
    session_id: Option<String>,
    from: String,
    to: String,
) -> Result<(), String> {
    let vfs = resolve_vfs(&sessions, &runtime, session_id.as_deref()).await?;
    vfs.rename(&from, &to).await
}

#[tauri::command]
pub async fn vfs_chmod(
    sessions: State<'_, SessionRegistry>,
    runtime: State<'_, SshRuntimeState>,
    session_id: Option<String>,
    path: String,
    mode: u32,
) -> Result<(), String> {
    let vfs = resolve_vfs(&sessions, &runtime, session_id.as_deref()).await?;
    vfs.chmod(&path, mode).await
}

#[tauri::command]
pub async fn vfs_read_text(
    sessions: State<'_, SessionRegistry>,
    runtime: State<'_, SshRuntimeState>,
    session_id: Option<String>,
    path: String,
    max_bytes: Option<u64>,
) -> Result<ft_vfs::TextFile, String> {
    let vfs = resolve_vfs(&sessions, &runtime, session_id.as_deref()).await?;
    ft_vfs::read_text(vfs.as_ref(), &path, max_bytes.unwrap_or(2 * 1024 * 1024)).await
}

#[tauri::command]
pub async fn vfs_write_text(
    sessions: State<'_, SessionRegistry>,
    runtime: State<'_, SshRuntimeState>,
    session_id: Option<String>,
    path: String,
    content: String,
    expected_mtime: Option<u64>,
) -> Result<u64, String> {
    let vfs = resolve_vfs(&sessions, &runtime, session_id.as_deref()).await?;
    ft_vfs::write_text(vfs.as_ref(), &path, &content, expected_mtime).await
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferRequest {
    pub id: String,
    #[serde(default)]
    pub source_session: Option<String>,
    pub source_path: String,
    #[serde(default)]
    pub target_session: Option<String>,
    pub target_path: String,
    #[serde(default)]
    pub directory: bool,
}

fn is_remote(session: &Option<String>) -> bool {
    session.as_deref().is_some_and(|id| !id.is_empty())
}

#[tauri::command]
pub async fn transfer_enqueue(
    sessions: State<'_, SessionRegistry>,
    runtime: State<'_, SshRuntimeState>,
    request: TransferRequest,
) -> Result<String, String> {
    let source_remote = is_remote(&request.source_session);
    let target_remote = is_remote(&request.target_session);
    let mut source = ft_transfer::Endpoint::new(
        resolve_vfs(&sessions, &runtime, request.source_session.as_deref()).await?,
        request.source_path,
    )
    .with_session(request.source_session.clone());
    if source_remote {
        source = source.as_remote();
    }
    let mut target = ft_transfer::Endpoint::new(
        resolve_vfs(&sessions, &runtime, request.target_session.as_deref()).await?,
        request.target_path,
    )
    .with_session(request.target_session.clone());
    if target_remote {
        target = target.as_remote();
    }
    if request.directory {
        runtime
            .transfers
            .enqueue_tree(request.id, source, target)
            .await
    } else {
        let size = source.bound()?.stat(&source.path).await?.size;
        Ok(runtime
            .transfers
            .enqueue_file(request.id, source, target, size)
            .await)
    }
}

#[tauri::command]
pub async fn transfer_set_concurrency(
    runtime: State<'_, SshRuntimeState>,
    concurrency: usize,
) -> Result<usize, String> {
    runtime.transfers.set_concurrency(concurrency);
    Ok(runtime.transfers.concurrency())
}

#[tauri::command]
pub async fn transfer_list(
    runtime: State<'_, SshRuntimeState>,
) -> Result<Vec<ft_transfer::TransferJob>, String> {
    Ok(runtime.transfers.list().await)
}

#[tauri::command]
pub async fn transfer_pause(runtime: State<'_, SshRuntimeState>, id: String) -> Result<(), String> {
    runtime.transfers.pause(&id).await;
    Ok(())
}

#[tauri::command]
pub async fn transfer_resume(
    runtime: State<'_, SshRuntimeState>,
    id: String,
) -> Result<(), String> {
    runtime.transfers.resume(&id).await;
    Ok(())
}

#[tauri::command]
pub async fn transfer_cancel(
    runtime: State<'_, SshRuntimeState>,
    id: String,
) -> Result<(), String> {
    runtime.transfers.cancel(&id).await;
    Ok(())
}

#[tauri::command]
pub async fn transfer_retry(runtime: State<'_, SshRuntimeState>, id: String) -> Result<(), String> {
    runtime.transfers.retry(&id).await;
    Ok(())
}

#[tauri::command]
pub async fn transfer_remove(
    runtime: State<'_, SshRuntimeState>,
    id: String,
) -> Result<(), String> {
    runtime.transfers.remove(&id).await;
    Ok(())
}

#[tauri::command]
pub async fn transfer_clear_finished(runtime: State<'_, SshRuntimeState>) -> Result<(), String> {
    runtime.transfers.clear_finished().await;
    Ok(())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn sync_preview(
    sessions: State<'_, SessionRegistry>,
    runtime: State<'_, SshRuntimeState>,
    source_session: Option<String>,
    source_path: String,
    target_session: Option<String>,
    target_path: String,
    two_way: bool,
    delete_extra: bool,
) -> Result<ft_transfer::SyncPlan, String> {
    let source = resolve_vfs(&sessions, &runtime, source_session.as_deref()).await?;
    let target = resolve_vfs(&sessions, &runtime, target_session.as_deref()).await?;
    if two_way {
        return ft_transfer::preview_sync_two_way(
            source.as_ref(),
            &source_path,
            target.as_ref(),
            &target_path,
        )
        .await;
    }
    ft_transfer::preview_sync(
        source.as_ref(),
        &source_path,
        target.as_ref(),
        &target_path,
        delete_extra,
    )
    .await
}

#[tauri::command]
pub async fn sync_apply(
    sessions: State<'_, SessionRegistry>,
    runtime: State<'_, SshRuntimeState>,
    source_session: Option<String>,
    target_session: Option<String>,
    plan: ft_transfer::SyncPlan,
) -> Result<ft_transfer::SyncPlan, String> {
    let source = ft_transfer::Endpoint::new(
        resolve_vfs(&sessions, &runtime, source_session.as_deref()).await?,
        plan.source_root.clone(),
    )
    .with_remote(source_session.as_deref().is_some_and(|id| !id.is_empty()))
    .with_session(source_session.clone());
    let target = ft_transfer::Endpoint::new(
        resolve_vfs(&sessions, &runtime, target_session.as_deref()).await?,
        plan.target_root.clone(),
    )
    .with_remote(target_session.as_deref().is_some_and(|id| !id.is_empty()))
    .with_session(target_session.clone());
    let prefix = format!(
        "sync-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    ft_transfer::apply_sync(&runtime.transfers, &source, &target, &plan, &prefix).await?;
    Ok(plan)
}

#[cfg(test)]
mod key_bundle_tests {
    use super::*;

    #[test]
    fn expiry_accepts_only_plain_iso_dates() {
        assert_eq!(normalize_expiry(" 2027-01-31 "), "2027-01-31");
        assert_eq!(normalize_expiry("2028-02-29"), "2028-02-29");
        assert_eq!(normalize_expiry(""), "");
        assert_eq!(normalize_expiry("   "), "");
        assert_eq!(normalize_expiry("2027-1-31"), "");
        assert_eq!(normalize_expiry("31/01/2027"), "");
        assert_eq!(normalize_expiry("2027-01-31T00:00:00Z"), "");
        assert_eq!(normalize_expiry("2027-13-01"), "");
        assert_eq!(normalize_expiry("2027-00-10"), "");
        assert_eq!(normalize_expiry("2027-01-32"), "");
        assert_eq!(normalize_expiry("abcd-ef-gh"), "");
    }
}

#[cfg(test)]
mod market_tests {
    use super::*;

    /// Hits the published marketplace so a broken index or moved manifest is
    /// caught before release. Not part of the normal suite because it needs
    /// network access.
    #[tokio::test]
    #[ignore = "requires network access to the live GitHub marketplace"]
    async fn installs_the_serial_plugin_from_the_live_marketplace() {
        let raw = fetch_market_text(ft_plugin::DEFAULT_MARKET_URL, MARKET_INDEX_CAP)
            .await
            .unwrap();
        let index = ft_plugin::parse_market_index(&raw).unwrap();
        let entry = index
            .plugins
            .iter()
            .find(|entry| entry.kind == "serial")
            .expect("marketplace lists the serial plugin");
        let manifest = fetch_market_text(&entry.manifest_url, MARKET_MANIFEST_CAP)
            .await
            .unwrap();

        let dir = std::env::temp_dir().join(format!(
            "ferriterm-market-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let registry = ft_plugin::PluginRegistry::new(&dir);
        let installed = registry.install(&entry.id, &manifest).unwrap();
        assert_eq!(installed.kind, "serial");
        assert!(registry.scan().plugins.iter().any(|p| p.id == entry.id));
        assert!(registry.remove(&entry.id).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
