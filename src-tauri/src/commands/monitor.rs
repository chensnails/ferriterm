use std::sync::Arc;

use ft_core::SessionRegistry;
use tauri::State;

#[tauri::command]
pub async fn monitor_sample(
    state: State<'_, tokio::sync::Mutex<ft_monitor::Monitor>>,
) -> Result<ft_monitor::MonitorSnapshot, String> {
    let mut monitor = state.lock().await;
    Ok(monitor.sample().await)
}

/// Samples a connected SSH session with the fixed read-only probe. The output
/// is capped and the call times out so a stalled host cannot wedge the panel.
/// The wall time of the exec is reported as the probe latency.
#[tauri::command]
pub async fn monitor_remote(
    sessions: State<'_, SessionRegistry>,
    remote: State<'_, Arc<ft_monitor::RemoteMonitor>>,
    session_id: String,
) -> Result<ft_monitor::RemoteSnapshot, String> {
    let started = std::time::Instant::now();
    let output = ft_ssh::exec(
        &sessions,
        &session_id,
        ft_monitor::REMOTE_PROBE,
        std::time::Duration::from_secs(6),
        64 * 1024,
    )
    .await?;
    let latency_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    remote.sample(&session_id, &output, latency_ms)
}

/// Politely terminates a process on a monitored host. Only PIDs from the last
/// rendered process list are accepted, the pid is numeric by type, and the
/// command is a constant string, so nothing from the UI reaches a shell.
#[tauri::command]
pub async fn monitor_kill_process(
    sessions: State<'_, SessionRegistry>,
    remote: State<'_, Arc<ft_monitor::RemoteMonitor>>,
    session_id: String,
    pid: u32,
) -> Result<(), String> {
    if pid <= 1 {
        return Err("refusing to signal pid 1".to_string());
    }
    if !remote.is_visible(&session_id, pid) {
        return Err(
            "that process is no longer in the monitored list; refresh the panel".to_string(),
        );
    }
    ft_ssh::exec(
        &sessions,
        &session_id,
        &format!("kill -TERM {pid}"),
        std::time::Duration::from_secs(5),
        4096,
    )
    .await
    .map(|_| ())
}
