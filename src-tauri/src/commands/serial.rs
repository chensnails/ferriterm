use std::sync::Arc;

use super::plugin::{serial_plugin_installed, telnet_plugin_installed};
use super::session::TauriSink;
use ft_core::RecorderRegistry;
use tauri::{AppHandle, State};

#[tauri::command]
pub fn serial_list_ports() -> Result<Vec<ft_serial::SerialPortInfo>, String> {
    ft_serial::list_ports()
}
#[tauri::command]
pub async fn serial_open(
    app: AppHandle,
    plugins: State<'_, ft_plugin::PluginRegistry>,
    state: State<'_, ft_serial::SerialRegistry>,
    recorders: State<'_, RecorderRegistry>,
    id: String,
    config: ft_serial::SerialConfig,
) -> Result<(), String> {
    if !serial_plugin_installed(&plugins) {
        return Err("serial support requires the Serial Ports plugin".to_string());
    }
    let sink = Arc::new(TauriSink {
        app,
        recorders: recorders.inner().clone(),
    });
    ft_serial::open(sink, &state, id, config).await
}

#[tauri::command]
pub async fn serial_write(
    state: State<'_, ft_serial::SerialRegistry>,
    recorders: State<'_, RecorderRegistry>,
    id: String,
    data: String,
) -> Result<(), String> {
    let result = ft_serial::write(&state, &id, data.clone()).await;
    if result.is_ok() {
        recorders.write_input(&id, data.as_bytes());
    }
    result
}

#[tauri::command]
pub async fn serial_close(
    state: State<'_, ft_serial::SerialRegistry>,
    id: String,
) -> Result<(), String> {
    ft_serial::close(&state, &id).await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn telnet_open(
    app: AppHandle,
    plugins: State<'_, ft_plugin::PluginRegistry>,
    state: State<'_, ft_telnet::TelnetRegistry>,
    recorders: State<'_, RecorderRegistry>,
    id: String,
    host: String,
    port: u16,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    if !telnet_plugin_installed(&plugins) {
        return Err("telnet support requires the Telnet plugin".to_string());
    }
    let sink = Arc::new(TauriSink {
        app,
        recorders: recorders.inner().clone(),
    });
    ft_telnet::open(sink, &state, id, host, port, cols, rows).await
}

#[tauri::command]
pub async fn telnet_write(
    state: State<'_, ft_telnet::TelnetRegistry>,
    recorders: State<'_, RecorderRegistry>,
    id: String,
    data: String,
) -> Result<(), String> {
    let result = ft_telnet::write(&state, &id, data.clone()).await;
    if result.is_ok() {
        recorders.write_input(&id, data.as_bytes());
    }
    result
}

#[tauri::command]
pub async fn telnet_resize(
    state: State<'_, ft_telnet::TelnetRegistry>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    ft_telnet::resize(&state, &id, cols, rows).await
}

#[tauri::command]
pub async fn telnet_close(
    state: State<'_, ft_telnet::TelnetRegistry>,
    id: String,
) -> Result<(), String> {
    ft_telnet::close(&state, &id).await
}
