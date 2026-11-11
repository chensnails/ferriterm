//! The application's side of the local control service.
//!
//! `ft-agent` owns the socket, the token and the JSON; everything that needs
//! the running app — the session registry, the settings database, the policy,
//! the audit log, SFTP — is reached through the backend implemented here.
//!
//! The one rule worth stating out loud: `exec` is graded by the same engine and
//! recorded in the same chain as the assistant's run button. A command that
//! arrives from an agent is not a special case, which is what makes the audit
//! log worth reading.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use ft_agent::protocol::{
    CheckReport, CheckRequest, ExecReport, ExecRequest, FileBlob, Health, HostSummary,
    PathRequest, SessionSummary, UploadReport, UploadRequest,
};
use ft_agent::server::{Backend, BackendFuture};
use ft_core::SessionRegistry;
use ft_policy::{Choice, Verdict};
use ft_vfs::{OpenMode, Vfs};

use crate::commands::SessionLabels;
use crate::policy::{self, AuditInput};

/// Ceiling on what one `exec` hands back. A stray `cat` of a database should
/// cost a bounded amount of memory.
const EXEC_CAP: usize = 256 * 1024;
/// Ceiling on a file moved through the service in either direction.
const FILE_CAP: u64 = 24 * 1024 * 1024;
/// Longest a remote command may run, whatever the client asks for.
const MAX_TIMEOUT_MS: u64 = 600_000;

/// Path of the endpoint file this process wrote, so it can be removed on the
/// way out instead of leaving a token behind.
static ENDPOINT_FILE: OnceLock<PathBuf> = OnceLock::new();

struct AgentBackend {
    sessions: SessionRegistry,
    labels: Arc<SessionLabels>,
    store: Arc<ft_store::Store>,
    sftp: Arc<ft_vfs::SftpRegistry>,
    audit: ft_policy::AuditLog,
}

impl AgentBackend {
    fn describe(&self, session: &str) -> Result<crate::commands::SessionLabel, String> {
        self.labels
            .get(session)
            .ok_or_else(|| format!("no open session {session}"))
    }

    /// Grades one command. Both `check` and `exec` go through here, so a dry
    /// run and a real one can never disagree.
    fn grade(&self, command: &str, host: &str) -> Result<ft_policy::Decision, String> {
        policy::decide(&self.store, command, host, false)
    }

    fn record(
        &self,
        command: &str,
        host: &str,
        decision: &ft_policy::Decision,
        choice: Choice,
    ) {
        // Best effort on purpose: a refused command must still reach the
        // caller even if the disk is full, and the failure is logged there.
        let entry = AuditInput {
            command: command.to_string(),
            host: host.to_string(),
            decision: decision.clone(),
            choice,
        };
        if let Err(err) = policy::record_decision(&self.audit, entry) {
            tracing::warn!(error = %err, "could not append to the audit chain");
        }
    }
}

impl Backend for AgentBackend {
    fn health(&self) -> BackendFuture<'_, Health> {
        Box::pin(async {
            Ok(Health {
                version: env!("CARGO_PKG_VERSION").to_string(),
                pid: std::process::id(),
                sessions: self.labels.list().len(),
                uptime_secs: 0,
                ready: true,
            })
        })
    }

    fn hosts(&self) -> BackendFuture<'_, Vec<HostSummary>> {
        Box::pin(async move {
            let hosts = self.store.list_hosts().map_err(|err| err.to_string())?;
            let live = self.labels.list();
            let mut out = Vec::new();
            for host in hosts {
                if host.is_folder {
                    continue;
                }
                let session = live
                    .iter()
                    .find(|(_, label)| {
                        label.host.eq_ignore_ascii_case(&host.host)
                            && label.port == host.port
                            && label.username == host.username
                    })
                    .map(|(id, _)| id.clone());
                out.push(HostSummary {
                    id: host.id,
                    label: host.name,
                    host: host.host,
                    port: host.port,
                    username: host.username,
                    protocol: host.protocol,
                    production: host.production,
                    session,
                });
            }
            Ok(out)
        })
    }

    fn sessions(&self) -> BackendFuture<'_, Vec<SessionSummary>> {
        Box::pin(async move {
            Ok(self
                .labels
                .list()
                .into_iter()
                .map(|(id, label)| SessionSummary {
                    id,
                    label: label.label,
                    host: label.host,
                    username: label.username,
                    // Only SSH sessions can answer exec, and only SSH sessions
                    // carry a label today.
                    kind: "ssh".to_string(),
                })
                .collect())
        })
    }

    fn check(&self, request: CheckRequest) -> BackendFuture<'_, CheckReport> {
        Box::pin(async move {
            let host = match (&request.session, &request.host) {
                (Some(session), _) => self.describe(session)?.label,
                (None, Some(host)) => host.clone(),
                (None, None) => String::new(),
            };
            let decision = self.grade(&request.command, &host)?;
            Ok(CheckReport {
                command: request.command,
                host,
                verdict: verdict_label(decision.verdict),
                risk: risk_label(decision.risk),
                reasons: reason_labels(&decision.reasons),
                rule: decision.rule,
                mode: mode_label(decision.mode),
            })
        })
    }

    fn exec(&self, request: ExecRequest) -> BackendFuture<'_, ExecReport> {
        Box::pin(async move {
            let label = self.describe(&request.session)?;
            let started = Instant::now();
            let decision = self.grade(&request.command, &label.label)?;
            let mut report = ExecReport {
                verdict: verdict_label(decision.verdict),
                risk: risk_label(decision.risk),
                reasons: reason_labels(&decision.reasons),
                rule: decision.rule.clone(),
                mode: mode_label(decision.mode),
                ran: false,
                output: None,
                truncated: false,
                duration_ms: 0,
            };

            match decision.verdict {
                Verdict::Allow => {
                    let timeout = Duration::from_millis(
                        request.timeout_ms.unwrap_or(60_000).clamp(1_000, MAX_TIMEOUT_MS),
                    );
                    // The audit line goes in before the command runs: a record
                    // written afterwards would be missing for exactly the
                    // commands that crash the process.
                    self.record(&request.command, &label.label, &decision, Choice::Auto);
                    let output = ft_ssh::exec(
                        &self.sessions,
                        &request.session,
                        &request.command,
                        timeout,
                        EXEC_CAP,
                    )
                    .await?;
                    report.truncated = output.len() >= EXEC_CAP;
                    report.output = Some(output);
                    report.ran = true;
                }
                Verdict::Ask => {
                    // Nobody to ask: an `ask` verdict reached from here stays
                    // unrun. Approving it is a person's job, in the window.
                    self.record(
                        &request.command,
                        &label.label,
                        &decision,
                        Choice::Cancelled,
                    );
                }
                Verdict::Deny => {
                    self.record(&request.command, &label.label, &decision, Choice::Denied);
                }
            }
            report.duration_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
            Ok(report)
        })
    }

    fn read_file(&self, request: PathRequest) -> BackendFuture<'_, FileBlob> {
        Box::pin(async move {
            self.describe(&request.session)?;
            let connection = self.sftp.get(&self.sessions, &request.session).await?;
            let vfs = ft_vfs::SftpVfs::new(connection);
            let mut file = vfs.open(&request.path, OpenMode::Read).await?;
            let mut content = Vec::new();
            let mut offset = 0u64;
            loop {
                let chunk = file.read(offset, 64 * 1024).await?;
                if chunk.is_empty() {
                    break;
                }
                offset += chunk.len() as u64;
                if offset > FILE_CAP {
                    let _ = file.close().await;
                    return Err(format!(
                        "{} is larger than the {FILE_CAP}-byte limit",
                        request.path
                    ));
                }
                content.extend_from_slice(&chunk);
            }
            file.close().await?;
            Ok(FileBlob {
                path: request.path,
                size: content.len() as u64,
                content_base64: ft_agent::encode_base64(&content),
            })
        })
    }

    fn write_file(&self, request: UploadRequest) -> BackendFuture<'_, UploadReport> {
        Box::pin(async move {
            self.describe(&request.session)?;
            let bytes = ft_agent::decode_base64(&request.content_base64)?;
            if bytes.len() as u64 > FILE_CAP {
                return Err(format!("upload exceeds the {FILE_CAP}-byte limit"));
            }
            let connection = self.sftp.get(&self.sessions, &request.session).await?;
            let vfs = ft_vfs::SftpVfs::new(connection);
            let mut file = vfs
                .open(&request.path, OpenMode::Write { truncate: true })
                .await?;
            let mut offset = 0u64;
            for chunk in bytes.chunks(64 * 1024) {
                file.write(offset, chunk).await?;
                offset += chunk.len() as u64;
            }
            file.close().await?;
            Ok(UploadReport {
                path: request.path,
                written: offset,
            })
        })
    }
}

fn verdict_label(verdict: Verdict) -> String {
    match verdict {
        Verdict::Allow => "allow",
        Verdict::Ask => "ask",
        Verdict::Deny => "deny",
    }
    .to_string()
}

fn risk_label(risk: ft_policy::Risk) -> String {
    match risk {
        ft_policy::Risk::R0 => "r0",
        ft_policy::Risk::R1 => "r1",
        ft_policy::Risk::R2 => "r2",
        ft_policy::Risk::R3 => "r3",
        ft_policy::Risk::R4 => "r4",
    }
    .to_string()
}

fn mode_label(mode: ft_policy::Mode) -> String {
    match mode {
        ft_policy::Mode::Auto => "auto",
        ft_policy::Mode::Ask => "ask",
        ft_policy::Mode::Plan => "plan",
    }
    .to_string()
}

/// Reason names travel as the same camelCase strings the audit file uses, so a
/// record and the answer that produced it read alike.
fn reason_labels(reasons: &[ft_policy::Reason]) -> Vec<String> {
    reasons
        .iter()
        .filter_map(|reason| {
            serde_json::to_value(reason)
                .ok()
                .and_then(|value| value.as_str().map(|text| text.to_string()))
        })
        .collect()
}

/// Starts the service on a worker thread with its own runtime.
///
/// A dedicated thread keeps the agent independent of the UI runtime: a stall in
/// the window cannot stop an agent mid-command, and the port is bound before
/// `setup` returns so the endpoint file is never a lie.
pub fn start(
    dir: &Path,
    sessions: SessionRegistry,
    labels: Arc<SessionLabels>,
    store: Arc<ft_store::Store>,
    sftp: Arc<ft_vfs::SftpRegistry>,
    audit: ft_policy::AuditLog,
) -> Result<(), String> {
    let (agent, listener) = ft_agent::server::Agent::bind().map_err(|err| err.to_string())?;
    let endpoint = agent.endpoint(env!("CARGO_PKG_VERSION"));
    let path = endpoint.save(dir)?;
    let _ = ENDPOINT_FILE.set(path.clone());

    let backend = AgentBackend {
        sessions,
        labels,
        store,
        sftp,
        audit,
    };
    let port = agent.port;
    let token = agent.token.clone();
    let version = env!("CARGO_PKG_VERSION").to_string();

    std::thread::Builder::new()
        .name("ferriterm-agent".to_string())
        .spawn(move || {
            let built = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            let runtime = match built {
                Ok(runtime) => runtime,
                Err(err) => {
                    tracing::error!(error = %err, "the agent runtime could not start");
                    return;
                }
            };
            let served = runtime.block_on(ft_agent::server::serve(
                listener,
                Arc::new(backend),
                token,
                version,
                std::future::pending::<()>(),
            ));
            if let Err(err) = served {
                tracing::error!(error = %err, "the agent service stopped");
            }
        })
        .map_err(|err| format!("cannot start the agent thread: {err}"))?;

    tracing::info!(
        port,
        endpoint = %path.display(),
        "control service listening on loopback"
    );
    Ok(())
}

/// Removes the endpoint file when the app exits cleanly, so the file never
/// outlives the token it holds.
pub fn clear_endpoint() {
    if let Some(path) = ENDPOINT_FILE.get() {
        let _ = std::fs::remove_file(path);
    }
}
