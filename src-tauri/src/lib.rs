mod commands;
mod net_tools;

use std::sync::Arc;

use ft_core::SessionRegistry;
use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // One registry, cloned into both the managed state and the transfer
    // resolver: a second copy would leave resumed jobs looking at an empty map.
    let sessions = SessionRegistry::default();
    tauri::Builder::default()
        .manage(sessions.clone())
        .manage(ft_serial::SerialRegistry::default())
        .manage(tokio::sync::Mutex::new(ft_monitor::Monitor::new()))
        .manage(std::sync::Arc::new(net_tools::NetToolState::default()))
        .setup(move |app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let store = Arc::new(ft_store::Store::open(&dir.join("ferriterm.db"))?);
            app.manage(store.clone());
            app.manage(Arc::new(ft_security::CredentialStore::new(dir.clone())));
            app.manage(ft_sync::BackupStore::new(dir.join("backups")));
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
                store,
                sftp,
                transfers.clone(),
            ));
            let mut updates = transfers.subscribe();
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while let Ok(job) = updates.recv().await {
                    let _ = handle.emit("transfer://update", &job);
                }
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
            commands::recording_start,
            commands::recording_stop,
            commands::recording_list,
            commands::recording_read,
            commands::import_preview,
            commands::monitor_sample,
            commands::export_backup,
            commands::import_backup,
            commands::sync_remote_test,
            commands::sync_remote_upload,
            commands::sync_remote_download,
            commands::list_language_packs,
            commands::plugin_scan,
            commands::plugin_open_dir,
            commands::plugin_import_commands,
            commands::plugin_run_wasm,
            commands::plugin_market_fetch,
            commands::plugin_market_install,
            commands::plugin_market_uninstall,
            commands::save_credential,
            commands::save_host_credential,
            commands::delete_credential,
            commands::credential_status,
            commands::save_secret,
            commands::secret_saved,
            commands::delete_secret,
            commands::vault_status,
            commands::vault_unlock,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running ferriterm");
}
