//! The loopback service.
//!
//! One request per connection, `Connection: close`, JSON in and out. It is not
//! a general web server on purpose: the only clients are this project's own
//! CLI and, later, the agents we wire up, so the grammar stays small enough to
//! read in one sitting and every route below is auth-gated.
//!
//! Nothing here decides whether a command is safe. The gate lives in
//! `ft-policy` and is consulted by the [`Backend`] implementation; this layer
//! only carries the answer back.

use std::future::Future;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::protocol::{
    CheckReport, CheckRequest, ErrorBody, ExecReport, ExecRequest, FileBlob, Health, HostSummary,
    PathRequest, SessionSummary, UploadReport, UploadRequest,
};
use crate::Endpoint;

/// Largest request head we accept. The routes only send a few headers, so
/// anything bigger is a mistake or an attack.
const MAX_HEAD: usize = 64 * 1024;
/// Largest body we accept. Uploads come through this path, so it has to hold a
/// file; the CLI refuses bigger ones before they get here.
pub const MAX_BODY: usize = 32 * 1024 * 1024;
/// How long a client gets to send its request before the connection is dropped.
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Future returned by every backend call. Boxed rather than written as an
/// `async fn` in the trait so the trait stays object safe, the same shape
/// `ft-vfs` uses.
pub type BackendFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, String>> + Send + 'a>>;

/// What the service can do, implemented by the application.
///
/// Keeping this a trait is what lets the HTTP layer be tested without a
/// window, an SSH connection or a settings database.
pub trait Backend: Send + Sync {
    fn health(&self) -> BackendFuture<'_, Health>;
    fn hosts(&self) -> BackendFuture<'_, Vec<HostSummary>>;
    fn sessions(&self) -> BackendFuture<'_, Vec<SessionSummary>>;
    /// Grade a command without running it.
    fn check(&self, request: CheckRequest) -> BackendFuture<'_, CheckReport>;
    /// Grade a command and run it when the gate says allow.
    fn exec(&self, request: ExecRequest) -> BackendFuture<'_, ExecReport>;
    fn read_file(&self, request: PathRequest) -> BackendFuture<'_, FileBlob>;
    fn write_file(&self, request: UploadRequest) -> BackendFuture<'_, UploadReport>;
}

/// A running service: the port it settled on and the token clients need.
pub struct Agent {
    pub port: u16,
    pub token: String,
}

impl Agent {
    /// Binds `127.0.0.1:0` and hands back the port the kernel picked, so two
    /// copies of the app never fight over a fixed number and a local process
    /// cannot find the port by guessing.
    pub fn bind() -> std::io::Result<(Self, StdListener)> {
        let listener = StdListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        Ok((
            Self {
                port,
                token: crate::random_token(),
            },
            listener,
        ))
    }

    /// Endpoint description, ready to be written next to the settings file.
    pub fn endpoint(&self, version: &str) -> Endpoint {
        Endpoint {
            version: version.to_string(),
            port: self.port,
            token: self.token.clone(),
            pid: std::process::id(),
        }
    }
}

/// Serves until the future `shutdown` completes or the process exits.
pub async fn serve(
    listener: StdListener,
    backend: Arc<dyn Backend>,
    token: String,
    version: String,
    shutdown: impl Future<Output = ()>,
) -> std::io::Result<()> {
    let listener = TcpListener::from_std(listener)?;
    let shared = Arc::new(Shared {
        backend,
        token: Arc::new(token),
        version,
        started: Instant::now(),
    });

    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            _ = &mut shutdown => return Ok(()),
            accepted = listener.accept() => {
                let (stream, _peer) = match accepted {
                    Ok(value) => value,
                    // One refused connection is not a reason to stop serving.
                    Err(err) if is_transient(&err) => continue,
                    Err(err) => return Err(err),
                };
                let shared = shared.clone();
                tokio::spawn(async move {
                    if let Err(err) = handle(stream, shared).await {
                        tracing::debug!(error = %err, "agent connection ended early");
                    }
                });
            }
        }
    }
}

fn is_transient(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::Interrupted
    )
}

struct Shared {
    backend: Arc<dyn Backend>,
    token: Arc<String>,
    version: String,
    started: Instant,
}

struct Request {
    method: String,
    path: String,
    authorization: Option<String>,
    body: Vec<u8>,
}

async fn handle(mut stream: TcpStream, shared: Arc<Shared>) -> std::io::Result<()> {
    let request = match read_request(&mut stream).await {
        Ok(request) => request,
        Err(problem) => {
            let body = ErrorBody {
                error: problem.error,
                kind: problem.kind.to_string(),
            };
            return reply(&mut stream, problem.status, &body).await;
        }
    };

    let presented = request.authorization.as_deref().unwrap_or_default();
    if !token_matches(presented, &shared.token) {
        let body = ErrorBody {
            error: "the endpoint token is missing or wrong".to_string(),
            kind: "auth".to_string(),
        };
        return reply(&mut stream, 401, &body).await;
    }

    match route(&request, &shared).await {
        Ok(value) => reply(&mut stream, 200, &value).await,
        Err(problem) => {
            let body = ErrorBody {
                error: problem.error,
                kind: problem.kind.to_string(),
            };
            reply(&mut stream, problem.status, &body).await
        }
    }
}

async fn route(
    request: &Request,
    shared: &Shared,
) -> Result<serde_json::Value, Problem> {
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/v1/health") => {
            let mut health = shared.backend.health().await.map_err(Problem::backend)?;
            health.version = shared.version.clone();
            health.uptime_secs = shared.started.elapsed().as_secs();
            encode(health)
        }
        ("GET", "/v1/hosts") => encode(shared.backend.hosts().await.map_err(Problem::backend)?),
        ("GET", "/v1/sessions") => {
            encode(shared.backend.sessions().await.map_err(Problem::backend)?)
        }
        ("POST", "/v1/check") => {
            let body: CheckRequest = parse(&request.body)?;
            encode(shared.backend.check(body).await.map_err(Problem::backend)?)
        }
        ("POST", "/v1/exec") => {
            let body: ExecRequest = parse(&request.body)?;
            encode(shared.backend.exec(body).await.map_err(Problem::backend)?)
        }
        ("POST", "/v1/download") => {
            let body: PathRequest = parse(&request.body)?;
            encode(
                shared
                    .backend
                    .read_file(body)
                    .await
                    .map_err(Problem::backend)?,
            )
        }
        ("POST", "/v1/upload") => {
            let body: UploadRequest = parse(&request.body)?;
            encode(
                shared
                    .backend
                    .write_file(body)
                    .await
                    .map_err(Problem::backend)?,
            )
        }
        ("GET", _) | ("POST", _) => Err(Problem::missing("unknown route")),
        _ => Err(Problem::missing("unknown method")),
    }
}

fn encode<T: serde::Serialize>(value: T) -> Result<serde_json::Value, Problem> {
    serde_json::to_value(value).map_err(|err| Problem::backend(err.to_string()))
}

fn parse<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T, Problem> {
    serde_json::from_slice(body).map_err(|err| Problem::request(format!("bad JSON body: {err}")))
}

struct Problem {
    status: u16,
    kind: &'static str,
    error: String,
}

impl Problem {
    fn request(message: impl Into<String>) -> Self {
        Self {
            status: 400,
            kind: "request",
            error: message.into(),
        }
    }

    fn missing(message: impl Into<String>) -> Self {
        Self {
            status: 404,
            kind: "missing",
            error: message.into(),
        }
    }

    /// A backend message is usually "session not found", which is the caller's
    /// problem rather than a service fault, so it comes back as 409 and the
    /// text the CLI prints is the one the backend wrote.
    fn backend(message: String) -> Self {
        Self {
            status: 409,
            kind: "backend",
            error: message,
        }
    }
}

async fn read_request(stream: &mut TcpStream) -> Result<Request, Problem> {
    let mut head = Vec::new();
    let mut chunk = [0u8; 4096];
    let end = tokio::time::timeout(READ_TIMEOUT, async {
        loop {
            let read = stream
                .read(&mut chunk)
                .await
                .map_err(|err| Problem::request(format!("read failed: {err}")))?;
            if read == 0 {
                return Err(Problem::request("connection closed before headers"));
            }
            head.extend_from_slice(&chunk[..read]);
            if let Some(at) = find_head_end(&head) {
                return Ok(at);
            }
            if head.len() > MAX_HEAD {
                return Err(Problem::request("request head too large"));
            }
        }
    })
    .await
    .map_err(|_| Problem::request("timed out waiting for the request"))??;

    let text = String::from_utf8_lossy(&head[..end]).into_owned();
    let mut lines = text.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| Problem::request("missing request line"))?;
    let mut parts = request_line.split(' ');
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();
    if method.is_empty() || path.is_empty() {
        return Err(Problem::request("malformed request line"));
    }

    let mut length = 0usize;
    let mut authorization = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if name.eq_ignore_ascii_case("content-length") {
            length = value
                .parse::<usize>()
                .map_err(|_| Problem::request("bad content-length"))?;
        } else if name.eq_ignore_ascii_case("authorization") {
            authorization = value
                .strip_prefix("Bearer ")
                .map(|token| token.to_string());
        }
    }
    if length > MAX_BODY {
        return Err(Problem {
            status: 413,
            kind: "request",
            error: format!("body larger than {MAX_BODY} bytes"),
        });
    }

    let mut body = head[end + 4..].to_vec();
    while body.len() < length {
        let read = stream
            .read(&mut chunk)
            .await
            .map_err(|err| Problem::request(format!("read failed: {err}")))?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read]);
    }
    body.truncate(length);

    Ok(Request {
        method,
        path,
        authorization,
        body,
    })
}

fn find_head_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

async fn reply<T: serde::Serialize>(
    stream: &mut TcpStream,
    status: u16,
    body: &T,
) -> std::io::Result<()> {
    let payload = serde_json::to_vec(body).unwrap_or_else(|_| b"{}".to_vec());
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        status,
        reason(status),
        payload.len()
    )
    .into_bytes();
    head.extend_from_slice(&payload);
    stream.write_all(&head).await?;
    stream.flush().await?;
    let _ = stream.shutdown().await;
    Ok(())
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        409 => "Conflict",
        413 => "Payload Too Large",
        _ => "Error",
    }
}

/// Constant-time compare, so a wrong token cannot be discovered a byte at a
/// time by timing the answer.
pub fn token_matches(candidate: &str, expected: &str) -> bool {
    let candidate = candidate.as_bytes();
    let expected = expected.as_bytes();
    if candidate.len() != expected.len() {
        return false;
    }
    let mut diff = 0u8;
    for index in 0..candidate.len() {
        diff |= candidate[index] ^ expected[index];
    }
    diff == 0
}

/// The address a client dials for a given port.
pub fn loopback(port: u16) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], port))
}

#[cfg(test)]
mod tests;
