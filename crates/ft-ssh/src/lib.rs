use std::borrow::Cow;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use ft_core::{
    RemoteForwardHandler, ServerKeyVerifier, Session, SessionRegistry, SshHandler, SshSession,
    TerminalSink, X11ChannelHandler,
};
use russh::client::{self, Config};
use russh::keys::{Algorithm, HashAlg, PrivateKey, PublicKey};
use russh::{cipher, compression, kex, mac, ChannelMsg, Disconnect};
use serde::Deserialize;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use tokio::net::TcpStream;

mod forward;
mod keys;
mod known_hosts;

pub use forward::{
    start_forward, ForwardInfo, ForwardKind, ForwardRegistry, ForwardRequest, ForwardRouter,
};
pub use keys::{
    decrypt_for_use, generate_key, inspect_private_key, read_key_file, scan_key_candidates,
    KeyCandidate, KeyInfo,
};
pub use known_hosts::{HostKeyEntry, HostKeyStatus, KnownHostsStore};

#[derive(Debug, Clone)]
pub struct HostKeyChallenge {
    pub host: String,
    pub port: u16,
    pub fingerprint: String,
    pub algorithm: String,
    pub changed: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct HostKeyDecision {
    pub accept: bool,
    pub remember: bool,
}

pub trait HostKeyPrompt: Send + Sync {
    fn prompt(
        &self,
        challenge: HostKeyChallenge,
        respond: tokio::sync::oneshot::Sender<HostKeyDecision>,
    );
}

#[derive(Debug, Clone)]
pub struct KeyboardPrompt {
    pub text: String,
    pub echo: bool,
}

#[derive(Debug, Clone)]
pub struct KeyboardInteractiveChallenge {
    pub name: String,
    pub instructions: String,
    pub prompts: Vec<KeyboardPrompt>,
}

pub trait KeyboardInteractivePrompt: Send + Sync {
    fn prompt(
        &self,
        challenge: KeyboardInteractiveChallenge,
        respond: tokio::sync::oneshot::Sender<Option<Vec<String>>>,
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKind {
    Password,
    Passphrase,
}

/// Resolves stored secrets inside the Rust process so plaintext never crosses
/// into the webview. Implementations read from the credential vault.
pub trait CredentialProvider: Send + Sync {
    fn host_secret(&self, host_id: &str, kind: SecretKind) -> Option<String>;
    fn key_material(&self, key_id: &str) -> Option<(String, Option<String>)>;
    fn locked(&self) -> bool {
        false
    }
}

pub struct ConnectContext {
    pub server_key: Arc<dyn ServerKeyVerifier>,
    pub keyboard_interactive: Option<Arc<dyn KeyboardInteractivePrompt>>,
    pub remote_forwards: Option<Arc<dyn RemoteForwardHandler>>,
    pub credentials: Option<Arc<dyn CredentialProvider>>,
    pub cleanup: Option<Arc<dyn SessionCleanup>>,
}

/// Tears down what the shell attached to a session after that session's output
/// loop ends. The shell implements this because it owns the port forwards, the
/// SFTP channel, the remote monitor and the transfer queue; `ft-ssh` cannot
/// reach any of them without depending on those crates.
pub trait SessionCleanup: Send + Sync {
    fn cleanup<'a>(&'a self, session_id: &'a str) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
}

impl ConnectContext {
    pub fn new(server_key: Arc<dyn ServerKeyVerifier>) -> Self {
        Self {
            server_key,
            keyboard_interactive: None,
            remote_forwards: None,
            credentials: None,
            cleanup: None,
        }
    }

    pub fn with_keyboard_interactive(mut self, prompt: Arc<dyn KeyboardInteractivePrompt>) -> Self {
        self.keyboard_interactive = Some(prompt);
        self
    }

    pub fn with_remote_forwards(mut self, handler: Arc<dyn RemoteForwardHandler>) -> Self {
        self.remote_forwards = Some(handler);
        self
    }

    pub fn with_credentials(mut self, provider: Arc<dyn CredentialProvider>) -> Self {
        self.credentials = Some(provider);
        self
    }

    pub fn with_cleanup(mut self, cleanup: Arc<dyn SessionCleanup>) -> Self {
        self.cleanup = Some(cleanup);
        self
    }
}

pub struct TofuVerifier {
    store: Arc<KnownHostsStore>,
    prompt: Option<Arc<dyn HostKeyPrompt>>,
}

impl TofuVerifier {
    pub fn new(store: Arc<KnownHostsStore>, prompt: Option<Arc<dyn HostKeyPrompt>>) -> Self {
        Self { store, prompt }
    }
}

impl ServerKeyVerifier for TofuVerifier {
    fn verify<'a>(
        &'a self,
        host: &'a str,
        port: u16,
        key: &'a PublicKey,
    ) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move {
            let status = self.store.check(host, port, key);
            if status == HostKeyStatus::Trusted {
                return true;
            }
            let Some(prompt) = &self.prompt else {
                return false;
            };
            let (respond, decision) = tokio::sync::oneshot::channel();
            prompt.prompt(
                HostKeyChallenge {
                    host: host.to_string(),
                    port,
                    fingerprint: key.fingerprint(HashAlg::Sha256).to_string(),
                    algorithm: key.algorithm().as_str().to_string(),
                    changed: status == HostKeyStatus::Mismatch,
                },
                respond,
            );
            let decision = decision.await.unwrap_or_default();
            if decision.accept && decision.remember {
                let _ = self.store.remember(host, port, key);
            }
            decision.accept
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JumpHop {
    pub host: String,
    pub port: u16,
    pub username: String,
    #[serde(default)]
    pub host_id: Option<String>,
    #[serde(default)]
    pub key_id: Option<String>,
    #[serde(default, alias = "privateKey")]
    pub key_path: Option<String>,
    #[serde(default)]
    pub auth_method: String,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub key_passphrase: Option<String>,
    #[serde(default)]
    pub agent: bool,
    #[serde(default)]
    pub keyboard_interactive: bool,
}

/// User-selected SSH algorithm preference lists. An empty group keeps the
/// russh default for that group, so a partially filled payload can never
/// produce a handshake with no usable algorithm.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshAlgorithms {
    #[serde(default)]
    pub kex: Vec<String>,
    #[serde(default)]
    pub cipher: Vec<String>,
    #[serde(default)]
    pub mac: Vec<String>,
    #[serde(default)]
    pub host_key: Vec<String>,
    #[serde(default)]
    pub compression: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshConnectArgs {
    pub host: String,
    pub port: u16,
    pub username: String,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default, alias = "privateKey")]
    pub key_path: Option<String>,
    #[serde(default)]
    pub key_passphrase: Option<String>,
    #[serde(default)]
    pub key_id: Option<String>,
    #[serde(default)]
    pub auth_method: String,
    #[serde(default)]
    pub keyboard_interactive: bool,
    #[serde(default)]
    pub agent: bool,
    #[serde(default)]
    pub x11: bool,
    #[serde(default)]
    pub jump: Vec<JumpHop>,
    #[serde(default)]
    pub keepalive: Option<u64>,
    #[serde(default)]
    pub host_id: Option<String>,
    #[serde(default)]
    pub algorithms: Option<SshAlgorithms>,
    pub cols: u32,
    pub rows: u32,
}

trait DynStream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> DynStream for T {}

pub async fn connect(
    sink: Arc<dyn TerminalSink>,
    registry: &SessionRegistry,
    id: String,
    args: SshConnectArgs,
    ctx: ConnectContext,
) -> Result<(), String> {
    let x11 = if args.x11 {
        Some(x11_target().ok_or_else(|| {
            "X11 forwarding requires a local DISPLAY (X server not found)".to_string()
        })?)
    } else {
        None
    };
    let config = Arc::new(client_config(&args));

    let mut route: Vec<JumpHop> = args.jump.clone();
    route.push(JumpHop {
        host: args.host.clone(),
        port: args.port,
        username: args.username.clone(),
        host_id: args.host_id.clone(),
        key_id: args.key_id.clone(),
        key_path: args.key_path.clone(),
        auth_method: args.auth_method.clone(),
        password: args.password.clone(),
        key_passphrase: args.key_passphrase.clone(),
        agent: args.agent,
        keyboard_interactive: args.keyboard_interactive,
    });

    let last_index = route.len() - 1;
    let mut stream: Option<Box<dyn DynStream>> = None;
    let mut handle: Option<Arc<client::Handle<SshHandler>>> = None;

    for (index, hop) in route.iter().enumerate() {
        let is_last = index == last_index;
        let mut handler = SshHandler::new(&hop.host, hop.port, ctx.server_key.clone());
        if is_last {
            if let Some(router) = &ctx.remote_forwards {
                handler = handler.with_remote_forwards(router.clone());
            }
            if let Some(target) = &x11 {
                handler = handler.with_x11(Arc::new(X11Router {
                    target: target.clone(),
                }));
            }
        }
        let mut connected = match stream.take() {
            Some(stream) => tokio::time::timeout(
                Duration::from_secs(15),
                client::connect_stream(config.clone(), stream, handler),
            )
            .await
            .map_err(|_| format!("connection to {} timed out (15s)", hop.host))?,
            None => tokio::time::timeout(
                Duration::from_secs(15),
                client::connect(config.clone(), (hop.host.as_str(), hop.port), handler),
            )
            .await
            .map_err(|_| format!("connection to {} timed out (15s)", hop.host))?,
        }
        .map_err(|e| {
            if matches!(e, russh::Error::UnknownKey) {
                format!(
                    "host key for {} was not accepted; connection aborted",
                    hop.host
                )
            } else {
                e.to_string()
            }
        })?;

        authenticate_hop(&mut connected, hop, &ctx).await?;

        if is_last {
            handle = Some(Arc::new(connected));
        } else {
            let next = &route[index + 1];
            let channel = connected
                .channel_open_direct_tcpip(next.host.clone(), u32::from(next.port), "127.0.0.1", 0)
                .await
                .map_err(|e| format!("failed to open tunnel to {}: {e}", next.host))?;
            stream = Some(Box::new(channel.into_stream()));
        }
    }

    let handle = handle.ok_or_else(|| "no SSH connection was established".to_string())?;

    let channel = handle
        .channel_open_session()
        .await
        .map_err(|e| e.to_string())?;
    if args.x11 {
        channel
            .request_x11(true, false, "MIT-MAGIC-COOKIE-1", "", 0)
            .await
            .map_err(|e| format!("failed to request X11 forwarding: {e}"))?;
    }
    channel
        .request_pty(true, "xterm-256color", args.cols, args.rows, 0, 0, &[])
        .await
        .map_err(|e| e.to_string())?;
    channel
        .request_shell(true)
        .await
        .map_err(|e| e.to_string())?;

    let (mut reader, writer) = channel.split();
    let event_id = id.clone();
    registry.sessions.lock().await.insert(
        id.clone(),
        Session::Ssh(SshSession {
            handle: handle.clone(),
            writer,
        }),
    );

    let cleanup = ctx.cleanup.clone();
    let registry = registry.clone();
    tokio::spawn(async move {
        while let Some(msg) = reader.wait().await {
            match msg {
                ChannelMsg::Data { data } => sink.output(&event_id, data.to_vec()),
                ChannelMsg::ExtendedData { data, .. } => sink.output(&event_id, data.to_vec()),
                ChannelMsg::Close => break,
                _ => {}
            }
        }
        // The connection stopped producing output, so this session is finished
        // whether the server hung up, the shell exited, or a keepalive gave up.
        // Reap it here and only then announce the closure: the UI reconnects on
        // the closed event under the same id, and a reconnect must never find
        // its fresh session torn down by the loop it replaced.
        let reaped = {
            let mut sessions = registry.sessions.lock().await;
            match sessions.get(&event_id) {
                Some(Session::Ssh(session)) if Arc::ptr_eq(&session.handle, &handle) => {
                    sessions.remove(&event_id)
                }
                _ => None,
            }
        };
        if let Some(Session::Ssh(dead)) = reaped {
            let _ = dead
                .handle
                .disconnect(Disconnect::ByApplication, "session ended", "")
                .await;
            if let Some(cleanup) = &cleanup {
                cleanup.cleanup(&event_id).await;
            }
            sink.closed(&event_id);
        }
    });
    Ok(())
}

fn client_config(args: &SshConnectArgs) -> Config {
    let mut config = Config::default();
    let keepalive = args.keepalive.unwrap_or(30);
    if keepalive > 0 {
        config.keepalive_interval = Some(Duration::from_secs(keepalive));
        config.keepalive_max = 3;
    }
    if let Some(algorithms) = &args.algorithms {
        apply_algorithms(&mut config, algorithms);
    }
    config
}

/// Maps the user's selected algorithm names onto russh's `Preferred` lists.
///
/// Unknown names are ignored (the frontend validates already, but a stale
/// settings blob must never break a connection). A group is only replaced
/// when at least one name maps, so an empty group keeps the library default.
/// KEX additionally re-appends the four extension pseudo-algorithms that
/// russh's own default KEXINIT advertises — without them `ext-info-c` and
/// OpenSSH strict-kex would silently disappear from the proposal.
fn apply_algorithms(config: &mut Config, algorithms: &SshAlgorithms) {
    let kex: Vec<kex::Name> = algorithms
        .kex
        .iter()
        .filter_map(|name| kex::Name::try_from(name.as_str()).ok())
        .collect();
    if !kex.is_empty() {
        let mut names = kex;
        for extension in [
            kex::EXTENSION_SUPPORT_AS_CLIENT,
            kex::EXTENSION_SUPPORT_AS_SERVER,
            kex::EXTENSION_OPENSSH_STRICT_KEX_AS_CLIENT,
            kex::EXTENSION_OPENSSH_STRICT_KEX_AS_SERVER,
        ] {
            if !names.contains(&extension) {
                names.push(extension);
            }
        }
        config.preferred.kex = Cow::Owned(names);
    }

    let cipher: Vec<cipher::Name> = algorithms
        .cipher
        .iter()
        .filter_map(|name| cipher::Name::try_from(name.as_str()).ok())
        .collect();
    if !cipher.is_empty() {
        config.preferred.cipher = Cow::Owned(cipher);
    }

    let mac: Vec<mac::Name> = algorithms
        .mac
        .iter()
        .filter_map(|name| mac::Name::try_from(name.as_str()).ok())
        .collect();
    if !mac.is_empty() {
        config.preferred.mac = Cow::Owned(mac);
    }

    // Host-key algorithms are an enum, not a Name: use russh's own default
    // list as the allow-list so only variants it can actually verify get
    // advertised, while preserving the user's preference order.
    let defaults = russh::Preferred::DEFAULT;
    let host_key: Vec<Algorithm> = algorithms
        .host_key
        .iter()
        .filter_map(|name| {
            defaults
                .key
                .iter()
                .find(|algo| algo.as_str() == name.as_str())
                .cloned()
        })
        .collect();
    if !host_key.is_empty() {
        config.preferred.key = Cow::Owned(host_key);
    }

    let compression: Vec<compression::Name> = algorithms
        .compression
        .iter()
        .filter_map(|name| compression::Name::try_from(name.as_str()).ok())
        .collect();
    if !compression.is_empty() {
        config.preferred.compression = Cow::Owned(compression);
    }
}

async fn authenticate_hop(
    handle: &mut client::Handle<SshHandler>,
    hop: &JumpHop,
    ctx: &ConnectContext,
) -> Result<(), String> {
    if hop.keyboard_interactive || hop.auth_method.eq_ignore_ascii_case("keyboard-interactive") {
        let authenticated = authenticate_keyboard_interactive(
            handle,
            &hop.username,
            ctx.keyboard_interactive.as_ref(),
        )
        .await?;
        if authenticated {
            return Ok(());
        }
        return Err(auth_failed(hop));
    }

    let methods = auth_methods(hop);
    let mut last_error: Option<String> = None;
    for method in methods {
        let result = match method {
            AuthMethod::Key => authenticate_key(handle, hop, ctx).await,
            AuthMethod::Agent => authenticate_agent(handle, &hop.username).await,
            AuthMethod::Password => authenticate_password(handle, hop, ctx).await,
        };
        match result {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(err) => last_error = Some(err),
        }
    }
    Err(last_error.unwrap_or_else(|| auth_failed(hop)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthMethod {
    Key,
    Agent,
    Password,
}

/// Ordered fallback chain, capped so a misconfigured host cannot loop over
/// every method twice.
fn auth_methods(hop: &JumpHop) -> Vec<AuthMethod> {
    let has_key = hop
        .key_id
        .as_deref()
        .is_some_and(|id| !id.trim().is_empty())
        || hop
            .key_path
            .as_deref()
            .is_some_and(|path| !path.trim().is_empty());
    let methods = match hop.auth_method.to_ascii_lowercase().as_str() {
        "key" => vec![AuthMethod::Key, AuthMethod::Agent, AuthMethod::Password],
        "agent" => vec![AuthMethod::Agent, AuthMethod::Key, AuthMethod::Password],
        "password" => vec![AuthMethod::Password, AuthMethod::Key, AuthMethod::Agent],
        _ if has_key => vec![AuthMethod::Key, AuthMethod::Agent, AuthMethod::Password],
        _ => vec![AuthMethod::Password, AuthMethod::Agent],
    };
    methods.into_iter().take(3).collect()
}

async fn authenticate_key(
    handle: &mut client::Handle<SshHandler>,
    hop: &JumpHop,
    ctx: &ConnectContext,
) -> Result<bool, String> {
    let provider = ctx.credentials.as_ref();
    let (text, passphrase) = if let Some(key_id) = hop
        .key_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        let Some((text, stored)) = provider.and_then(|p| p.key_material(key_id)) else {
            return Err(format!(
                "managed key is unavailable for {}@{} (unlock the vault or import a key)",
                hop.username, hop.host
            ));
        };
        (
            text,
            stored.or_else(|| host_secret(ctx, hop, SecretKind::Passphrase)),
        )
    } else if let Some(path) = hop
        .key_path
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
    {
        let text = keys::read_key_file(path)?;
        let passphrase =
            host_secret(ctx, hop, SecretKind::Passphrase).or_else(|| hop.key_passphrase.clone());
        (text, passphrase)
    } else {
        return Ok(false);
    };
    let key = keys::decrypt_for_use(&text, passphrase.as_deref())?;
    authenticate_public_key(handle, &hop.username, key).await
}

async fn authenticate_public_key(
    handle: &mut client::Handle<SshHandler>,
    username: &str,
    key: PrivateKey,
) -> Result<bool, String> {
    let hash = handle
        .best_supported_rsa_hash()
        .await
        .map_err(|e| e.to_string())?
        .flatten();
    let key = russh::keys::PrivateKeyWithHashAlg::new(Arc::new(key), hash);
    let result = handle
        .authenticate_publickey(username, key)
        .await
        .map_err(|e| e.to_string())?;
    Ok(result.success())
}

async fn authenticate_password(
    handle: &mut client::Handle<SshHandler>,
    hop: &JumpHop,
    ctx: &ConnectContext,
) -> Result<bool, String> {
    let password = hop
        .password
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| host_secret(ctx, hop, SecretKind::Password));
    let Some(password) = password else {
        if ctx.credentials.as_ref().is_some_and(|p| p.locked()) {
            return Err("credential vault is locked; unlock it before connecting".to_string());
        }
        return Err(format!(
            "credentials are required for {}@{}",
            hop.username, hop.host
        ));
    };
    let result = handle
        .authenticate_password(&hop.username, password)
        .await
        .map_err(|e| e.to_string())?;
    Ok(result.success())
}

fn host_secret(ctx: &ConnectContext, hop: &JumpHop, kind: SecretKind) -> Option<String> {
    let host_id = hop
        .host_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())?;
    ctx.credentials.as_ref()?.host_secret(host_id, kind)
}

fn auth_failed(hop: &JumpHop) -> String {
    format!(
        "authentication failed for {}@{}: check credentials",
        hop.username, hop.host
    )
}

#[cfg(any(unix, windows))]
type DynAgent = russh::keys::agent::client::AgentClient<
    Box<dyn russh::keys::agent::client::AgentStream + Send + Unpin>,
>;

#[cfg(unix)]
async fn open_agent() -> Result<DynAgent, String> {
    use russh::keys::agent::client::AgentClient;

    AgentClient::connect_env()
        .await
        .map(|agent| agent.dynamic())
        .map_err(|e| format!("ssh-agent unavailable (SSH_AUTH_SOCK): {e}"))
}

/// The Windows OpenSSH agent listens on a named pipe; Pageant is the common
/// alternative. Trying both keeps the Agent option usable instead of always
/// failing on this platform.
#[cfg(windows)]
async fn open_agent() -> Result<DynAgent, String> {
    use russh::keys::agent::client::AgentClient;

    match AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent").await {
        Ok(agent) => Ok(agent.dynamic()),
        Err(pipe_err) => match AgentClient::connect_pageant().await {
            Ok(agent) => Ok(agent.dynamic()),
            Err(pageant_err) => Err(format!(
                "ssh-agent unavailable (OpenSSH named pipe: {pipe_err}; Pageant: {pageant_err})"
            )),
        },
    }
}

#[cfg(any(unix, windows))]
async fn authenticate_agent(
    handle: &mut client::Handle<SshHandler>,
    username: &str,
) -> Result<bool, String> {
    let mut agent = open_agent().await?;
    if let Some(binding) = handle.agent_session_binding() {
        agent.bind_session(binding, false).await.ok();
    }
    let identities = agent
        .request_identities()
        .await
        .map_err(|e| format!("ssh-agent error: {e}"))?;
    if identities.is_empty() {
        return Ok(false);
    }
    let hash = handle
        .best_supported_rsa_hash()
        .await
        .map_err(|e| e.to_string())?
        .flatten();
    for identity in identities {
        let key = identity.public_key().into_owned();
        let result = handle
            .authenticate_publickey_with(username, key, hash, &mut agent)
            .await;
        if let Ok(result) = result {
            if result.success() {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

#[cfg(not(any(unix, windows)))]
async fn authenticate_agent(
    _handle: &mut client::Handle<SshHandler>,
    _username: &str,
) -> Result<bool, String> {
    Err("ssh-agent authentication is not supported on this platform".to_string())
}

pub async fn write(registry: &SessionRegistry, id: &str, data: String) -> Result<(), String> {
    let sessions = registry.sessions.lock().await;
    match sessions.get(id) {
        Some(Session::Ssh(s)) => s
            .writer
            .data_bytes(data.into_bytes())
            .await
            .map_err(|e| e.to_string()),
        _ => Err("ssh session not found".to_string()),
    }
}

pub async fn resize(
    registry: &SessionRegistry,
    id: &str,
    cols: u32,
    rows: u32,
) -> Result<(), String> {
    let sessions = registry.sessions.lock().await;
    match sessions.get(id) {
        Some(Session::Ssh(s)) => s
            .writer
            .window_change(cols, rows, 0, 0)
            .await
            .map_err(|e| e.to_string()),
        _ => Err("ssh session not found".to_string()),
    }
}

pub async fn disconnect(registry: &SessionRegistry, id: &str) -> Result<(), String> {
    let session = registry.sessions.lock().await.remove(id);
    if let Some(Session::Ssh(s)) = session {
        let _ = s
            .handle
            .disconnect(Disconnect::ByApplication, "closed by user", "")
            .await;
    }
    Ok(())
}

/// Runs one fixed command over an existing session and returns stdout. The
/// caller supplies the timeout and output cap; nothing from user input is
/// interpolated into the command. Used by the remote monitor.
pub async fn exec(
    registry: &SessionRegistry,
    id: &str,
    command: &str,
    timeout: Duration,
    cap: usize,
) -> Result<String, String> {
    let handle = {
        let sessions = registry.sessions.lock().await;
        match sessions.get(id) {
            Some(Session::Ssh(s)) => s.handle.clone(),
            _ => return Err("ssh session not found".to_string()),
        }
    };
    let channel = tokio::time::timeout(timeout, handle.channel_open_session())
        .await
        .map_err(|_| "remote command timed out".to_string())?
        .map_err(|e| format!("failed to open exec channel: {e}"))?;
    channel
        .exec(true, command.to_owned())
        .await
        .map_err(|e| format!("failed to run remote command: {e}"))?;
    let mut stream = channel.into_stream();
    let deadline = tokio::time::Instant::now() + timeout;
    let mut out = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        match tokio::time::timeout_at(deadline, stream.read(&mut buf)).await {
            Err(_) => return Err("remote command timed out".to_string()),
            Ok(Err(error)) => return Err(format!("remote command failed: {error}")),
            Ok(Ok(0)) => break,
            Ok(Ok(read)) => {
                let room = cap.saturating_sub(out.len());
                out.extend_from_slice(&buf[..read.min(room)]);
                if out.len() >= cap {
                    break;
                }
            }
        }
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

async fn authenticate_keyboard_interactive(
    handle: &mut client::Handle<SshHandler>,
    username: &str,
    prompt: Option<&Arc<dyn KeyboardInteractivePrompt>>,
) -> Result<bool, String> {
    use russh::client::KeyboardInteractiveAuthResponse as Response;

    let mut response = handle
        .authenticate_keyboard_interactive_start(username, None::<String>)
        .await
        .map_err(|e| e.to_string())?;
    loop {
        match response {
            Response::Success => return Ok(true),
            Response::Failure { .. } => return Ok(false),
            Response::InfoRequest {
                name,
                instructions,
                prompts,
            } => {
                let Some(prompt) = prompt else {
                    return Err("server requires keyboard-interactive authentication".to_string());
                };
                let expected = prompts.len();
                let challenge = KeyboardInteractiveChallenge {
                    name,
                    instructions,
                    prompts: prompts
                        .into_iter()
                        .map(|prompt| KeyboardPrompt {
                            text: prompt.prompt,
                            echo: prompt.echo,
                        })
                        .collect(),
                };
                let (respond, answers) = tokio::sync::oneshot::channel();
                prompt.prompt(challenge, respond);
                let Some(answers) = answers.await.ok().flatten() else {
                    return Ok(false);
                };
                if answers.len() != expected {
                    return Err("keyboard-interactive response count mismatch".to_string());
                }
                response = handle
                    .authenticate_keyboard_interactive_respond(answers)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
    }
}

#[derive(Debug, Clone)]
enum X11Target {
    Unix(PathBuf),
    Tcp(String, u16),
}

fn x11_target() -> Option<X11Target> {
    let display = std::env::var("DISPLAY").ok()?;
    let display = display.trim();
    if display.is_empty() {
        return None;
    }
    let (host, rest) = display.rsplit_once(':')?;
    let screen = rest
        .split('.')
        .next()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(0);
    if host.is_empty() || host == "unix" {
        Some(X11Target::Unix(PathBuf::from(format!(
            "/tmp/.X11-unix/X{screen}"
        ))))
    } else {
        Some(X11Target::Tcp(host.to_string(), 6000 + screen))
    }
}

struct X11Router {
    target: X11Target,
}

impl X11ChannelHandler for X11Router {
    fn x11_channel<'a>(
        &'a self,
        channel: russh::Channel<client::Msg>,
    ) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move {
            let mut remote = channel.into_stream();
            match &self.target {
                X11Target::Tcp(host, port) => {
                    let Ok(mut local) = TcpStream::connect((host.as_str(), *port)).await else {
                        return false;
                    };
                    tokio::spawn(async move {
                        let _ = tokio::io::copy_bidirectional(&mut local, &mut remote).await;
                    });
                    true
                }
                X11Target::Unix(path) => {
                    #[cfg(unix)]
                    {
                        let Ok(mut local) = tokio::net::UnixStream::connect(path).await else {
                            return false;
                        };
                        tokio::spawn(async move {
                            let _ = tokio::io::copy_bidirectional(&mut local, &mut remote).await;
                        });
                        true
                    }
                    #[cfg(not(unix))]
                    {
                        let _ = path;
                        false
                    }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hop(auth_method: &str, key_id: Option<&str>) -> JumpHop {
        JumpHop {
            host: "example.com".into(),
            port: 22,
            username: "root".into(),
            host_id: Some("h1".into()),
            key_id: key_id.map(str::to_string),
            key_path: None,
            auth_method: auth_method.into(),
            password: None,
            key_passphrase: None,
            agent: false,
            keyboard_interactive: false,
        }
    }

    #[test]
    fn auth_chain_follows_declared_method() {
        assert_eq!(
            auth_methods(&hop("key", None)),
            vec![AuthMethod::Key, AuthMethod::Agent, AuthMethod::Password]
        );
        assert_eq!(
            auth_methods(&hop("agent", None)),
            vec![AuthMethod::Agent, AuthMethod::Key, AuthMethod::Password]
        );
        assert_eq!(
            auth_methods(&hop("password", None)),
            vec![AuthMethod::Password, AuthMethod::Key, AuthMethod::Agent]
        );
    }

    #[test]
    fn missing_method_prefers_key_when_key_configured() {
        assert_eq!(auth_methods(&hop("", Some("k1")))[0], AuthMethod::Key);
        assert_eq!(auth_methods(&hop("", None))[0], AuthMethod::Password);
        assert!(auth_methods(&hop("", None)).len() <= 3);
    }

    #[test]
    fn unknown_auth_method_payload_still_parses() {
        let args: SshConnectArgs = serde_json::from_str(
            r#"{"host":"h","port":22,"username":"u","authMethod":"key","keyId":"k1","cols":80,"rows":24}"#,
        )
        .expect("deserialize");
        assert_eq!(args.auth_method, "key");
        assert_eq!(args.key_id.as_deref(), Some("k1"));
    }

    #[test]
    fn legacy_private_key_payload_is_accepted_as_path() {
        let args: SshConnectArgs = serde_json::from_str(
            r#"{"host":"h","port":22,"username":"u","privateKey":"~/.ssh/id_ed25519","cols":80,"rows":24}"#,
        )
        .expect("deserialize");
        assert_eq!(args.key_path.as_deref(), Some("~/.ssh/id_ed25519"));
    }

    fn algorithms_payload(json: &str) -> Config {
        let args: SshConnectArgs = serde_json::from_str(json).expect("deserialize");
        client_config(&args)
    }

    #[test]
    fn algorithms_payload_is_optional_and_camel_cased() {
        let plain =
            algorithms_payload(r#"{"host":"h","port":22,"username":"u","cols":80,"rows":24}"#);
        assert_eq!(
            plain.preferred.kex.as_ref(),
            Config::default().preferred.kex.as_ref()
        );

        let config = algorithms_payload(
            r#"{"host":"h","port":22,"username":"u","cols":80,"rows":24,
                "algorithms":{"hostKey":["ssh-ed25519"],"compression":["none"]}}"#,
        );
        let args: SshConnectArgs = serde_json::from_str(
            r#"{"host":"h","port":22,"username":"u","cols":80,"rows":24,
                "algorithms":{"hostKey":["ssh-ed25519"],"compression":["none"]}}"#,
        )
        .expect("deserialize");
        assert_eq!(
            args.algorithms.as_ref().and_then(|a| a.host_key.first()),
            Some(&"ssh-ed25519".to_string())
        );
        assert_eq!(config.preferred.compression.len(), 1);
    }

    #[test]
    fn kex_selection_keeps_extension_pseudo_algorithms() {
        let config = algorithms_payload(
            r#"{"host":"h","port":22,"username":"u","cols":80,"rows":24,
                "algorithms":{"kex":["curve25519-sha256"]}}"#,
        );
        let names: Vec<&str> = config
            .preferred
            .kex
            .iter()
            .map(|name| name.as_ref())
            .collect();
        assert_eq!(names.first(), Some(&"curve25519-sha256"));
        for extension in [
            "ext-info-c",
            "ext-info-s",
            "kex-strict-c-v00@openssh.com",
            "kex-strict-s-v00@openssh.com",
        ] {
            assert!(names.contains(&extension), "missing {extension}");
        }
    }

    #[test]
    fn unknown_algorithm_names_are_dropped_without_emptying_the_group() {
        let config = algorithms_payload(
            r#"{"host":"h","port":22,"username":"u","cols":80,"rows":24,
                "algorithms":{"kex":["made-up-kex"],"cipher":["made-up-cipher"],
                              "mac":["made-up-mac"],"hostKey":["made-up-key"],
                              "compression":["made-up-compression"]}}"#,
        );
        let defaults = Config::default();
        assert_eq!(
            config.preferred.kex.as_ref(),
            defaults.preferred.kex.as_ref()
        );
        assert_eq!(
            config.preferred.cipher.as_ref(),
            defaults.preferred.cipher.as_ref()
        );
        assert_eq!(
            config.preferred.mac.as_ref(),
            defaults.preferred.mac.as_ref()
        );
        assert_eq!(
            config.preferred.key.as_ref(),
            defaults.preferred.key.as_ref()
        );
        assert_eq!(
            config.preferred.compression.as_ref(),
            defaults.preferred.compression.as_ref()
        );
    }

    /// Every name exposed by the settings UI (src/ssh-algorithms.ts) must be
    /// accepted by russh's runtime registries. A russh upgrade that renames or
    /// drops an algorithm would otherwise silently remove it from a preset.
    #[test]
    fn every_exposed_algorithm_name_is_understood_by_russh() {
        let kex = [
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
        ];
        let cipher = [
            "chacha20-poly1305@openssh.com",
            "aes256-gcm@openssh.com",
            "aes128-gcm@openssh.com",
            "aes256-ctr",
            "aes192-ctr",
            "aes128-ctr",
            "aes256-cbc",
            "aes192-cbc",
            "aes128-cbc",
        ];
        let mac = [
            "hmac-sha2-512-etm@openssh.com",
            "hmac-sha2-256-etm@openssh.com",
            "hmac-sha2-512",
            "hmac-sha2-256",
            "hmac-sha1-etm@openssh.com",
            "hmac-sha1",
        ];
        let host_key = [
            "ssh-ed25519",
            "rsa-sha2-512",
            "rsa-sha2-256",
            "ecdsa-sha2-nistp256",
            "ecdsa-sha2-nistp384",
            "ecdsa-sha2-nistp521",
            "ssh-rsa",
        ];
        let compression = ["none", "zlib", "zlib@openssh.com"];

        let mut unknown = Vec::new();
        for name in kex {
            if kex::Name::try_from(name).is_err() {
                unknown.push(format!("kex:{name}"));
            }
        }
        for name in cipher {
            if cipher::Name::try_from(name).is_err() {
                unknown.push(format!("cipher:{name}"));
            }
        }
        for name in mac {
            if mac::Name::try_from(name).is_err() {
                unknown.push(format!("mac:{name}"));
            }
        }
        for name in host_key {
            if !russh::Preferred::DEFAULT
                .key
                .iter()
                .any(|algo| algo.as_str() == name)
            {
                unknown.push(format!("hostKey:{name}"));
            }
        }
        for name in compression {
            if compression::Name::try_from(name).is_err() {
                unknown.push(format!("compression:{name}"));
            }
        }
        assert!(unknown.is_empty(), "unavailable algorithms: {unknown:?}");
    }

    #[test]
    fn strict_selection_applies_all_five_groups() {
        let config = algorithms_payload(
            r#"{"host":"h","port":22,"username":"u","cols":80,"rows":24,
                "algorithms":{"kex":["curve25519-sha256"],
                              "cipher":["chacha20-poly1305@openssh.com"],
                              "mac":["hmac-sha2-256-etm@openssh.com"],
                              "hostKey":["ssh-ed25519"],
                              "compression":["none"]}}"#,
        );
        assert_eq!(config.preferred.cipher.len(), 1);
        assert_eq!(config.preferred.mac.len(), 1);
        assert_eq!(config.preferred.key.len(), 1);
        assert_eq!(config.preferred.compression.len(), 1);
        assert!(config
            .preferred
            .kex
            .iter()
            .any(|name| name.as_ref() == "curve25519-sha256"));
    }
}
