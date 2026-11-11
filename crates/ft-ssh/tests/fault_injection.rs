//! Fault-injection acceptance for the transport: a server that goes away, or a
//! network that silently stops carrying packets, must close the session and
//! reap its registry entry within a bounded time instead of hanging the app.

use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ft_core::{ServerKeyVerifier, SessionRegistry, TerminalSink};
use ft_ssh::{connect, ConnectContext, SshConnectArgs};
use russh::keys::{PrivateKey, PublicKey};
use russh::server::{
    Auth, ChannelOpenHandle, Config as ServerConfig, Handler as ServerHandler, Msg,
    RunningServerHandle, Server, Session as ServerSession,
};
use russh::{Channel, ChannelId, MethodKind, MethodSet, Pty, SshId};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

const HOST_KEY: &str = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW
QyNTUxOQAAACBRj/S7lVzoFrtdY/lC31sdhxrnEJqOnwQcwa0UYSoGtgAAAKBt9TB4bfUw
eAAAAAtzc2gtZWQyNTUxOQAAACBRj/S7lVzoFrtdY/lC31sdhxrnEJqOnwQcwa0UYSoGtg
AAAECAmzhCTkjF7BeJ+VjgrBy5TGcYUCMbZ2jmVPFPr6A9n1GP9LuVXOgWu11j+ULfWx2H
GucQmo6fBBzBrRRhKga2AAAAFmZlcnJpdGVybS10ZXN0LWZpeHR1cmUBAgMEBQYH
-----END OPENSSH PRIVATE KEY-----";

struct AcceptAny;

impl ServerKeyVerifier for AcceptAny {
    fn verify<'a>(
        &'a self,
        _host: &'a str,
        _port: u16,
        _key: &'a PublicKey,
    ) -> Pin<Box<dyn std::future::Future<Output = bool> + Send + 'a>> {
        Box::pin(async { true })
    }
}

struct RecordingSink {
    closed: mpsc::UnboundedSender<()>,
}

impl TerminalSink for RecordingSink {
    fn output(&self, _id: &str, _data: Vec<u8>) {}

    fn closed(&self, _id: &str) {
        let _ = self.closed.send(());
    }
}

struct Fixture;

impl Server for Fixture {
    type Handler = FixtureHandler;

    fn new_client(&mut self, _peer: Option<std::net::SocketAddr>) -> Self::Handler {
        FixtureHandler
    }
}

struct FixtureHandler;

impl ServerHandler for FixtureHandler {
    type Error = russh::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        Ok(if user == "test" && password == "ferriterm-test" {
            Auth::Accept
        } else {
            Auth::reject()
        })
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        _session: &mut ServerSession,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        _cols: u32,
        _rows: u32,
        _pix_w: u32,
        _pix_h: u32,
        _modes: &[(Pty, u32)],
        session: &mut ServerSession,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut ServerSession,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        session.data(channel, "ready\r\n")?;
        Ok(())
    }
}

async fn start_fixture() -> (u16, RunningServerHandle, tokio::task::JoinHandle<()>) {
    let listener = Box::leak(Box::new(TcpListener::bind("127.0.0.1:0").await.unwrap()));
    let port = listener.local_addr().unwrap().port();
    let config = Arc::new(ServerConfig {
        server_id: SshId::Standard("SSH-2.0-FerritermFaultFixture".into()),
        methods: MethodSet::from(&[MethodKind::Password][..]),
        keys: vec![PrivateKey::from_openssh(HOST_KEY).unwrap()],
        ..Default::default()
    });
    let server: &'static mut Fixture = Box::leak(Box::new(Fixture));
    let running = server.run_on_socket(config, listener);
    let handle = running.handle();
    let task = tokio::spawn(async move {
        let _ = running.await;
    });
    (port, handle, task)
}

/// A TCP relay whose forwarding can be switched off while both sockets stay
/// open — the client sees a silent link, not a clean close.
async fn start_proxy(upstream: u16) -> (u16, Arc<AtomicBool>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let gate = Arc::new(AtomicBool::new(true));
    let switch = gate.clone();
    tokio::spawn(async move {
        while let Ok((client, _)) = listener.accept().await {
            let Ok(server) = TcpStream::connect(("127.0.0.1", upstream)).await else {
                continue;
            };
            let (mut client_read, mut client_write) = client.into_split();
            let (mut server_read, mut server_write) = server.into_split();
            let up = switch.clone();
            let down = switch.clone();
            tokio::spawn(async move {
                forward(&mut client_read, &mut server_write, up).await;
            });
            tokio::spawn(async move {
                forward(&mut server_read, &mut client_write, down).await;
            });
        }
    });
    (port, gate)
}

async fn forward<R, W>(reader: &mut R, writer: &mut W, gate: Arc<AtomicBool>)
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buffer = vec![0u8; 16 * 1024];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(read) if gate.load(Ordering::Relaxed) => {
                if writer.write_all(&buffer[..read]).await.is_err() {
                    break;
                }
                let _ = writer.flush().await;
            }
            Ok(_) => {}
        }
    }
}

fn args(port: u16, keepalive: Option<u64>) -> SshConnectArgs {
    SshConnectArgs {
        host: "127.0.0.1".into(),
        port,
        username: "test".into(),
        password: Some("ferriterm-test".into()),
        key_path: None,
        key_passphrase: None,
        key_id: None,
        auth_method: "password".into(),
        keyboard_interactive: false,
        agent: false,
        x11: false,
        jump: Vec::new(),
        keepalive,
        host_id: None,
        algorithms: None,
        cols: 80,
        rows: 24,
    }
}

async fn connect_session(
    port: u16,
    keepalive: Option<u64>,
) -> (SessionRegistry, mpsc::UnboundedReceiver<()>) {
    let registry = SessionRegistry::default();
    let (closed, events) = mpsc::unbounded_channel();
    connect(
        Arc::new(RecordingSink { closed }),
        &registry,
        "fixture".into(),
        args(port, keepalive),
        ConnectContext::new(Arc::new(AcceptAny)),
    )
    .await
    .expect("fixture connection");
    (registry, events)
}

#[tokio::test]
async fn a_server_that_disappears_reaps_the_session() {
    let (port, server, task) = start_fixture().await;
    let (registry, mut closed) = connect_session(port, None).await;

    server.shutdown("fixture stopping".into());
    let event = tokio::time::timeout(Duration::from_secs(10), closed.recv()).await;
    assert!(
        event.is_ok(),
        "a vanished server never surfaced a close event"
    );
    assert!(
        registry.sessions.lock().await.get("fixture").is_none(),
        "the dead session stayed in the registry"
    );
    let _ = task.await;
}

#[tokio::test]
async fn a_silent_link_is_closed_by_keepalive() {
    let (upstream, server, task) = start_fixture().await;
    let (proxy, gate) = start_proxy(upstream).await;
    let (registry, mut closed) = connect_session(proxy, Some(1)).await;

    gate.store(false, Ordering::Relaxed);
    let event = tokio::time::timeout(Duration::from_secs(20), closed.recv()).await;
    assert!(
        event.is_ok(),
        "a blackholed link never surfaced a close event"
    );
    assert!(
        registry.sessions.lock().await.get("fixture").is_none(),
        "the half-open session stayed in the registry"
    );
    server.shutdown("fixture stopping".into());
    let _ = task.await;
}
