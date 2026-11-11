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
    shell: Option<String>,
    cwd: Option<String>,
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

    let mut cmd = match shell.map(|line| line.trim().to_string()) {
        Some(line) if !line.is_empty() => {
            // A saved shell may carry flags ("/bin/zsh -l"); portable-pty
            // wants the program and its arguments separately.
            let mut parts = line.split_whitespace();
            let program = parts.next().unwrap_or_default().to_string();
            let mut cmd = CommandBuilder::new(program);
            for arg in parts {
                cmd.arg(arg);
            }
            cmd
        }
        _ => CommandBuilder::new(default_shell()),
    };
    cmd.env("TERM", "xterm-256color");
    if let Some(dir) = cwd
        .map(|dir| dir.trim().to_string())
        .filter(|dir| !dir.is_empty())
    {
        cmd.cwd(expand_home(&dir));
    }
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

fn default_shell() -> String {
    #[cfg(windows)]
    {
        std::env::var("COMSPEC").unwrap_or_else(|_| "powershell.exe".to_string())
    }
    #[cfg(not(windows))]
    {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
    }
}

fn expand_home(path: &str) -> String {
    let home = if cfg!(windows) {
        std::env::var("USERPROFILE")
    } else {
        std::env::var("HOME")
    };
    expand_with_home(path, home.as_deref().ok())
}

fn expand_with_home(path: &str, home: Option<&str>) -> String {
    match home {
        Some(home) if path == "~" => home.to_string(),
        Some(home) => match path.strip_prefix("~/") {
            Some(rest) => std::path::Path::new(home)
                .join(rest)
                .to_string_lossy()
                .into_owned(),
            None => path.to_string(),
        },
        None => path.to_string(),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    const HOME: &str = "C:\\Users\\me";
    #[cfg(not(windows))]
    const HOME: &str = "/home/me";

    #[test]
    fn expands_bare_tilde_to_home() {
        assert_eq!(expand_with_home("~", Some(HOME)), HOME);
    }

    #[test]
    fn expands_path_below_home() {
        let expanded = expand_with_home("~/src/app", Some(HOME));
        assert_eq!(
            expanded,
            std::path::Path::new(HOME)
                .join("src/app")
                .to_string_lossy()
                .into_owned()
        );
    }

    #[test]
    fn leaves_paths_alone_without_a_home() {
        assert_eq!(expand_with_home("~/src", None), "~/src");
        assert_eq!(expand_with_home("/tmp", None), "/tmp");
    }

    #[test]
    fn only_expands_a_leading_bare_tilde() {
        assert_eq!(expand_with_home("~other/src", Some(HOME)), "~other/src");
        assert_eq!(expand_with_home("/srv/~/x", Some(HOME)), "/srv/~/x");
    }

    #[test]
    fn default_shell_is_never_empty() {
        assert!(!default_shell().trim().is_empty());
    }
}
