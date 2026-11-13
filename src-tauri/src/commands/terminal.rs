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
    recorders: State<'_, RecorderRegistry>,
    id: String,
    data: String,
) -> Result<(), String> {
    let result = ft_pty::write(&state, &id, data.clone()).await;
    if result.is_ok() {
        recorders.write_input(&id, data.as_bytes());
    }
    result
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
    mode: Option<String>,
) -> Result<ft_core::CapturePaths, String> {
    let mode = match mode {
        Some(raw) => ft_core::CaptureMode::parse(&raw)
            .ok_or_else(|| format!("unknown capture mode: {raw}"))?,
        None => ft_core::CaptureMode::default(),
    };
    recorders.start(&id, &title, cols, rows, mode)
}

/// Changes what a running session keeps on disk without interrupting it.
#[tauri::command]
pub fn recording_set_mode(
    recorders: State<'_, RecorderRegistry>,
    id: String,
    title: String,
    cols: u16,
    rows: u16,
    mode: String,
) -> Result<ft_core::CapturePaths, String> {
    let mode = ft_core::CaptureMode::parse(&mode)
        .ok_or_else(|| format!("unknown capture mode: {mode}"))?;
    if recorders.mode(&id).is_some() {
        return recorders.set_mode(&id, mode);
    }
    // A session that is not being captured yet (a reconnect, a tab restored
    // from a snapshot) just starts in the requested mode.
    recorders.start(&id, &title, cols, rows, mode)
}

#[tauri::command]
pub fn recording_stop(
    recorders: State<'_, RecorderRegistry>,
    id: String,
) -> Option<ft_core::CapturePaths> {
    recorders.stop(&id)
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
pub fn session_log_list(
    recorders: State<'_, RecorderRegistry>,
) -> Result<Vec<ft_core::SessionLogMeta>, String> {
    recorders.list_logs()
}

#[tauri::command]
pub fn session_log_read(recorders: State<'_, RecorderRegistry>, name: String) -> Result<String, String> {
    recorders.read_log(&name)
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
