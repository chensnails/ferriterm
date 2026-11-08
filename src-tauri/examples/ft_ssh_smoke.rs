//! Drives the refactored ft-ssh crate against the loopback testenv server.
//! Run loopback_sshd first, then: cargo run --example ft_ssh_smoke

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ft_core::{ServerKeyVerifier, SessionRegistry, TerminalSink};
use russh::keys::PublicKey;

struct CollectSink {
    buf: Arc<Mutex<Vec<u8>>>,
    closed: Arc<AtomicBool>,
}

/// Loopback testenv accepts whatever key the throwaway server presents.
struct TrustLocalServer;

impl ServerKeyVerifier for TrustLocalServer {
    fn verify<'a>(
        &'a self,
        _host: &'a str,
        _port: u16,
        _key: &'a PublicKey,
    ) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async { true })
    }
}

impl TerminalSink for CollectSink {
    fn output(&self, _id: &str, data: Vec<u8>) {
        self.buf.lock().unwrap().extend_from_slice(&data);
    }
    fn closed(&self, _id: &str) {
        self.closed.store(true, Ordering::SeqCst);
    }
}

async fn wait_for(buf: &Arc<Mutex<Vec<u8>>>, needle: &str) -> bool {
    for _ in 0..100 {
        if String::from_utf8_lossy(&buf.lock().unwrap()).contains(needle) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

async fn run() -> Result<(), String> {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let closed = Arc::new(AtomicBool::new(false));
    let sink = Arc::new(CollectSink {
        buf: buf.clone(),
        closed: closed.clone(),
    });
    let registry = SessionRegistry::default();
    let id = "smoke".to_string();

    ft_ssh::connect(
        sink,
        &registry,
        id.clone(),
        ft_ssh::SshConnectArgs {
            host: "127.0.0.1".into(),
            port: 2222,
            username: "test".into(),
            password: Some("ferriterm-test".into()),
            private_key: None,
            key_passphrase: None,
            keyboard_interactive: false,
            agent: false,
            x11: false,
            jump: Vec::new(),
            keepalive: None,
            host_id: None,
            cols: 80,
            rows: 24,
        },
        ft_ssh::ConnectContext::new(Arc::new(TrustLocalServer)),
    )
    .await?;

    if !wait_for(&buf, "test@loopback").await {
        return Err("no prompt".into());
    }
    ft_ssh::write(&registry, &id, "whoami\r".into()).await?;
    if !wait_for(&buf, "test\r\n").await {
        return Err("no whoami output".into());
    }
    ft_ssh::write(&registry, &id, "exit\r".into()).await?;
    for _ in 0..100 {
        if closed.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    ft_ssh::disconnect(&registry, &id).await?;
    if !closed.load(Ordering::SeqCst) {
        return Err("channel did not close".into());
    }
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    match tokio::time::timeout(Duration::from_secs(20), run()).await {
        Ok(Ok(())) => println!("FT_SSH_SMOKE_PASS"),
        Ok(Err(e)) => {
            eprintln!("FT_SSH_SMOKE_FAIL: {e}");
            std::process::exit(1);
        }
        Err(_) => {
            eprintln!("FT_SSH_SMOKE_FAIL: timeout");
            std::process::exit(1);
        }
    }
}
