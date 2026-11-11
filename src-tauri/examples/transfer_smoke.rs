//! Exercises the transfer queue and directory sync against the loopback testenv.
//! Run loopback_sshd first, then: cargo run --example transfer_smoke

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use ft_core::{ServerKeyVerifier, SessionRegistry, TerminalSink};
use ft_transfer::{Endpoint, TransferManager, TransferStatus};
use ft_vfs::{LocalFs, SftpRegistry, SftpVfs, Vfs};
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

fn fixture_root(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(name);
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

async fn wait_jobs(manager: &TransferManager, expected: usize) -> Result<(), String> {
    for _ in 0..200 {
        let jobs = manager.list().await;
        if jobs.len() >= expected
            && jobs.iter().all(|job| {
                matches!(
                    job.status,
                    TransferStatus::Done | TransferStatus::Failed | TransferStatus::Cancelled
                )
            })
        {
            for job in &jobs {
                if job.status != TransferStatus::Done {
                    return Err(format!(
                        "job {} ended as {:?}: {:?}",
                        job.id, job.status, job.error
                    ));
                }
            }
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err("transfer jobs did not finish in time".into())
}

async fn run() -> Result<(), String> {
    let registry = SessionRegistry::default();
    let id = "transfer-smoke".to_string();
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

    let local_root = fixture_root("ferriterm-transfer-local");
    std::fs::write(local_root.join("single.txt"), "single file\n").unwrap();
    let tree = local_root.join("tree");
    std::fs::create_dir_all(tree.join("nested")).unwrap();
    std::fs::write(tree.join("a.txt"), "alpha\n").unwrap();
    std::fs::write(tree.join("nested/b.bin"), vec![7u8; 200_000]).unwrap();

    let sftp = SftpRegistry::default();
    let sftp_vfs: Arc<dyn Vfs> = Arc::new(SftpVfs::new(sftp.get(&registry, &id).await?));
    let home = sftp_vfs.home().await?;
    let local_vfs: Arc<dyn Vfs> = Arc::new(LocalFs);

    let manager = TransferManager::new(6);
    manager
        .enqueue_file(
            "up-file".into(),
            Endpoint::new(
                local_vfs.clone(),
                local_root.join("single.txt").to_string_lossy(),
            ),
            Endpoint::new(sftp_vfs.clone(), format!("{home}/up-single.txt")),
            12,
        )
        .await;
    manager
        .enqueue_tree(
            "up-tree".into(),
            Endpoint::new(local_vfs.clone(), tree.to_string_lossy()),
            Endpoint::new(sftp_vfs.clone(), format!("{home}/up-tree")),
        )
        .await?;
    wait_jobs(&manager, 2).await?;

    let single = ft_vfs::read_text(&*sftp_vfs, &format!("{home}/up-single.txt"), 4096).await?;
    if single.content != "single file\n" {
        return Err(format!("uploaded file mismatch: {:?}", single.content));
    }
    let b = ft_vfs::read_text(&*sftp_vfs, &format!("{home}/up-tree/nested/b.bin"), 4096).await;
    if b.map(|file| file.size).unwrap_or(0) != 200_000 {
        return Err("uploaded binary size mismatch".into());
    }

    let download_root = fixture_root("ferriterm-transfer-download");
    manager
        .enqueue_tree(
            "down-tree".into(),
            Endpoint::new(sftp_vfs.clone(), format!("{home}/up-tree")),
            Endpoint::new(
                local_vfs.clone(),
                download_root.join("tree").to_string_lossy(),
            ),
        )
        .await?;
    wait_jobs(&manager, 3).await?;
    let downloaded = std::fs::read(download_root.join("tree/nested/b.bin")).unwrap();
    if downloaded != vec![7u8; 200_000] {
        return Err("downloaded binary mismatch".into());
    }

    let sync_target = format!("{home}/sync-target");
    let plan = ft_transfer::preview_sync(
        local_vfs.as_ref(),
        &tree.to_string_lossy(),
        sftp_vfs.as_ref(),
        &sync_target,
        false,
    )
    .await?;
    if plan.up_files != 2 || plan.create_dirs == 0 {
        return Err(format!(
            "unexpected sync plan: {} dirs, {} files",
            plan.create_dirs, plan.up_files
        ));
    }
    ft_transfer::apply_sync(
        &manager,
        &Endpoint::new(local_vfs.clone(), tree.to_string_lossy()),
        &Endpoint::new(sftp_vfs.clone(), sync_target.clone()),
        &plan,
        "sync",
    )
    .await?;
    wait_jobs(&manager, 3 + plan.up_files).await?;
    let synced = ft_vfs::read_text(&*sftp_vfs, &format!("{sync_target}/a.txt"), 4096).await?;
    if synced.content != "alpha\n" {
        return Err("synced file mismatch".into());
    }

    let _ = ft_vfs::read_text(&*sftp_vfs, &format!("{home}/README.md"), 4096).await?;
    let _ = sftp_vfs.remove_dir_all(&format!("{home}/up-tree")).await;
    let _ = sftp_vfs.remove_dir_all(&sync_target).await;
    let _ = sftp_vfs.remove_file(&format!("{home}/up-single.txt")).await;
    let _ = std::fs::remove_dir_all(&local_root);
    let _ = std::fs::remove_dir_all(&download_root);
    ft_ssh::disconnect(&registry, &id).await?;
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    match tokio::time::timeout(Duration::from_secs(90), run()).await {
        Ok(Ok(())) => println!("TRANSFER_SMOKE_PASS"),
        Ok(Err(e)) => {
            eprintln!("TRANSFER_SMOKE_FAIL: {e}");
            std::process::exit(1);
        }
        Err(_) => {
            eprintln!("TRANSFER_SMOKE_FAIL: timeout");
            std::process::exit(1);
        }
    }
}
