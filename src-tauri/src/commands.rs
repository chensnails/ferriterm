use std::sync::Arc;

use ft_core::{events, SessionRegistry, TerminalSink};
use tauri::{AppHandle, Emitter, State};

struct TauriSink(AppHandle);

impl TerminalSink for TauriSink {
    fn output(&self, id: &str, data: Vec<u8>) {
        let _ = self.0.emit(&events::output(id), data);
    }
    fn closed(&self, id: &str) {
        let _ = self.0.emit(&events::closed(id), ());
    }
}

#[tauri::command]
pub async fn ssh_connect(
    app: AppHandle,
    state: State<'_, SessionRegistry>,
    id: String,
    args: ft_ssh::SshConnectArgs,
) -> Result<(), String> {
    ft_ssh::connect(Arc::new(TauriSink(app)), &state, id, args).await
}

#[tauri::command]
pub async fn ssh_write(
    state: State<'_, SessionRegistry>,
    id: String,
    data: String,
) -> Result<(), String> {
    ft_ssh::write(&state, &id, data).await
}

#[tauri::command]
pub async fn ssh_resize(
    state: State<'_, SessionRegistry>,
    id: String,
    cols: u32,
    rows: u32,
) -> Result<(), String> {
    ft_ssh::resize(&state, &id, cols, rows).await
}

#[tauri::command]
pub async fn ssh_disconnect(state: State<'_, SessionRegistry>, id: String) -> Result<(), String> {
    ft_ssh::disconnect(&state, &id).await
}

#[tauri::command]
pub async fn pty_spawn(
    app: AppHandle,
    state: State<'_, SessionRegistry>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    ft_pty::spawn(Arc::new(TauriSink(app)), &state, id, cols, rows).await
}

#[tauri::command]
pub async fn pty_write(
    state: State<'_, SessionRegistry>,
    id: String,
    data: String,
) -> Result<(), String> {
    ft_pty::write(&state, &id, data).await
}

#[tauri::command]
pub async fn pty_resize(
    state: State<'_, SessionRegistry>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    ft_pty::resize(&state, &id, cols, rows).await
}

#[tauri::command]
pub async fn pty_kill(state: State<'_, SessionRegistry>, id: String) -> Result<(), String> {
    ft_pty::kill(&state, &id).await
}
