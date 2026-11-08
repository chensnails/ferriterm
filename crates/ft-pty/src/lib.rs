use std::io::{Read, Write};
use std::sync::Arc;

use ft_core::{PtySession, Session, SessionRegistry, TerminalSink};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};

pub async fn spawn(
    sink: Arc<dyn TerminalSink>,
    registry: &SessionRegistry,
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

    let event_id = id.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => sink.output(&event_id, buf[..n].to_vec()),
            }
        }
        sink.closed(&event_id);
    });

    registry.sessions.lock().await.insert(
        id,
        Session::Pty(PtySession {
            master: pair.master,
            writer,
            child,
        }),
    );
    Ok(())
}

pub async fn write(registry: &SessionRegistry, id: &str, data: String) -> Result<(), String> {
    let mut sessions = registry.sessions.lock().await;
    match sessions.get_mut(id) {
        Some(Session::Pty(s)) => {
            s.writer
                .write_all(data.as_bytes())
                .map_err(|e| e.to_string())?;
            s.writer.flush().map_err(|e| e.to_string())
        }
        _ => Err("pty session not found".to_string()),
    }
}

pub async fn resize(
    registry: &SessionRegistry,
    id: &str,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let sessions = registry.sessions.lock().await;
    match sessions.get(id) {
        Some(Session::Pty(s)) => s
            .master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| e.to_string()),
        _ => Err("pty session not found".to_string()),
    }
}

pub async fn kill(registry: &SessionRegistry, id: &str) -> Result<(), String> {
    let session = registry.sessions.lock().await.remove(id);
    if let Some(Session::Pty(mut s)) = session {
        let _ = s.child.kill();
    }
    Ok(())
}
