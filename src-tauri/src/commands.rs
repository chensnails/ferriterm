use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use ft_core::{events, RecorderRegistry, SessionRegistry, TerminalSink};
use ft_ssh::{
    ConnectContext, HostKeyChallenge, HostKeyDecision, HostKeyPrompt, KeyboardInteractiveChallenge,
    KeyboardInteractivePrompt, KnownHostsStore, TofuVerifier,
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
    let ctx = ConnectContext::new(verifier)
        .with_keyboard_interactive(keyboard_prompt)
        .with_remote_forwards(router);
    let host_id = args.host_id.clone();
    ft_ssh::connect(sink, &state, id.clone(), args, ctx).await?;
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
    id: String,
) -> Result<(), String> {
    runtime.forwards.stop_session(&id).await;
    runtime.sftp.invalidate(&id).await;
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
pub async fn pty_spawn(
    app: AppHandle,
    state: State<'_, SessionRegistry>,
    recorders: State<'_, RecorderRegistry>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let sink = Arc::new(TauriSink {
        app,
        recorders: recorders.inner().clone(),
    });
    ft_pty::spawn(sink, &state, id, cols, rows).await
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

#[derive(serde::Deserialize)]
struct BackupPayload {
    #[serde(default)]
    tags: Vec<ft_store::Tag>,
    #[serde(default)]
    hosts: Vec<ft_store::Host>,
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

#[tauri::command]
pub fn export_backup(
    backups: State<'_, ft_sync::BackupStore>,
    store: State<'_, Arc<ft_store::Store>>,
    passphrase: String,
) -> Result<String, String> {
    let payload = backup_payload(&store)?;
    backups
        .export(&passphrase, &payload)
        .map(|path| path.display().to_string())
        .map_err(|e| e.to_string())
}

fn backup_payload(store: &ft_store::Store) -> Result<Vec<u8>, String> {
    let hosts = store.list_hosts().map_err(|e| e.to_string())?;
    let tags = store.list_tags().map_err(|e| e.to_string())?;
    let exported_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    serde_json::to_vec(&serde_json::json!({
        "version": 1,
        "exportedAt": exported_at,
        "tags": tags,
        "hosts": hosts,
    }))
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn import_backup(
    store: State<'_, Arc<ft_store::Store>>,
    payload: String,
    passphrase: String,
) -> Result<ft_store::ImportStats, String> {
    let plain = ft_sync::open(&passphrase, &payload).map_err(|e| e.to_string())?;
    let parsed: BackupPayload =
        serde_json::from_slice(&plain).map_err(|e| format!("invalid backup payload: {e}"))?;
    store
        .import_snapshot(&parsed.hosts, &parsed.tags)
        .map_err(|e| e.to_string())
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
pub async fn sync_remote_test(config: RemoteConfig) -> Result<String, String> {
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
    config: RemoteConfig,
    passphrase: String,
) -> Result<ft_sync::RemoteUpload, String> {
    let payload = backup_payload(&store)?;
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
    config: RemoteConfig,
    passphrase: String,
) -> Result<ft_store::ImportStats, String> {
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
    store
        .import_snapshot(&parsed.hosts, &parsed.tags)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_credential(
    state: State<'_, ft_security::CredentialStore>,
    host_id: String,
    secret: String,
) -> Result<(), String> {
    state.set(&host_id, &secret).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_credential(
    state: State<'_, ft_security::CredentialStore>,
    host_id: String,
) -> Result<Option<String>, String> {
    state.get(&host_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_credential(
    state: State<'_, ft_security::CredentialStore>,
    host_id: String,
) -> Result<(), String> {
    state.delete(&host_id).map_err(|e| e.to_string())
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
pub fn delete_host(state: State<'_, Arc<ft_store::Store>>, id: String) -> Result<(), String> {
    state.delete_host(&id).map_err(|e| e.to_string())
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

#[tauri::command]
pub async fn transfer_enqueue(
    sessions: State<'_, SessionRegistry>,
    runtime: State<'_, SshRuntimeState>,
    request: TransferRequest,
) -> Result<String, String> {
    let source = ft_transfer::Endpoint::new(
        resolve_vfs(&sessions, &runtime, request.source_session.as_deref()).await?,
        request.source_path,
    );
    let target = ft_transfer::Endpoint::new(
        resolve_vfs(&sessions, &runtime, request.target_session.as_deref()).await?,
        request.target_path,
    );
    if request.directory {
        runtime
            .transfers
            .enqueue_tree(request.id, source, target)
            .await
    } else {
        let size = source.vfs.stat(&source.path).await?.size;
        Ok(runtime
            .transfers
            .enqueue_file(request.id, source, target, size)
            .await)
    }
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
pub async fn sync_preview(
    sessions: State<'_, SessionRegistry>,
    runtime: State<'_, SshRuntimeState>,
    source_session: Option<String>,
    source_path: String,
    target_session: Option<String>,
    target_path: String,
    delete_extra: bool,
) -> Result<ft_transfer::SyncPlan, String> {
    let source = resolve_vfs(&sessions, &runtime, source_session.as_deref()).await?;
    let target = resolve_vfs(&sessions, &runtime, target_session.as_deref()).await?;
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
    source_path: String,
    target_session: Option<String>,
    target_path: String,
    delete_extra: bool,
) -> Result<ft_transfer::SyncPlan, String> {
    let source = ft_transfer::Endpoint::new(
        resolve_vfs(&sessions, &runtime, source_session.as_deref()).await?,
        source_path,
    );
    let target = ft_transfer::Endpoint::new(
        resolve_vfs(&sessions, &runtime, target_session.as_deref()).await?,
        target_path,
    );
    let plan = ft_transfer::preview_sync(
        source.vfs.as_ref(),
        &source.path,
        target.vfs.as_ref(),
        &target.path,
        delete_extra,
    )
    .await?;
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
