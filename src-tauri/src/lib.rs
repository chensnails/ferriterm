mod commands;
mod net_tools;

use std::sync::Arc;

use ft_core::SessionRegistry;
use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(SessionRegistry::default())
        .manage(tokio::sync::Mutex::new(ft_monitor::Monitor::new()))
        .manage(std::sync::Arc::new(net_tools::NetToolState::default()))
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let store = Arc::new(ft_store::Store::open(&dir.join("ferriterm.db"))?);
            app.manage(store.clone());
            app.manage(ft_security::CredentialStore::new(dir.clone()));
            app.manage(ft_sync::BackupStore::new(dir.join("backups")));
            app.manage(ft_plugin::PluginRegistry::new(dir.join("plugins")));
            app.manage(ft_core::RecorderRegistry::new(dir.join("recordings")));
            let transfers = ft_transfer::TransferManager::default();
            app.manage(commands::SshRuntimeState::new(
                Arc::new(ft_ssh::KnownHostsStore::new(dir.join("ssh"))),
                Arc::new(ft_ssh::ForwardRegistry::default()),
                store,
                Arc::new(ft_vfs::SftpRegistry::default()),
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
            commands::save_credential,
            commands::get_credential,
            commands::delete_credential,
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
