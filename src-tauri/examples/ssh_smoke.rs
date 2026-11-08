//! Automated SSH login + command round-trip against the loopback fixture.
//! Run `loopback_sshd` first, then this example; prints SMOKE_PASS on success.

use std::sync::Arc;
use std::time::Duration;

use russh::client::{self, Config};
use russh::keys::PublicKeyOrCertificate;
use russh::{ChannelMsg, Disconnect};

struct Handler;

impl client::Handler for Handler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        // Loopback fixture with a disposable key; real clients must do TOFU.
        Ok(true)
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut handle =
        client::connect(Arc::new(Config::default()), ("127.0.0.1", 2222), Handler).await?;
    let auth = handle
        .authenticate_password("test", "ferriterm-test")
        .await?;
    if !auth.success() {
        return Err("auth rejected".into());
    }
    let channel = handle.channel_open_session().await?;
    channel
        .request_pty(true, "xterm-256color", 80, 24, 0, 0, &[])
        .await?;
    channel.request_shell(true).await?;
    let (mut reader, writer) = channel.split();

    let mut seen = String::new();
    let mut sent_cmd = false;
    let mut sent_exit = false;
    while let Some(msg) = reader.wait().await {
        match msg {
            ChannelMsg::Data { data } => {
                seen.push_str(&String::from_utf8_lossy(&data));
                if !sent_cmd && seen.contains("test@loopback:~$ ") {
                    writer
                        .data_bytes("whoami\r".to_string().into_bytes())
                        .await?;
                    sent_cmd = true;
                } else if sent_cmd && !sent_exit && seen.contains("test\r\n") {
                    writer.data_bytes("exit\r".to_string().into_bytes()).await?;
                    sent_exit = true;
                }
            }
            ChannelMsg::Close => break,
            _ => {}
        }
    }
    if !(sent_cmd && sent_exit && seen.contains("logout")) {
        return Err(format!("unexpected transcript: {seen}").into());
    }
    let _ = handle
        .disconnect(Disconnect::ByApplication, "smoke done", "")
        .await;
    println!("SMOKE_PASS");
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    match tokio::time::timeout(Duration::from_secs(20), run()).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            eprintln!("SMOKE_FAIL: {e}");
            std::process::exit(1);
        }
        Err(_) => {
            eprintln!("SMOKE_FAIL: timeout");
            std::process::exit(1);
        }
    }
}
