//! Exercises the SFTP VFS against the loopback testenv server.
//! Run loopback_sshd first, then: cargo run --example sftp_smoke

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use ft_core::{ServerKeyVerifier, SessionRegistry, TerminalSink};
use ft_vfs::{SftpRegistry, SftpVfs, Vfs};
use russh::keys::PublicKey;

struct NullSink;

impl TerminalSink for NullSink {
    fn output(&self, _id: &str, _data: Vec<u8>) {}
    fn closed(&self, _id: &str) {}
}

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

async fn run() -> Result<(), String> {
    let registry = SessionRegistry::default();
    let id = "sftp-smoke".to_string();
    ft_ssh::connect(
        Arc::new(NullSink),
        &registry,
        id.clone(),
        ft_ssh::SshConnectArgs {
            host: "127.0.0.1".into(),
            port: 2222,
            username: "test".into(),
            password: Some("ferriterm-test".into()),
            key_path: None,
            key_id: None,
            auth_method: String::new(),
            key_passphrase: None,
            keyboard_interactive: false,
            agent: false,
            x11: false,
            jump: Vec::new(),
            algorithms: None,
            encoding: None,
            keepalive: None,
            host_id: None,
            cols: 80,
            rows: 24,
        },
        ft_ssh::ConnectContext::new(Arc::new(TrustLocalServer)),
    )
    .await?;

    let sftp = SftpRegistry::default();
    let vfs = SftpVfs::new(sftp.get(&registry, &id).await?);
    let home = vfs.home().await?;
    if home.is_empty() || home == "." {
        return Err(format!("unexpected home {home}"));
    }

    let names: Vec<String> = vfs
        .list(&home, false)
        .await?
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    if !names.iter().any(|name| name == "README.md") {
        return Err(format!("README.md missing from {names:?}"));
    }

    let text = ft_vfs::read_text(&vfs, &format!("{home}/README.md"), 64 * 1024).await?;
    if !text.content.contains("Ferriterm loopback SFTP") {
        return Err("README content mismatch".into());
    }

    let dir = format!("{home}/smoke-dir");
    let _ = vfs.remove_dir_all(&dir).await;
    vfs.mkdir(&dir).await?;
    let file = format!("{dir}/report.txt");
    let mtime = ft_vfs::write_text(&vfs, &file, "ferriterm sftp smoke\n", None).await?;
    let updated = ft_vfs::write_text(&vfs, &file, "ferriterm sftp smoke v2\n", Some(mtime)).await?;
    if updated == 0 {
        return Err("mtime was not updated".into());
    }
    let read_back = ft_vfs::read_text(&vfs, &file, 64 * 1024).await?;
    if read_back.content != "ferriterm sftp smoke v2\n" {
        return Err(format!("content mismatch: {:?}", read_back.content));
    }
    match ft_vfs::write_text(&vfs, &file, "stale\n", Some(1)).await {
        Err(err) if err.contains("changed since") => {}
        other => return Err(format!("mtime conflict was not detected: {other:?}")),
    }

    let renamed = format!("{dir}/renamed.txt");
    vfs.rename(&file, &renamed).await?;
    vfs.chmod(&renamed, 0o640).await?;
    let entry = vfs.stat(&renamed).await?;
    if entry.mode & 0o777 != 0o640 {
        return Err(format!("chmod mismatch: {:o}", entry.mode));
    }
    if !vfs
        .list(&dir, false)
        .await?
        .iter()
        .any(|e| e.name == "renamed.txt")
    {
        return Err("renamed file missing".into());
    }

    vfs.remove_file(&renamed).await?;
    vfs.remove_dir_all(&dir).await?;
    if vfs.stat(&dir).await.is_ok() {
        return Err("directory was not removed".into());
    }

    ft_ssh::disconnect(&registry, &id).await?;
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    match tokio::time::timeout(Duration::from_secs(30), run()).await {
        Ok(Ok(())) => println!("SFTP_SMOKE_PASS"),
        Ok(Err(e)) => {
            eprintln!("SFTP_SMOKE_FAIL: {e}");
            std::process::exit(1);
        }
        Err(_) => {
            eprintln!("SFTP_SMOKE_FAIL: timeout");
            std::process::exit(1);
        }
    }
}
