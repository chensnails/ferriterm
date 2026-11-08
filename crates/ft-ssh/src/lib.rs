use std::sync::Arc;
use std::time::Duration;

use ft_core::{Session, SessionRegistry, SshHandler, SshSession, TerminalSink};
use russh::client::{self, Config};
use russh::{ChannelMsg, Disconnect};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct SshConnectArgs {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub cols: u32,
    pub rows: u32,
}

pub async fn connect(
    sink: Arc<dyn TerminalSink>,
    registry: &SessionRegistry,
    id: String,
    args: SshConnectArgs,
) -> Result<(), String> {
    let config = Arc::new(Config::default());
    let mut handle = tokio::time::timeout(
        Duration::from_secs(15),
        client::connect(config, (args.host.as_str(), args.port), SshHandler),
    )
    .await
    .map_err(|_| "connection timed out (15s)".to_string())?
    .map_err(|e| e.to_string())?;

    let auth = handle
        .authenticate_password(args.username.as_str(), args.password.as_str())
        .await
        .map_err(|e| e.to_string())?;
    if !auth.success() {
        return Err("authentication failed: bad username or password".to_string());
    }

    let channel = handle
        .channel_open_session()
        .await
        .map_err(|e| e.to_string())?;
    channel
        .request_pty(true, "xterm-256color", args.cols, args.rows, 0, 0, &[])
        .await
        .map_err(|e| e.to_string())?;
    channel
        .request_shell(true)
        .await
        .map_err(|e| e.to_string())?;

    let (mut reader, writer) = channel.split();
    let event_id = id.clone();
    tokio::spawn(async move {
        while let Some(msg) = reader.wait().await {
            match msg {
                ChannelMsg::Data { data } => sink.output(&event_id, data.to_vec()),
                ChannelMsg::ExtendedData { data, .. } => sink.output(&event_id, data.to_vec()),
                ChannelMsg::Close => break,
                _ => {}
            }
        }
        sink.closed(&event_id);
    });

    registry
        .sessions
        .lock()
        .await
        .insert(id, Session::Ssh(SshSession { handle, writer }));
    Ok(())
}

pub async fn write(registry: &SessionRegistry, id: &str, data: String) -> Result<(), String> {
    let sessions = registry.sessions.lock().await;
    match sessions.get(id) {
        Some(Session::Ssh(s)) => s
            .writer
            .data_bytes(data.into_bytes())
            .await
            .map_err(|e| e.to_string()),
        _ => Err("ssh session not found".to_string()),
    }
}

pub async fn resize(
    registry: &SessionRegistry,
    id: &str,
    cols: u32,
    rows: u32,
) -> Result<(), String> {
    let sessions = registry.sessions.lock().await;
    match sessions.get(id) {
        Some(Session::Ssh(s)) => s
            .writer
            .window_change(cols, rows, 0, 0)
            .await
            .map_err(|e| e.to_string()),
        _ => Err("ssh session not found".to_string()),
    }
}

pub async fn disconnect(registry: &SessionRegistry, id: &str) -> Result<(), String> {
    let session = registry.sessions.lock().await.remove(id);
    if let Some(Session::Ssh(s)) = session {
        let _ = s
            .handle
            .disconnect(Disconnect::ByApplication, "closed by user", "")
            .await;
    }
    Ok(())
}
