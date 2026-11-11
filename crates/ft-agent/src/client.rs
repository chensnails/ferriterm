//! Client half of the service, shared by `ferriterm-cli` and anything else
//! that wants to drive the app.

use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::base64;
use crate::protocol::{
    CheckReport, CheckRequest, ErrorBody, ExecReport, ExecRequest, FileBlob, Health, HostSummary,
    PathRequest, SessionSummary, UploadReport, UploadRequest,
};
use crate::server::MAX_BODY;
use crate::Endpoint;

/// How long a connect may take. Loopback, so this is generous already.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Default ceiling for a whole exchange; `exec` raises it to the command
/// timeout the caller asked for.
const CALL_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug)]
pub struct ClientError {
    /// HTTP status, or 0 when the connection itself failed.
    pub status: u16,
    pub kind: String,
    pub message: String,
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl std::error::Error for ClientError {}

impl ClientError {
    pub fn is_auth(&self) -> bool {
        self.status == 401 || self.kind == "auth"
    }

    fn transport(message: impl Into<String>) -> Self {
        Self {
            status: 0,
            kind: "transport".to_string(),
            message: message.into(),
        }
    }
}

pub struct Client {
    addr: std::net::SocketAddr,
    token: String,
}

impl Client {
    pub fn new(endpoint: &Endpoint) -> Self {
        Self {
            addr: crate::server::loopback(endpoint.port),
            token: endpoint.token.clone(),
        }
    }

    /// Rewrites the auth failure into something a person can act on: the usual
    /// cause is an endpoint file left behind by an app that has since
    /// restarted, which mints a new token every launch.
    fn auth_hint(error: ClientError) -> ClientError {
        if !error.is_auth() {
            return error;
        }
        ClientError {
            message: format!(
                "{} — the app may have restarted since the endpoint file was written; check `ferriterm-cli status`",
                error.message
            ),
            ..error
        }
    }

    pub async fn health(&self) -> Result<Health, ClientError> {
        self.call("GET", "/v1/health", None::<&()>, CALL_TIMEOUT)
            .await
    }

    pub async fn hosts(&self) -> Result<Vec<HostSummary>, ClientError> {
        self.call("GET", "/v1/hosts", None::<&()>, CALL_TIMEOUT)
            .await
    }

    pub async fn sessions(&self) -> Result<Vec<SessionSummary>, ClientError> {
        self.call("GET", "/v1/sessions", None::<&()>, CALL_TIMEOUT)
            .await
    }

    pub async fn check(&self, request: &CheckRequest) -> Result<CheckReport, ClientError> {
        self.call("POST", "/v1/check", Some(request), CALL_TIMEOUT)
            .await
    }

    /// Runs a command through the gate. The client waits for the command, so
    /// the ceiling follows the command timeout with room to spare.
    pub async fn exec(&self, request: &ExecRequest) -> Result<ExecReport, ClientError> {
        let budget = request.timeout_ms.unwrap_or(60_000).min(600_000);
        self.call(
            "POST",
            "/v1/exec",
            Some(request),
            Duration::from_millis(budget) + Duration::from_secs(15),
        )
        .await
    }

    pub async fn download(&self, session: &str, path: &str) -> Result<FileBlob, ClientError> {
        let request = PathRequest {
            session: session.to_string(),
            path: path.to_string(),
        };
        self.call("POST", "/v1/download", Some(&request), CALL_TIMEOUT)
            .await
    }

    /// Whole file at once, encoded on the way out. A file too big for one
    /// request is refused here instead of being truncated silently.
    pub async fn upload(
        &self,
        session: &str,
        path: &str,
        content: &[u8],
    ) -> Result<UploadReport, ClientError> {
        if content.len() > MAX_BODY / 4 * 3 {
            return Err(ClientError {
                status: 413,
                kind: "request".to_string(),
                message: format!(
                    "file is {} bytes; the agent accepts up to {} in one upload",
                    content.len(),
                    MAX_BODY / 4 * 3
                ),
            });
        }
        let request = UploadRequest {
            session: session.to_string(),
            path: path.to_string(),
            content_base64: base64::encode(content),
        };
        self.call("POST", "/v1/upload", Some(&request), CALL_TIMEOUT)
            .await
    }

    pub(crate) async fn call<B, T>(
        &self,
        method: &str,
        path: &str,
        body: Option<&B>,
        timeout: Duration,
    ) -> Result<T, ClientError>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        let payload = match body {
            Some(value) => serde_json::to_vec(value)
                .map_err(|err| ClientError::transport(format!("cannot encode request: {err}")))?,
            None => Vec::new(),
        };

        let mut request = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.token,
            payload.len()
        )
        .into_bytes();
        request.extend_from_slice(&payload);

        let exchange =
            async {
                let mut stream = TcpStream::connect(self.addr).await.map_err(|err| {
                    ClientError::transport(format!("cannot reach the app: {err}"))
                })?;
                stream.write_all(&request).await.map_err(|err| {
                    ClientError::transport(format!("cannot send the request: {err}"))
                })?;
                stream.flush().await.map_err(|err| {
                    ClientError::transport(format!("cannot send the request: {err}"))
                })?;

                let mut response = Vec::new();
                let mut chunk = [0u8; 8192];
                loop {
                    let read = stream.read(&mut chunk).await.map_err(|err| {
                        ClientError::transport(format!("cannot read the answer: {err}"))
                    })?;
                    if read == 0 {
                        break;
                    }
                    response.extend_from_slice(&chunk[..read]);
                    if response.len() > MAX_BODY * 2 {
                        return Err(ClientError::transport(
                            "the answer is larger than this client accepts",
                        ));
                    }
                }
                Ok::<Vec<u8>, ClientError>(response)
            };

        let response = match tokio::time::timeout(timeout, exchange).await {
            Ok(result) => result?,
            Err(_) => {
                return Err(ClientError::transport(format!(
                    "the app did not answer within {}s",
                    timeout.as_secs()
                )))
            }
        };

        self.interpret(&response)
    }

    fn interpret<T: DeserializeOwned>(&self, response: &[u8]) -> Result<T, ClientError> {
        let split = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .ok_or_else(|| ClientError::transport("the answer had no header block"))?;
        let head = String::from_utf8_lossy(&response[..split]).into_owned();
        let body = &response[split + 4..];

        let status = head
            .lines()
            .next()
            .and_then(|line| line.split(' ').nth(1))
            .and_then(|code| code.parse::<u16>().ok())
            .ok_or_else(|| ClientError::transport("the answer had no status line"))?;

        if status != 200 {
            let error: ErrorBody = serde_json::from_slice(body).unwrap_or_else(|_| ErrorBody {
                error: String::from_utf8_lossy(body).trim().to_string(),
                kind: "unknown".to_string(),
            });
            return Err(Self::auth_hint(ClientError {
                status,
                kind: error.kind,
                message: error.error,
            }));
        }

        serde_json::from_slice(body)
            .map_err(|err| ClientError::transport(format!("cannot read the answer: {err}")))
    }
}

/// Dial `port` once, for the CLI's `status` command before it has a token.
pub async fn reachable(port: u16) -> bool {
    matches!(
        tokio::time::timeout(
            CONNECT_TIMEOUT,
            TcpStream::connect(crate::server::loopback(port))
        )
        .await,
        Ok(Ok(_))
    )
}

#[cfg(test)]
mod tests;
