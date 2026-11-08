use std::collections::HashMap;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use ft_core::{RemoteForwardHandler, Session, SessionRegistry, SshHandler};
use russh::client;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{watch, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ForwardKind {
    Local,
    Remote,
    Dynamic,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForwardRequest {
    pub id: String,
    pub session_id: String,
    #[serde(default)]
    pub kind: Option<ForwardKind>,
    #[serde(default)]
    pub bind_host: Option<String>,
    pub bind_port: u16,
    #[serde(default)]
    pub dest_host: String,
    #[serde(default)]
    pub dest_port: u16,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForwardInfo {
    pub id: String,
    pub session_id: String,
    pub kind: ForwardKind,
    pub bind_host: String,
    pub bind_port: u16,
    pub dest_host: String,
    pub dest_port: u16,
    pub connections: u64,
    pub bytes_in: u64,
    pub bytes_out: u64,
}

#[derive(Default)]
struct ForwardStats {
    connections: AtomicU64,
    bytes_in: AtomicU64,
    bytes_out: AtomicU64,
}

struct ForwardEntry {
    info: ForwardInfo,
    stats: Arc<ForwardStats>,
    shutdown: watch::Sender<bool>,
    handle: Option<Arc<client::Handle<SshHandler>>>,
}

#[derive(Default)]
pub struct ForwardRegistry {
    entries: Mutex<HashMap<String, ForwardEntry>>,
}

impl ForwardRegistry {
    pub async fn list(&self) -> Vec<ForwardInfo> {
        let entries = self.entries.lock().await;
        let mut infos: Vec<ForwardInfo> = entries
            .values()
            .map(|entry| {
                let mut info = entry.info.clone();
                info.connections = entry.stats.connections.load(Ordering::Relaxed);
                info.bytes_in = entry.stats.bytes_in.load(Ordering::Relaxed);
                info.bytes_out = entry.stats.bytes_out.load(Ordering::Relaxed);
                info
            })
            .collect();
        infos.sort_by_key(|info| (info.bind_port, info.dest_host.clone()));
        infos
    }

    pub async fn stop(&self, id: &str) -> bool {
        let entry = self.entries.lock().await.remove(id);
        let Some(entry) = entry else {
            return false;
        };
        let _ = entry.shutdown.send(true);
        if entry.info.kind == ForwardKind::Remote {
            if let Some(handle) = &entry.handle {
                let _ = handle
                    .cancel_tcpip_forward(
                        entry.info.bind_host.clone(),
                        u32::from(entry.info.bind_port),
                    )
                    .await;
            }
        }
        true
    }

    async fn route_remote(
        &self,
        session_id: &str,
        remote_port: u32,
        channel: russh::Channel<client::Msg>,
    ) -> bool {
        let route = {
            let entries = self.entries.lock().await;
            entries
                .values()
                .find(|entry| {
                    entry.info.kind == ForwardKind::Remote
                        && entry.info.session_id == session_id
                        && u32::from(entry.info.bind_port) == remote_port
                })
                .map(|entry| {
                    (
                        entry.info.dest_host.clone(),
                        entry.info.dest_port,
                        entry.stats.clone(),
                        entry.shutdown.subscribe(),
                    )
                })
        };
        let Some((dest_host, dest_port, stats, mut shutdown)) = route else {
            return false;
        };
        stats.connections.fetch_add(1, Ordering::Relaxed);
        tokio::spawn(async move {
            tokio::select! {
                _ = shutdown.changed() => {}
                _ = serve_forwarded(channel, dest_host, dest_port, stats) => {}
            }
        });
        true
    }

    pub async fn stop_session(&self, session_id: &str) {
        let mut pending = Vec::new();
        {
            let mut entries = self.entries.lock().await;
            let ids: Vec<String> = entries
                .iter()
                .filter(|(_, entry)| entry.info.session_id == session_id)
                .map(|(id, _)| id.clone())
                .collect();
            for id in ids {
                if let Some(entry) = entries.remove(&id) {
                    let _ = entry.shutdown.send(true);
                    if entry.info.kind == ForwardKind::Remote {
                        if let Some(handle) = entry.handle {
                            pending.push((handle, entry.info.bind_host, entry.info.bind_port));
                        }
                    }
                }
            }
        }
        for (handle, host, port) in pending {
            let _ = handle.cancel_tcpip_forward(host, u32::from(port)).await;
        }
    }
}

pub async fn start_forward(
    sessions: &SessionRegistry,
    forwards: &ForwardRegistry,
    request: ForwardRequest,
) -> Result<ForwardInfo, String> {
    let kind = request.kind.unwrap_or(ForwardKind::Local);
    let (dest_host, dest_port) = if kind == ForwardKind::Dynamic {
        (String::new(), 0)
    } else {
        let host = request.dest_host.trim().to_string();
        if host.is_empty() {
            return Err("destination host is required".to_string());
        }
        if request.dest_port == 0 {
            return Err("destination port is required".to_string());
        }
        (host, request.dest_port)
    };
    if kind == ForwardKind::Remote && request.bind_port == 0 {
        return Err("remote port is required".to_string());
    }
    let handle = {
        let guard = sessions.sessions.lock().await;
        match guard.get(&request.session_id) {
            Some(Session::Ssh(session)) => session.handle.clone(),
            _ => return Err("ssh session not found".to_string()),
        }
    };
    {
        let guard = forwards.entries.lock().await;
        if guard.contains_key(&request.id) {
            return Err("forward already exists".to_string());
        }
    }
    let bind_host = request
        .bind_host
        .as_deref()
        .map(str::trim)
        .filter(|host| !host.is_empty())
        .unwrap_or("127.0.0.1")
        .to_string();

    let (shutdown, shutdown_rx) = watch::channel(false);
    let stats = Arc::new(ForwardStats::default());
    let mut info = ForwardInfo {
        id: request.id.clone(),
        session_id: request.session_id.clone(),
        kind,
        bind_host,
        bind_port: request.bind_port,
        dest_host: dest_host.clone(),
        dest_port,
        connections: 0,
        bytes_in: 0,
        bytes_out: 0,
    };

    let entry_handle = if kind == ForwardKind::Remote {
        let assigned = handle
            .tcpip_forward(info.bind_host.clone(), u32::from(request.bind_port))
            .await
            .map_err(|e| format!("remote forwarding request failed: {e}"))?;
        info.bind_port =
            u16::try_from(assigned).map_err(|_| "server returned an invalid port".to_string())?;
        Some(handle)
    } else {
        let listener = TcpListener::bind((info.bind_host.as_str(), request.bind_port))
            .await
            .map_err(|e| {
                format!(
                    "failed to bind {}:{}: {e}",
                    info.bind_host, request.bind_port
                )
            })?;
        info.bind_port = listener
            .local_addr()
            .map(|addr| addr.port())
            .unwrap_or(request.bind_port);
        spawn_listener(
            listener,
            handle,
            kind,
            dest_host,
            dest_port,
            stats.clone(),
            shutdown_rx,
        );
        None
    };

    forwards.entries.lock().await.insert(
        request.id,
        ForwardEntry {
            info: info.clone(),
            stats,
            shutdown,
            handle: entry_handle,
        },
    );
    Ok(info)
}

fn spawn_listener(
    listener: TcpListener,
    handle: Arc<client::Handle<SshHandler>>,
    kind: ForwardKind,
    dest_host: String,
    dest_port: u16,
    stats: Arc<ForwardStats>,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => break,
                accepted = listener.accept() => {
                    let Ok((stream, peer)) = accepted else {
                        break;
                    };
                    stats.connections.fetch_add(1, Ordering::Relaxed);
                    let handle = handle.clone();
                    let dest_host = dest_host.clone();
                    let stats = stats.clone();
                    let mut connection_shutdown = shutdown_rx.clone();
                    tokio::spawn(async move {
                        tokio::select! {
                            _ = connection_shutdown.changed() => {}
                            _ = serve_connection(
                                handle,
                                stream,
                                peer,
                                kind,
                                dest_host,
                                dest_port,
                                stats,
                            ) => {}
                        }
                    });
                }
            }
        }
    });
}

async fn serve_connection(
    handle: Arc<client::Handle<SshHandler>>,
    mut stream: TcpStream,
    peer: SocketAddr,
    kind: ForwardKind,
    dest_host: String,
    dest_port: u16,
    stats: Arc<ForwardStats>,
) -> Result<(), String> {
    let (host, port) = match kind {
        ForwardKind::Local => (dest_host, dest_port),
        ForwardKind::Dynamic => socks5_handshake(&mut stream).await?,
        ForwardKind::Remote => {
            return Err("remote forward cannot open a local listener".to_string())
        }
    };
    let channel = handle
        .channel_open_direct_tcpip(
            host,
            u32::from(port),
            peer.ip().to_string(),
            u32::from(peer.port()),
        )
        .await;
    let channel = match channel {
        Ok(channel) => channel,
        Err(error) => {
            if kind == ForwardKind::Dynamic {
                let _ = socks5_reply(&mut stream, 0x03).await;
            }
            return Err(error.to_string());
        }
    };
    if kind == ForwardKind::Dynamic {
        socks5_reply(&mut stream, 0x00)
            .await
            .map_err(|e| e.to_string())?;
    }
    bridge(stream, channel.into_stream(), &stats).await
}

async fn bridge<T>(stream: TcpStream, remote: T, stats: &ForwardStats) -> Result<(), String>
where
    T: AsyncRead + AsyncWrite + Unpin + Send,
{
    let (mut local_read, mut local_write) = stream.into_split();
    let (mut remote_read, mut remote_write) = tokio::io::split(remote);

    let upload = copy_counting(&mut local_read, &mut remote_write, &stats.bytes_out);
    let download = copy_counting(&mut remote_read, &mut local_write, &stats.bytes_in);
    tokio::select! {
        _ = upload => {}
        _ = download => {}
    }
    Ok(())
}

async fn serve_forwarded(
    channel: russh::Channel<client::Msg>,
    dest_host: String,
    dest_port: u16,
    stats: Arc<ForwardStats>,
) -> Result<(), String> {
    let stream = TcpStream::connect((dest_host.as_str(), dest_port))
        .await
        .map_err(|e| format!("local target unreachable: {e}"))?;
    bridge(stream, channel.into_stream(), &stats).await
}

pub struct ForwardRouter {
    registry: Arc<ForwardRegistry>,
    session_id: String,
}

impl ForwardRouter {
    pub fn new(registry: Arc<ForwardRegistry>, session_id: impl Into<String>) -> Self {
        Self {
            registry,
            session_id: session_id.into(),
        }
    }
}

impl RemoteForwardHandler for ForwardRouter {
    fn forwarded_channel<'a>(
        &'a self,
        _connected_address: &'a str,
        connected_port: u32,
        channel: russh::Channel<client::Msg>,
    ) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move {
            self.registry
                .route_remote(&self.session_id, connected_port, channel)
                .await
        })
    }
}

async fn socks5_handshake<S>(stream: &mut S) -> Result<(String, u16), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut greeting = [0u8; 2];
    stream
        .read_exact(&mut greeting)
        .await
        .map_err(|e| format!("SOCKS5 handshake failed: {e}"))?;
    if greeting[0] != 0x05 {
        return Err("unsupported SOCKS version".to_string());
    }
    let mut methods = vec![0u8; usize::from(greeting[1])];
    stream
        .read_exact(&mut methods)
        .await
        .map_err(|e| format!("SOCKS5 handshake failed: {e}"))?;
    if !methods.contains(&0x00) {
        let _ = stream.write_all(&[0x05, 0xFF]).await;
        return Err("SOCKS5 client did not offer no-auth".to_string());
    }
    stream
        .write_all(&[0x05, 0x00])
        .await
        .map_err(|e| format!("SOCKS5 handshake failed: {e}"))?;

    let mut request = [0u8; 4];
    stream
        .read_exact(&mut request)
        .await
        .map_err(|e| format!("SOCKS5 request failed: {e}"))?;
    if request[0] != 0x05 {
        return Err("unsupported SOCKS version".to_string());
    }
    if request[1] != 0x01 {
        let _ = socks5_reply(stream, 0x07).await;
        return Err("only SOCKS5 CONNECT is supported".to_string());
    }
    let host = match request[3] {
        0x01 => {
            let mut octets = [0u8; 4];
            stream
                .read_exact(&mut octets)
                .await
                .map_err(|e| format!("SOCKS5 request failed: {e}"))?;
            std::net::Ipv4Addr::from(octets).to_string()
        }
        0x03 => {
            let mut length = [0u8; 1];
            stream
                .read_exact(&mut length)
                .await
                .map_err(|e| format!("SOCKS5 request failed: {e}"))?;
            if length[0] == 0 {
                let _ = socks5_reply(stream, 0x08).await;
                return Err("empty SOCKS5 domain".to_string());
            }
            let mut domain = vec![0u8; usize::from(length[0])];
            stream
                .read_exact(&mut domain)
                .await
                .map_err(|e| format!("SOCKS5 request failed: {e}"))?;
            String::from_utf8(domain).map_err(|_| "invalid SOCKS5 domain".to_string())?
        }
        0x04 => {
            let mut octets = [0u8; 16];
            stream
                .read_exact(&mut octets)
                .await
                .map_err(|e| format!("SOCKS5 request failed: {e}"))?;
            std::net::Ipv6Addr::from(octets).to_string()
        }
        _ => {
            let _ = socks5_reply(stream, 0x08).await;
            return Err("unsupported SOCKS5 address type".to_string());
        }
    };
    let mut port = [0u8; 2];
    stream
        .read_exact(&mut port)
        .await
        .map_err(|e| format!("SOCKS5 request failed: {e}"))?;
    Ok((host, u16::from_be_bytes(port)))
}

async fn socks5_reply<S>(stream: &mut S, status: u8) -> std::io::Result<()>
where
    S: AsyncWrite + Unpin,
{
    stream
        .write_all(&[0x05, status, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
        .await
}

async fn copy_counting<R, W>(
    reader: &mut R,
    writer: &mut W,
    counter: &AtomicU64,
) -> std::io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buffer = [0u8; 16 * 1024];
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        writer.write_all(&buffer[..read]).await?;
        counter.fetch_add(read as u64, Ordering::Relaxed);
    }
    let _ = writer.shutdown().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::socks5_handshake;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn expect_method_selection(client: &mut tokio::io::DuplexStream) {
        let mut reply = [0u8; 2];
        client.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply, [0x05, 0x00]);
    }

    #[tokio::test]
    async fn socks5_handshake_accepts_domain_target() {
        let (mut client, mut server) = tokio::io::duplex(64);
        let handshake = tokio::spawn(async move { socks5_handshake(&mut server).await });
        client.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        expect_method_selection(&mut client).await;

        let domain = b"db.internal";
        let mut request = vec![0x05, 0x01, 0x00, 0x03, domain.len() as u8];
        request.extend_from_slice(domain);
        request.extend_from_slice(&5432u16.to_be_bytes());
        client.write_all(&request).await.unwrap();

        let target = handshake.await.unwrap().unwrap();
        assert_eq!(target, ("db.internal".to_string(), 5432));
    }

    #[tokio::test]
    async fn socks5_handshake_reads_ipv4_target() {
        let (mut client, mut server) = tokio::io::duplex(64);
        let handshake = tokio::spawn(async move { socks5_handshake(&mut server).await });
        client.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        expect_method_selection(&mut client).await;

        client
            .write_all(&[0x05, 0x01, 0x00, 0x01, 127, 0, 0, 1, 0x1F, 0x90])
            .await
            .unwrap();

        let target = handshake.await.unwrap().unwrap();
        assert_eq!(target, ("127.0.0.1".to_string(), 8080));
    }
}
