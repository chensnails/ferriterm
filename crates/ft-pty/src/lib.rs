use std::io::{Read, Write};
use std::sync::Arc;
use std::time::Duration;

use ft_core::{CloseReason, PtySession, Session, SessionGuard, SessionRegistry, TerminalSink};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};

/// A local PTY is opened synchronously; bound it so a broken shell or a
/// stalled pseudo-terminal allocation cannot leave the tab "connecting"
/// forever.
const SPAWN_TIMEOUT: Duration = Duration::from_secs(10);

pub async fn spawn(
    sink: Arc<dyn TerminalSink>,
    registry: &SessionRegistry,
    id: String,
    cols: u16,
    rows: u16,
    shell: Option<String>,
    cwd: Option<String>,
) -> Result<(), String> {
    // Shared with SSH: a duplicate id across protocols must be rejected too.
    let Some(guard) = registry.guards.begin(&id, sink.clone()) else {
        return Err("a connection for this session is already in progress".to_string());
    };
    let task = tokio::task::spawn_blocking(move || {
        create_pty(cols, rows, shell, cwd)
    });
    let mut pair = match tokio::time::timeout(SPAWN_TIMEOUT, task).await {
        Ok(Ok(Ok(pair))) => pair,
        Ok(Ok(Err(error))) => {
            registry.guards.take_if(&id, &guard);
            return Err(error);
        }
        Ok(Err(join)) => {
            registry.guards.take_if(&id, &guard);
            return Err(format!("failed to spawn shell: {join}"));
        }
        Err(_) => {
            registry.guards.take_if(&id, &guard);
            return Err(format!(
                "opening a local terminal timed out after {}s",
                SPAWN_TIMEOUT.as_secs()
            ));
        }
    };
    if guard.is_stopped() || !registry.guards.is_current(&id, &guard) {
        registry.guards.take_if(&id, &guard);
        // Kill the child we just spawned; nobody is listening to it.
        let _ = pair.child.kill();
        return Err("connection cancelled".to_string());
    }

    let reader = match pair.master.try_clone_reader() {
        Ok(reader) => reader,
        Err(error) => {
            registry.guards.take_if(&id, &guard);
            let _ = pair.child.kill();
            return Err(error.to_string());
        }
    };
    let writer = match pair.master.take_writer() {
        Ok(writer) => writer,
        Err(error) => {
            registry.guards.take_if(&id, &guard);
            let _ = pair.child.kill();
            return Err(error.to_string());
        }
    };

    // Publish before starting the reader: a shell that exits immediately must
    // not emit closed before spawn() has returned Ok.
    registry.sessions.lock().await.insert(
        id.clone(),
        Session::Pty(PtySession {
            master: pair.master,
            writer,
            child: pair.child,
            guard: guard.clone(),
        }),
    );

    let event_id = id.clone();
    let reader_guard = guard.clone();
    let reader_registry = registry.clone();
    let reader_sink = sink.clone();
    std::thread::spawn(move || {
        let (reason, message) = pump(reader, reader_sink, event_id.clone(), reader_guard.clone());
        finish(
            &reader_registry,
            &event_id,
            &reader_guard,
            reason,
            message,
        );
    });
    Ok(())
}

struct PtyPair {
    master: Box<dyn portable_pty::MasterPty + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
}

fn create_pty(
    cols: u16,
    rows: u16,
    shell: Option<String>,
    cwd: Option<String>,
) -> Result<PtyPair, String> {
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
    Ok(PtyPair {
        master: pair.master,
        child,
    })
}

fn pump(
    mut reader: Box<dyn Read + Send>,
    sink: Arc<dyn TerminalSink>,
    id: String,
    guard: SessionGuard,
) -> (CloseReason, Option<String>) {
    let mut buf = [0u8; 8192];
    loop {
        if guard.is_stopped() {
            return (CloseReason::User, None);
        }
        match reader.read(&mut buf) {
            Ok(0) => return (CloseReason::ShellExit, None),
            Ok(n) => sink.output(&id, buf[..n].to_vec()),
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(err) => return (CloseReason::TransportError, Some(err.to_string())),
        }
    }
}

/// The reader owns its registry entry. A shell that exits used to leave the
/// `PtySession` (and its child handle) in the map forever.
fn finish(
    registry: &SessionRegistry,
    id: &str,
    guard: &SessionGuard,
    reason: CloseReason,
    message: Option<String>,
) {
    let Some((sink, owned_guard)) = registry.guards.take_if(id, guard) else {
        return; // a newer connection owns this id
    };
    // A plain OS thread has no Tokio context; `blocking_lock` is the
    // supported way to take this mutex synchronously here.
    let mut sessions = registry.sessions.blocking_lock();
    if matches!(sessions.get(id), Some(Session::Pty(s)) if s.guard.same(guard)) {
        sessions.remove(id);
    }
    drop(sessions);
    owned_guard.finish(sink.as_ref(), id, reason, message);
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
        Some(Session::Pty(s)) if !s.guard.is_stopped() => {
            s.writer
                .write_all(data.as_bytes())
                .map_err(|e| e.to_string())?;
            s.writer.flush().map_err(|e| e.to_string())
        }
        Some(Session::Pty(_)) => Err("local terminal is closed; reconnect to continue".to_string()),
        _ => Err("pty session not found; reconnect to continue".to_string()),
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
        Some(Session::Pty(s)) if !s.guard.is_stopped() => s
            .master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| e.to_string()),
        Some(Session::Pty(_)) => Err("local terminal is closed; reconnect to continue".to_string()),
        _ => Err("pty session not found; reconnect to continue".to_string()),
    }
}

pub async fn kill(registry: &SessionRegistry, id: &str) -> Result<(), String> {
    let taken = registry.guards.take_any(id);
    let removed = {
        let mut sessions = registry.sessions.lock().await;
        match sessions.get(id) {
            // Only ever remove a PTY entry: the registry is shared with SSH.
            Some(Session::Pty(_)) => sessions.remove(id),
            _ => None,
        }
    };
    if let Some(Session::Pty(mut s)) = removed {
        let _ = s.child.kill();
        // Reap the child so Unix does not keep a zombie. `wait` follows a
        // successful kill promptly; do it off the async worker just in case.
        if matches!(s.child.try_wait(), Ok(None)) {
            std::thread::spawn(move || {
                let _ = s.child.wait();
            });
        }
    }
    if let Some((sink, guard)) = taken {
        guard.finish(sink.as_ref(), id, CloseReason::User, None);
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

    #[test]
    fn an_exited_shell_is_reported_as_shell_exit() {
        struct Sink;
        impl TerminalSink for Sink {
            fn output(&self, _id: &str, _data: Vec<u8>) {}
            fn closed(&self, _id: &str) {}
        }
        let guard = SessionGuard::default();
        let (reason, _) = pump(
            Box::new(std::io::Cursor::new(Vec::<u8>::new())) as Box<dyn Read + Send>,
            Arc::new(Sink),
            "pty-1".to_string(),
            guard,
        );
        assert_eq!(reason, CloseReason::ShellExit);
    }
}
