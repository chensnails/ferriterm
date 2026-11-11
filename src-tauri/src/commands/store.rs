use std::sync::Arc;

use super::vault::{passphrase_ref, password_ref, Credentials};
use tauri::State;

#[tauri::command]
pub fn credential_audit_list(
    store: State<'_, Arc<ft_store::Store>>,
    limit: Option<usize>,
) -> Result<Vec<ft_store::AuditRecord>, String> {
    store
        .list_audit(limit.unwrap_or(100))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_command_groups(
    state: State<'_, Arc<ft_store::Store>>,
) -> Result<Vec<ft_store::CommandGroup>, String> {
    state.list_command_groups().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_command_group(
    state: State<'_, Arc<ft_store::Store>>,
    id: String,
    name: String,
) -> Result<(), String> {
    state
        .create_command_group(&id, &name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_command_group(
    state: State<'_, Arc<ft_store::Store>>,
    id: String,
    name: String,
) -> Result<(), String> {
    state
        .update_command_group(&id, &name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_command_group(
    state: State<'_, Arc<ft_store::Store>>,
    id: String,
) -> Result<(), String> {
    state.delete_command_group(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_command(
    state: State<'_, Arc<ft_store::Store>>,
    id: String,
    group_id: String,
    name: String,
    template: String,
) -> Result<(), String> {
    state
        .create_command(&id, &group_id, &name, &template)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_command(
    state: State<'_, Arc<ft_store::Store>>,
    id: String,
    name: String,
    template: String,
) -> Result<(), String> {
    state
        .update_command(&id, &name, &template)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_command(state: State<'_, Arc<ft_store::Store>>, id: String) -> Result<(), String> {
    state.delete_command(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_settings(
    state: State<'_, Arc<ft_store::Store>>,
) -> Result<std::collections::BTreeMap<String, String>, String> {
    state.list_settings().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_setting(
    state: State<'_, Arc<ft_store::Store>>,
    key: String,
    value: String,
) -> Result<(), String> {
    state.set_setting(&key, &value).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_hosts(state: State<'_, Arc<ft_store::Store>>) -> Result<Vec<ft_store::Host>, String> {
    state.list_hosts().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_host(
    state: State<'_, Arc<ft_store::Store>>,
    input: ft_store::HostInput,
) -> Result<(), String> {
    state.create_host(&input).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_host(
    state: State<'_, Arc<ft_store::Store>>,
    input: ft_store::HostInput,
) -> Result<(), String> {
    state.update_host(&input).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_host(
    state: State<'_, Arc<ft_store::Store>>,
    credentials: State<'_, Credentials>,
    id: String,
) -> Result<(), String> {
    for reference in [password_ref(&id), passphrase_ref(&id), id.clone()] {
        // Best effort: a failing vault delete must not block removing the host.
        if let Err(err) = credentials.delete(&reference) {
            let _ = state.append_audit("secret.delete.failed", &id, &err.to_string());
        }
        let _ = state.delete_vault_entry(&reference);
    }
    state.delete_host(&id).map_err(|e| e.to_string())?;
    let _ = state.append_audit("host.delete", &id, "");
    Ok(())
}

#[tauri::command]
pub fn list_tags(state: State<'_, Arc<ft_store::Store>>) -> Result<Vec<ft_store::Tag>, String> {
    state.list_tags().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_tag(
    state: State<'_, Arc<ft_store::Store>>,
    tag: ft_store::Tag,
) -> Result<(), String> {
    state.create_tag(&tag).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_tag(
    state: State<'_, Arc<ft_store::Store>>,
    tag: ft_store::Tag,
) -> Result<(), String> {
    state.update_tag(&tag).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_tag(state: State<'_, Arc<ft_store::Store>>, tag_id: String) -> Result<(), String> {
    state.delete_tag(&tag_id).map_err(|e| e.to_string())
}
