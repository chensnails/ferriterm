use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use super::vault::VaultProvider;
use ft_core::{events, RecorderRegistry, SessionRegistry, TerminalSink};
use ft_ssh::{
    ConnectContext, HostKeyChallenge, HostKeyDecision, HostKeyPrompt, KeyboardInteractiveChallenge,
    KeyboardInteractivePrompt, KnownHostsStore, TofuVerifier,
};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::oneshot;

pub(crate) struct TauriSink {
    pub(crate) app: AppHandle,
    pub(crate) recorders: RecorderRegistry,
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

/// What a live session is connected to.
///
/// The interface names sessions with its own tab titles, which the backend
/// never sees; the agent service needs that name back, because a rule scoped to
/// "web-1" and an audit record reading "web-1" have to mean the same host as
/// the label a person reads in the window.
#[derive(Debug, Clone)]
pub struct SessionLabel {
    pub label: String,
    pub host: String,
    pub port: u16,
    pub username: String,
}

#[derive(Default)]
pub struct SessionLabels {
    inner: StdMutex<HashMap<String, SessionLabel>>,
}

impl SessionLabels {
    pub fn remember(&self, session: &str, label: SessionLabel) {
        if let Ok(mut map) = self.inner.lock() {
            map.insert(session.to_string(), label);
        }
    }

    pub fn forget(&self, session: &str) {
        if let Ok(mut map) = self.inner.lock() {
            map.remove(session);
        }
    }

    pub fn get(&self, session: &str) -> Option<SessionLabel> {
        self.inner
            .lock()
            .ok()
            .and_then(|map| map.get(session).cloned())
    }

    /// Every live session, ordered by label so repeated calls read the same.
    pub fn list(&self) -> Vec<(String, SessionLabel)> {
        let mut all: Vec<(String, SessionLabel)> = self
            .inner
            .lock()
            .map(|map| {
                map.iter()
                    .map(|(id, label)| (id.clone(), label.clone()))
                    .collect()
            })
            .unwrap_or_default();
        all.sort_by(|left, right| left.1.label.cmp(&right.1.label));
        all
    }
}

#[derive(Clone)]
pub struct SshRuntimeState {
    host_keys: HostKeyState,
    keyboard: KeyboardInteractiveState,
    pub(crate) known_hosts: Arc<KnownHostsStore>,
    forwards: Arc<ft_ssh::ForwardRegistry>,
    store: Arc<ft_store::Store>,
    pub(crate) sftp: Arc<ft_vfs::SftpRegistry>,
    pub(crate) transfers: ft_transfer::TransferManager,
    remote: Arc<ft_monitor::RemoteMonitor>,
    labels: Arc<SessionLabels>,
}

impl SshRuntimeState {
    pub fn new(
        known_hosts: Arc<KnownHostsStore>,
        forwards: Arc<ft_ssh::ForwardRegistry>,
        store: Arc<ft_store::Store>,
        sftp: Arc<ft_vfs::SftpRegistry>,
        transfers: ft_transfer::TransferManager,
        remote: Arc<ft_monitor::RemoteMonitor>,
        labels: Arc<SessionLabels>,
    ) -> Self {
        Self {
            host_keys: HostKeyState::default(),
            keyboard: KeyboardInteractiveState::default(),
            known_hosts,
            forwards,
            store,
            sftp,
            transfers,
            remote,
            labels,
        }
    }

    /// The map from session id to the host a person sees, shared with the
    /// agent service so both name a session the same way.
    pub fn labels(&self) -> Arc<SessionLabels> {
        self.labels.clone()
    }

    /// Reclaims everything a session was holding when it went away, whether it
    /// died on its own or was closed by the user. Safe to call twice.
    pub async fn reap(&self, session_id: &str) {
        tracing::info!(session = %session_id, "reaping a closed session");
        self.forwards.stop_session(session_id).await;
        self.sftp.invalidate(session_id).await;
        self.remote.forget(session_id);
        self.labels.forget(session_id);
        self.transfers
            .pause_session(session_id, "the session closed; resume after reconnecting")
            .await;
    }
}

impl ft_ssh::SessionCleanup for SshRuntimeState {
    fn cleanup<'a>(
        &'a self,
        session_id: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        Box::pin(self.reap(session_id))
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
        store: runtime.store.clone(),
    });
    let ctx = ConnectContext::new(verifier)
        .with_keyboard_interactive(keyboard_prompt)
        .with_remote_forwards(router)
        .with_credentials(provider)
        .with_cleanup(Arc::new(runtime.inner().clone()));
    let host_id = args.host_id.clone();
    // Captured before `args` moves into the connect call: the agent service
    // has to name this session the way the window does, or a rule scoped to a
    // host would never match what an agent asks for.
    let connected = SessionLabel {
        label: String::new(),
        host: args.host.clone(),
        port: args.port,
        username: args.username.clone(),
    };
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
    if result.is_ok() {
        tracing::info!(session = %id, target = %audit_target, "ssh session opened");
    } else {
        tracing::warn!(session = %id, target = %audit_target, %detail, "ssh connect failed");
    }
    result?;
    let label = match &host_id {
        Some(host_id) => runtime
            .store
            .list_hosts()
            .ok()
            .and_then(|hosts| hosts.into_iter().find(|host| &host.id == host_id))
            .map(|host| host.name)
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| format!("{}@{}", connected.username, connected.host)),
        None => format!("{}@{}", connected.username, connected.host),
    };
    runtime
        .labels()
        .remember(&id, SessionLabel { label, ..connected });
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
    runtime.reap(&id).await;
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
