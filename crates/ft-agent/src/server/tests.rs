use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use super::*;
use crate::client::Client;
use crate::protocol::*;

/// A backend that answers without a window, an SSH server or a database, so
/// these tests cover the HTTP layer and nothing else.
#[derive(Default)]
struct Mock {
    /// Everything the tested layer must have forwarded, in order.
    seen: std::sync::Mutex<Vec<String>>,
}

impl Mock {
    fn record(&self, what: impl Into<String>) {
        self.seen.lock().unwrap().push(what.into());
    }
}

impl Backend for Mock {
    fn health(&self) -> BackendFuture<'_, Health> {
        Box::pin(async {
            Ok(Health {
                version: String::new(),
                pid: 4242,
                sessions: 1,
                uptime_secs: 0,
                ready: true,
            })
        })
    }

    fn hosts(&self) -> BackendFuture<'_, Vec<HostSummary>> {
        Box::pin(async {
            Ok(vec![HostSummary {
                id: "h1".to_string(),
                label: "web-1".to_string(),
                host: "10.0.0.5".to_string(),
                port: 22,
                username: "deploy".to_string(),
                protocol: "ssh".to_string(),
                production: true,
                session: Some("s1".to_string()),
            }])
        })
    }

    fn sessions(&self) -> BackendFuture<'_, Vec<SessionSummary>> {
        Box::pin(async {
            Ok(vec![SessionSummary {
                id: "s1".to_string(),
                label: "web-1".to_string(),
                host: "10.0.0.5".to_string(),
                username: "deploy".to_string(),
                kind: "ssh".to_string(),
            }])
        })
    }

    fn check(&self, request: CheckRequest) -> BackendFuture<'_, CheckReport> {
        self.record(format!("check:{}", request.command));
        Box::pin(async move {
            Ok(CheckReport {
                command: request.command,
                host: request.host.unwrap_or_default(),
                verdict: "ask".to_string(),
                risk: "r2".to_string(),
                reasons: vec!["modifiesFiles".to_string()],
                rule: None,
                mode: "ask".to_string(),
            })
        })
    }

    fn exec(&self, request: ExecRequest) -> BackendFuture<'_, ExecReport> {
        self.record(format!("exec:{}", request.command));
        Box::pin(async move {
            if request.session != "s1" {
                return Err(format!("no session {}", request.session));
            }
            let ran = request.command == "uptime";
            Ok(ExecReport {
                verdict: if ran { "allow".to_string() } else { "deny".to_string() },
                risk: if ran { "r0".to_string() } else { "r4".to_string() },
                reasons: if ran { Vec::new() } else { vec!["recursiveDelete".to_string()] },
                rule: None,
                mode: "auto".to_string(),
                ran,
                output: ran.then(|| " 18:52 up 3 days".to_string()),
                truncated: false,
                duration_ms: 7,
            })
        })
    }

    fn read_file(&self, request: PathRequest) -> BackendFuture<'_, FileBlob> {
        self.record(format!("read:{}", request.path));
        Box::pin(async move {
            let content = b"hello\n";
            Ok(FileBlob {
                path: request.path,
                size: content.len() as u64,
                content_base64: crate::base64::encode(content),
            })
        })
    }

    fn write_file(&self, request: UploadRequest) -> BackendFuture<'_, UploadReport> {
        self.record(format!("write:{}", request.path));
        Box::pin(async move {
            let bytes = crate::base64::decode(&request.content_base64)?;
            Ok(UploadReport {
                path: request.path,
                written: bytes.len() as u64,
            })
        })
    }
}

async fn start(token: &str) -> (Arc<Mock>, Endpoint) {
    let (_agent, listener) = Agent::bind().expect("bind");
    let port = listener.local_addr().unwrap().port();
    let backend = Arc::new(Mock::default());
    let serving = backend.clone();
    tokio::spawn(serve(
        listener,
        serving as Arc<dyn Backend>,
        token.to_string(),
        "9.9.9".to_string(),
        std::future::pending::<()>(),
    ));
    // The task above must be polled once before accept works.
    tokio::task::yield_now().await;
    (
        backend,
        Endpoint {
            version: "9.9.9".to_string(),
            port,
            token: token.to_string(),
            pid: 1,
        },
    )
}

#[tokio::test]
async fn rebinding_keeps_the_token_and_prefers_the_old_port() {
    let (first, listener) = Agent::bind().expect("bind");
    let port = first.port;
    drop(listener);
    let (again, listener) = Agent::rebind(port, "kept".to_string()).expect("rebind");
    assert_eq!(again.token, "kept");
    assert_eq!(again.port, port, "a free port is reused, not replaced");

    // A port somebody else grabbed meanwhile is not fatal: serving moves to a
    // fresh one and the token still stands.
    let (held, _held_listener) = Agent::bind().expect("bind");
    let held_port = held.port;
    let (moved, _listener) = Agent::rebind(held_port, "kept".to_string()).expect("rebind");
    assert_eq!(moved.token, "kept");
    assert_ne!(moved.port, held_port);
}

#[tokio::test]
async fn health_carries_the_service_version() {
    let (_backend, endpoint) = start("secret").await;
    let health = Client::new(&endpoint).health().await.unwrap();
    assert_eq!(health.version, "9.9.9");
    assert_eq!(health.pid, 4242);
    assert!(health.ready);
}

#[tokio::test]
async fn a_wrong_token_is_refused_with_a_hint() {
    let (_backend, endpoint) = start("secret").await;
    let wrong = Endpoint {
        token: "not-the-token".to_string(),
        ..endpoint
    };
    let error = Client::new(&wrong).health().await.unwrap_err();
    assert_eq!(error.status, 401);
    assert!(error.is_auth());
    // The hint exists because a restarted app mints a new token.
    assert!(error.message.contains("restarted"), "{}", error.message);
}

#[tokio::test]
async fn a_token_is_checked_in_constant_time() {
    assert!(token_matches("abc", "abc"));
    assert!(!token_matches("abc", "abd"));
    assert!(!token_matches("ab", "abc"));
    assert!(!token_matches("", "abc"));
}

#[tokio::test]
async fn hosts_and_sessions_come_back_as_json() {
    let (_backend, endpoint) = start("secret").await;
    let client = Client::new(&endpoint);
    let hosts = client.hosts().await.unwrap();
    assert_eq!(hosts.len(), 1);
    assert_eq!(hosts[0].label, "web-1");
    assert!(hosts[0].production);
    assert_eq!(hosts[0].session.as_deref(), Some("s1"));

    let sessions = client.sessions().await.unwrap();
    assert_eq!(sessions[0].kind, "ssh");
}

#[tokio::test]
async fn exec_returns_the_gate_report_verbatim() {
    let (backend, endpoint) = start("secret").await;
    let client = Client::new(&endpoint);

    let allowed = client
        .exec(&ExecRequest {
            session: "s1".to_string(),
            command: "uptime".to_string(),
            timeout_ms: None,
            origin: None,
        })
        .await
        .unwrap();
    assert!(allowed.ran);
    assert_eq!(allowed.verdict, "allow");
    assert_eq!(allowed.output.as_deref(), Some(" 18:52 up 3 days"));

    // A refusal is a normal answer, not a transport error: the CLI needs to
    // tell "the gate said no" apart from "the app could not be reached".
    let refused = client
        .exec(&ExecRequest {
            session: "s1".to_string(),
            command: "rm -rf /".to_string(),
            timeout_ms: Some(1000),
            origin: None,
        })
        .await
        .unwrap();
    assert!(!refused.ran);
    assert_eq!(refused.verdict, "deny");
    assert_eq!(refused.reasons, vec!["recursiveDelete".to_string()]);
    assert_eq!(backend.seen.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn a_missing_session_is_an_error_not_an_empty_report() {
    let (_backend, endpoint) = start("secret").await;
    let error = Client::new(&endpoint)
        .exec(&ExecRequest {
            session: "gone".to_string(),
            command: "uptime".to_string(),
            timeout_ms: None,
            origin: None,
        })
        .await
        .unwrap_err();
    assert_eq!(error.status, 409);
    assert!(error.message.contains("no session gone"), "{}", error.message);
}

#[tokio::test]
async fn files_round_trip_through_base64() {
    let (_backend, endpoint) = start("secret").await;
    let client = Client::new(&endpoint);

    let blob = client.download("s1", "/etc/hostname").await.unwrap();
    assert_eq!(blob.size, 6);
    assert_eq!(blob.content_base64, crate::base64::encode(b"hello\n"));

    let report = client
        .upload("s1", "/tmp/note.txt", b"hello\n")
        .await
        .unwrap();
    assert_eq!(report.written, 6);
    assert_eq!(report.path, "/tmp/note.txt");
}

#[tokio::test]
async fn check_grades_without_running() {
    let (backend, endpoint) = start("secret").await;
    let report = Client::new(&endpoint)
        .check(&CheckRequest {
            command: "rm note.txt".to_string(),
            session: Some("s1".to_string()),
            host: None,
        })
        .await
        .unwrap();
    assert_eq!(report.verdict, "ask");
    assert_eq!(report.risk, "r2");
    assert!(backend
        .seen
        .lock()
        .unwrap()
        .iter()
        .all(|call| call.starts_with("check:")));
}

#[tokio::test]
async fn unknown_routes_are_not_found() {
    let (_backend, endpoint) = start("secret").await;
    let error = Client::new(&endpoint)
        .call::<(), serde_json::Value>("GET", "/v1/nope", None::<&()>, Duration::from_secs(5))
        .await
        .unwrap_err();
    assert_eq!(error.status, 404);
}

#[tokio::test]
async fn a_body_over_the_cap_is_refused_before_it_is_read() {
    let (_backend, endpoint) = start("secret").await;
    let mut stream = TcpStream::connect(crate::server::loopback(endpoint.port))
        .await
        .unwrap();
    let head = format!(
        "POST /v1/exec HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer secret\r\nContent-Length: {}\r\n\r\n",
        MAX_BODY + 1
    );
    stream.write_all(head.as_bytes()).await.unwrap();
    let mut answer = Vec::new();
    let mut chunk = [0u8; 512];
    let read = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut chunk))
        .await
        .expect("the service must answer without waiting for the body")
        .unwrap();
    answer.extend_from_slice(&chunk[..read]);
    let text = String::from_utf8_lossy(&answer);
    assert!(text.starts_with("HTTP/1.1 413"), "{text}");
}

#[tokio::test]
async fn a_malformed_body_is_a_request_error() {
    let (_backend, endpoint) = start("secret").await;
    let mut stream = TcpStream::connect(crate::server::loopback(endpoint.port))
        .await
        .unwrap();
    let body = b"not json";
    let head = format!(
        "POST /v1/exec HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer secret\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await.unwrap();
    stream.write_all(body).await.unwrap();
    let mut answer = Vec::new();
    let mut chunk = [0u8; 512];
    let read = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut chunk))
        .await
        .unwrap()
        .unwrap();
    answer.extend_from_slice(&chunk[..read]);
    let text = String::from_utf8_lossy(&answer);
    assert!(text.starts_with("HTTP/1.1 400"), "{text}");
    assert!(text.contains("bad JSON body"), "{text}");
}
