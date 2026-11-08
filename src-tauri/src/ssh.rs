use std::sync::Arc;
use std::time::Duration;

use russh::client::{self, Config};
use russh::keys::PublicKeyOrCertificate;
use russh::{ChannelMsg, Disconnect};
use serde::Deserialize;
use tauri::{AppHandle, Emitter, State};

use crate::events;
use crate::session::{AppState, Session, SshSession};

pub struct SshHandler;

impl client::Handler for SshHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        // TODO: 上线前必须实现 known_hosts/TOFU 校验；当前接受任意主机密钥，存在中间人风险
        Ok(true)
    }
}

#[derive(Deserialize)]
pub struct SshConnectArgs {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub cols: u32,
    pub rows: u32,
}

#[tauri::command]
pub async fn ssh_connect(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    args: SshConnectArgs,
) -> Result<(), String> {
    let config = Arc::new(Config::default());
    let mut handle = tokio::time::timeout(
        Duration::from_secs(15),
        client::connect(config, (args.host.as_str(), args.port), SshHandler),
    )
    .await
    .map_err(|_| "连接超时（15 秒）".to_string())?
    .map_err(|e| e.to_string())?;

    let auth = handle
        .authenticate_password(args.username.as_str(), args.password.as_str())
        .await
        .map_err(|e| e.to_string())?;
    if !auth.success() {
        return Err("认证失败：用户名或密码错误".to_string());
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
    let app_reader = app.clone();
    let event_id = id.clone();
    tokio::spawn(async move {
        while let Some(msg) = reader.wait().await {
            match msg {
                ChannelMsg::Data { data } => {
                    let _ = app_reader.emit(&events::output(&event_id), data.to_vec());
                }
                ChannelMsg::ExtendedData { data, .. } => {
                    let _ = app_reader.emit(&events::output(&event_id), data.to_vec());
                }
                ChannelMsg::Close => break,
                _ => {}
            }
        }
        let _ = app_reader.emit(&events::closed(&event_id), ());
    });

    state
        .sessions
        .lock()
        .await
        .insert(id, Session::Ssh(SshSession { handle, writer }));
    Ok(())
}

#[tauri::command]
pub async fn ssh_write(
    state: State<'_, AppState>,
    id: String,
    data: String,
) -> Result<(), String> {
    let sessions = state.sessions.lock().await;
    match sessions.get(&id) {
        Some(Session::Ssh(s)) => s
            .writer
            .data_bytes(data.into_bytes())
            .await
            .map_err(|e| e.to_string()),
        _ => Err("SSH 会话不存在".to_string()),
    }
}

#[tauri::command]
pub async fn ssh_resize(
    state: State<'_, AppState>,
    id: String,
    cols: u32,
    rows: u32,
) -> Result<(), String> {
    let sessions = state.sessions.lock().await;
    match sessions.get(&id) {
        Some(Session::Ssh(s)) => s
            .writer
            .window_change(cols, rows, 0, 0)
            .await
            .map_err(|e| e.to_string()),
        _ => Err("SSH 会话不存在".to_string()),
    }
}

#[tauri::command]
pub async fn ssh_disconnect(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let session = state.sessions.lock().await.remove(&id);
    if let Some(Session::Ssh(s)) = session {
        let _ = s
            .handle
            .disconnect(Disconnect::ByApplication, "closed by user", "")
            .await;
    }
    Ok(())
}