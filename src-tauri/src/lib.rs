mod agent;
mod ai;
mod commands;
mod logging;
mod net_tools;
mod policy;
mod updater;

use std::sync::Arc;

use ft_core::SessionRegistry;
use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // One registry, cloned into both the managed state and the transfer
    // resolver: a second copy would leave resumed jobs looking at an empty map.
    let sessions = SessionRegistry::default();
    // Shared by the window and the control service, so both call a session by
    // the same host name.
    let labels = Arc::new(commands::SessionLabels::default());
    let remote_monitor = Arc::new(ft_monitor::RemoteMonitor::default());
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(sessions.clone())
        .manage(ft_serial::SerialRegistry::default())
        .manage(ft_telnet::TelnetRegistry::default())
        .manage(tokio::sync::Mutex::new(ft_monitor::Monitor::new()))
        .manage(remote_monitor.clone())
        .manage(std::sync::Arc::new(net_tools::NetToolState::default()))
        .setup(move |app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            // Diagnostics are best effort: a missing log file must not stop
            // the app, so a failure here only costs the on-disk trail.
            if let Err(err) = logging::init(&dir.join("logs")) {
                eprintln!("ferriterm: log file unavailable: {err}");
            }
            tracing::info!(
                version = env!("CARGO_PKG_VERSION"),
                dir = %dir.display(),
                "ferriterm starting"
            );
            let store = Arc::new(ft_store::Store::open(&dir.join("ferriterm.db"))?);
            app.manage(store.clone());
            app.manage(Arc::new(ft_security::CredentialStore::new(dir.clone())));
            app.manage(ft_sync::BackupStore::new(dir.join("backups")));
            let audit = ft_policy::AuditLog::new(dir.join("audit"));
            app.manage(audit.clone());
            app.manage(ft_plugin::PluginRegistry::new(dir.join("plugins")));
            app.manage(ft_core::RecorderRegistry::new(dir.join("recordings")));
            let sftp = Arc::new(ft_vfs::SftpRegistry::default());
            let transfers =
                commands::transfer_manager(store.clone(), sessions.clone(), sftp.clone());
            // Jobs the last run left unfinished come back paused; nothing
            // reconnects on its own, so this is only a list.
            let restored = commands::restored_transfers(&store);
            if !restored.is_empty() {
                tauri::async_runtime::block_on(transfers.restore(restored));
            }
            app.manage(commands::SshRuntimeState::new(
                Arc::new(ft_ssh::KnownHostsStore::new(dir.join("ssh"))),
                Arc::new(ft_ssh::ForwardRegistry::default()),
                store.clone(),
                sftp.clone(),
                transfers.clone(),
                remote_monitor,
                labels.clone(),
            ));
            // Bound here rather than later so the endpoint file exists before
            // anything can look for it. A failure costs the CLI, not the app.
            if let Err(err) = agent::start(
                &dir,
                sessions.clone(),
                labels.clone(),
                store.clone(),
                sftp.clone(),
                audit.clone(),
            ) {
                tracing::warn!(error = %err, "control service unavailable");
            }
            let updates = transfers.subscribe();
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                forward_updates(updates, move |job| {
                    let _ = handle.emit("transfer://update", job);
                })
                .await;
            });
            std::fs::create_dir_all(dir.join("plugins"))?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::ssh_connect,
            commands::ssh_host_key_respond,
            commands::ssh_keyboard_interactive_respond,
            commands::ssh_write,
            commands::ssh_resize,
            commands::ssh_disconnect,
            commands::forward_start,
            commands::forward_stop,
            commands::forward_list,
            commands::list_forward_rules,
            commands::save_forward_rule,
            commands::delete_forward_rule,
            commands::pty_spawn,
            commands::pty_write,
            commands::pty_resize,
            commands::pty_kill,
            commands::serial_list_ports,
            commands::serial_open,
            commands::serial_write,
            commands::serial_close,
            commands::telnet_open,
            commands::telnet_write,
            commands::telnet_resize,
            commands::telnet_close,
            commands::recording_start,
            commands::recording_stop,
            commands::recording_list,
            commands::recording_read,
            commands::import_preview,
            commands::monitor_sample,
            commands::monitor_remote,
            commands::monitor_kill_process,
            commands::export_backup,
            commands::import_backup,
            commands::sync_remote_test,
            commands::sync_remote_upload,
            commands::sync_auto_upload,
            commands::sync_remote_download,
            commands::sync_versions_list,
            commands::sync_version_restore,
            commands::list_language_packs,
            commands::plugin_scan,
            commands::plugin_open_dir,
            commands::plugin_import_commands,
            commands::plugin_run_wasm,
            commands::plugin_market_fetch,
            commands::plugin_market_install,
            commands::plugin_market_uninstall,
            commands::plugin_install_builtin,
            commands::save_credential,
            commands::save_host_credential,
            commands::delete_credential,
            commands::credential_status,
            commands::save_secret,
            commands::secret_saved,
            commands::delete_secret,
            commands::vault_status,
            commands::vault_unlock,
            updater::update_check,
            updater::update_install,
            commands::vault_lock,
            commands::vault_enable_master_password,
            commands::vault_disable_master_password,
            commands::vault_change_master_password,
            commands::vault_reset,
            commands::vault_list_entries,
            commands::vault_delete_entry,
            commands::vault_clear_entries,
            commands::ssh_key_list,
            commands::ssh_key_scan,
            commands::ssh_key_inspect,
            commands::ssh_key_import,
            commands::ssh_key_generate,
            commands::ssh_key_delete,
            commands::ssh_key_update_metadata,
            commands::ssh_key_rotate,
            commands::ssh_key_export_public,
            commands::ssh_key_export_bundle,
            commands::ssh_key_import_bundle,
            commands::known_hosts_list,
            commands::known_hosts_remove,
            commands::credential_audit_list,
            commands::get_settings,
            commands::set_setting,
            commands::list_command_groups,
            commands::create_command_group,
            commands::update_command_group,
            commands::delete_command_group,
            commands::create_command,
            commands::update_command,
            commands::delete_command,
            commands::list_hosts,
            commands::create_host,
            commands::update_host,
            commands::delete_host,
            commands::list_tags,
            commands::create_tag,
            commands::update_tag,
            commands::delete_tag,
            commands::vfs_list,
            commands::vfs_stat,
            commands::vfs_home,
            commands::vfs_mkdir,
            commands::vfs_remove,
            commands::vfs_rename,
            commands::vfs_chmod,
            commands::vfs_read_text,
            commands::vfs_write_text,
            commands::remote_mount_add,
            commands::remote_mount_remove,
            commands::remote_mount_list,
            commands::transfer_enqueue,
            commands::transfer_set_concurrency,
            commands::transfer_list,
            commands::transfer_pause,
            commands::transfer_resume,
            commands::transfer_cancel,
            commands::transfer_retry,
            commands::transfer_remove,
            commands::transfer_clear_finished,
            commands::sync_preview,
            commands::sync_apply,
            net_tools::net_tool_start,
            net_tools::net_tool_cancel,
            net_tools::geoip_lookup,
            ai::ai_list_providers,
            ai::ai_save_provider,
            ai::ai_delete_provider,
            ai::ai_presets,
            ai::ai_test_provider,
            ai::ai_list_models,
            ai::ai_chat,
            ai::ai_chat_stream,
            ai::ai_generate_command,
            ai::ai_explain,
            policy::policy_get,
            policy::policy_save,
            policy::policy_evaluate,
            policy::policy_add_rule,
            policy::policy_remove_rule,
            policy::policy_record,
            policy::policy_audit_months,
            policy::policy_audit_list,
            policy::policy_audit_verify,
            policy::policy_audit_path,
            policy::policy_preview,
            agent::agent_cli_path,
        ])
        .build(tauri::generate_context!())
        .expect("error while building ferriterm")
        .run(|_app, event| {
            // The endpoint file holds a token that dies with this process, so
            // it should not outlive it either.
            if let tauri::RunEvent::Exit = event {
                agent::clear_endpoint();
            }
        });
}

/// Forward broadcast events to a sink until the sender is dropped. A slow
/// consumer only loses intermediate snapshots (Lagged), never the stream:
/// treating lag as termination froze the transfer UI forever after one burst.
async fn forward_updates<T, F>(mut updates: tokio::sync::broadcast::Receiver<T>, mut emit: F)
where
    T: Clone,
    F: FnMut(&T) + Send,
{
    loop {
        match updates.recv().await {
            Ok(job) => emit(&job),
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::forward_updates;

    #[tokio::test]
    async fn lagged_forwarder_resumes_with_the_latest_event() {
        let (tx, rx) = tokio::sync::broadcast::channel::<u32>(1);
        for i in 1..=3 {
            // Nothing drains rx yet, so the forwarder will report lag first.
            let _ = tx.send(i);
        }
        drop(tx); // close the stream after the backlog so the call returns
        let mut seen = vec![];
        forward_updates(rx, |job| seen.push(*job)).await;
        assert_eq!(seen, vec![3]);
    }
}
