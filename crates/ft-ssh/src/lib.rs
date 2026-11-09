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
use russh::keys::{HashAlg, PrivateKey, PublicKey};
use russh::{ChannelMsg, Disconnect};
use serde::Deserialize;
use tokio::io::{AsyncRead, AsyncWrite};
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
}

impl ConnectContext {
    pub fn new(server_key: Arc<dyn ServerKeyVerifier>) -> Self {
        Self {
            server_key,
            keyboard_interactive: None,
            remote_forwards: None,
            credentials: None,
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
    tokio::spawn(async move {
        while let Some(msg) = reader.wait().await {
            match msg {
                ChannelMsg::Data { data } => sink.output(&event_id, data.to_vec()),
                ChannelMsg::ExtendedData { data, .. } => sink.output(&event_id, data.to_vec()),
                ChannelMsg::Close => break,
                _ => {}
            }
        }
        sink.closed(&event_id);
    });

    registry
        .sessions
        .lock()
        .await
        .insert(id, Session::Ssh(SshSession { handle, writer }));
    Ok(())
}

fn client_config(args: &SshConnectArgs) -> Config {
    let mut config = Config::default();
    let keepalive = args.keepalive.unwrap_or(30);
    if keepalive > 0 {
        config.keepalive_interval = Some(Duration::from_secs(keepalive));
        config.keepalive_max = 3;
    }
    config
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

#[cfg(unix)]
async fn authenticate_agent(
    handle: &mut client::Handle<SshHandler>,
    username: &str,
) -> Result<bool, String> {
    use russh::keys::agent::client::AgentClient;

    let mut agent = AgentClient::connect_env()
        .await
        .map_err(|e| format!("ssh-agent unavailable (SSH_AUTH_SOCK): {e}"))?;
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

#[cfg(not(unix))]
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
}
