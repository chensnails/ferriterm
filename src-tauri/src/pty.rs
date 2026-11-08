use std::io::{Read, Write};

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use tauri::{AppHandle, Emitter, State};

use crate::events;
use crate::session::{AppState, PtySession, Session};

#[tauri::command]
pub async fn pty_spawn(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())?;

    #[cfg(windows)]
    let shell = std::env::var("COMSPEC").unwrap_or_else(|_| "powershell.exe".to_string());
    #[cfg(not(windows))]
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());

    let mut cmd = CommandBuilder::new(shell);
    cmd.env("TERM", "xterm-256color");
    let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;

    let app_reader = app.clone();
    let event_id = id.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let _ = app_reader.emit(&events::output(&event_id), buf[..n].to_vec());
                }
            }
        }
        let _ = app_reader.emit(&events::closed(&event_id), ());
    });

    state.sessions.lock().await.insert(
        id,
        Session::Pty(PtySession {
            master: pair.master,
            writer,
            child,
        }),
    );
    Ok(())
}

#[tauri::command]
pub async fn pty_write(
    state: State<'_, AppState>,
    id: String,
    data: String,
) -> Result<(), String> {
    let mut sessions = state.sessions.lock().await;
    match sessions.get_mut(&id) {
        Some(Session::Pty(s)) => {
            s.writer.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
            s.writer.flush().map_err(|e| e.to_string())
        }
        _ => Err("PTY 会话不存在".to_string()),
    }
}

#[tauri::command]
pub async fn pty_resize(
    state: State<'_, AppState>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let sessions = state.sessions.lock().await;
    match sessions.get(&id) {
        Some(Session::Pty(s)) => s
            .master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| e.to_string()),
        _ => Err("PTY 会话不存在".to_string()),
    }
}

#[tauri::command]
pub async fn pty_kill(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let session = state.sessions.lock().await.remove(&id);
    if let Some(Session::Pty(mut s)) = session {
        let _ = s.child.kill();
    }
    Ok(())
}