use std::sync::Arc;

use super::session::TauriSink;
use ft_core::{RecorderRegistry, SessionRegistry};
use tauri::{AppHandle, State};

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn pty_spawn(
    app: AppHandle,
    state: State<'_, SessionRegistry>,
    recorders: State<'_, RecorderRegistry>,
    id: String,
    cols: u16,
    rows: u16,
    shell: Option<String>,
    cwd: Option<String>,
) -> Result<(), String> {
    let sink = Arc::new(TauriSink {
        app,
        recorders: recorders.inner().clone(),
    });
    ft_pty::spawn(sink, &state, id, cols, rows, shell, cwd).await
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

#[tauri::command]
pub fn recording_start(
    recorders: State<'_, RecorderRegistry>,
    id: String,
    title: String,
    cols: u16,
    rows: u16,
) -> Result<String, String> {
    recorders
        .start(&id, &title, cols, rows)
        .map(|path| path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn recording_stop(recorders: State<'_, RecorderRegistry>, id: String) -> Option<String> {
    recorders
        .stop(&id)
        .map(|path| path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn recording_list(
    recorders: State<'_, RecorderRegistry>,
) -> Result<Vec<ft_core::RecordingMeta>, String> {
    recorders.list()
}

#[tauri::command]
pub fn recording_read(
    recorders: State<'_, RecorderRegistry>,
    name: String,
) -> Result<ft_core::CastFile, String> {
    recorders.read(&name)
}

#[tauri::command]
pub fn import_preview(format: String, content: String) -> Result<ft_import::ImportPreview, String> {
    let format = if format == "auto" {
        None
    } else {
        Some(ft_import::parse_format(&format).ok_or("unsupported import format")?)
    };
    ft_import::preview(format, &content).map_err(|e| e.to_string())
}
