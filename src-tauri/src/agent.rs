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

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
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
/// Settings key holding whether the service should be listening at all.
pub const ENABLED_KEY: &str = "agent:enabled";
/// How long a stop waits for the serving thread to let go of the port before
/// giving up and leaving it to finish on its own.
const STOP_TIMEOUT: Duration = Duration::from_secs(2);

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
        origin: ft_policy::Origin,
    ) {
        // Best effort on purpose: a refused command must still reach the
        // caller even if the disk is full, and the failure is logged there.
        let entry = AuditInput {
            command: command.to_string(),
            host: host.to_string(),
            decision: decision.clone(),
            choice,
            origin: Some(origin),
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
            // Declared by the caller, not guessed: a record that says "mcp" is
            // answering the question "did an agent do this".
            let origin = origin_of(request.origin.as_deref());
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
                    self.record(&request.command, &label.label, &decision, Choice::Auto, origin);
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
                        origin,
                    );
                }
                Verdict::Deny => {
                    self.record(&request.command, &label.label, &decision, Choice::Denied, origin);
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

/// Maps what a caller says about itself onto the chain's vocabulary. Anything
/// unrecognised is the window, which is the only caller that never says.
fn origin_of(declared: Option<&str>) -> ft_policy::Origin {
    match declared {
        Some("cli") => ft_policy::Origin::Cli,
        Some("mcp") => ft_policy::Origin::Mcp,
        _ => ft_policy::Origin::Ui,
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

/// What the settings page shows about the service.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatus {
    /// Whether the service is meant to be listening, as stored in settings.
    pub enabled: bool,
    /// Whether it is listening right now. These differ only when a bind failed.
    pub running: bool,
    /// The port it answers on; `0` while stopped.
    pub port: u16,
}

/// Everything the service needs from the running app, held for its lifetime.
struct Deps {
    sessions: SessionRegistry,
    labels: Arc<SessionLabels>,
    store: Arc<ft_store::Store>,
    sftp: Arc<ft_vfs::SftpRegistry>,
    audit: ft_policy::AuditLog,
}

struct Running {
    port: u16,
    /// Dropping or firing this ends `serve`.
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    /// Set by the serving thread just before it exits, so a stop can wait for
    /// the listener to be released instead of guessing with a sleep.
    finished: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[derive(Default)]
struct Inner {
    running: Option<Running>,
    /// Credentials of the last run. Kept across an off/on switch so a client
    /// that was handed a token before the switch can keep using it after.
    credentials: Option<(u16, String)>,
}

/// The control service as a managed piece of the app: it can be switched off
/// and on from settings without restarting the window.
pub struct AgentService {
    dir: PathBuf,
    deps: Deps,
    inner: Mutex<Inner>,
}

impl AgentService {
    pub fn new(
        dir: PathBuf,
        sessions: SessionRegistry,
        labels: Arc<SessionLabels>,
        store: Arc<ft_store::Store>,
        sftp: Arc<ft_vfs::SftpRegistry>,
        audit: ft_policy::AuditLog,
    ) -> Self {
        Self {
            dir,
            deps: Deps {
                sessions,
                labels,
                store,
                sftp,
                audit,
            },
            inner: Mutex::new(Inner::default()),
        }
    }

    pub fn enabled(&self) -> bool {
        self.deps
            .store
            .get_setting(ENABLED_KEY)
            .ok()
            .flatten()
            .map(|value| value != "0")
            .unwrap_or(true)
    }

    pub fn status(&self) -> AgentStatus {
        let port = self
            .inner
            .lock()
            .ok()
            .and_then(|inner| inner.running.as_ref().map(|running| running.port))
            .unwrap_or(0);
        AgentStatus {
            enabled: self.enabled(),
            running: port != 0,
            port,
        }
    }

    /// Starts listening, or does nothing when it already is.
    pub fn start(&self) -> Result<(), String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "the control service state is poisoned".to_string())?;
        if inner.running.is_some() {
            return Ok(());
        }
        let reused = inner.credentials.clone();
        let (agent, listener) = match &reused {
            Some((port, token)) => {
                ft_agent::server::Agent::rebind(*port, token.clone()).map_err(bind_error)?
            }
            None => ft_agent::server::Agent::bind().map_err(bind_error)?,
        };
        let path = agent
            .endpoint(env!("CARGO_PKG_VERSION"))
            .save(&self.dir)
            .map_err(|err| format!("cannot write the endpoint file: {err}"))?;
        let port = agent.port;
        inner.credentials = Some((port, agent.token.clone()));
        let token = agent.token;

        let backend = Arc::new(AgentBackend {
            sessions: self.deps.sessions.clone(),
            labels: self.deps.labels.clone(),
            store: self.deps.store.clone(),
            sftp: self.deps.sftp.clone(),
            audit: self.deps.audit.clone(),
        });
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let finished = Arc::new(AtomicBool::new(false));
        let flag = finished.clone();
        let version = env!("CARGO_PKG_VERSION").to_string();
        let spawned = std::thread::Builder::new()
            .name("ferriterm-agent".to_string())
            .spawn(move || {
                let built = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                let runtime = match built {
                    Ok(runtime) => runtime,
                    Err(err) => {
                        tracing::error!(error = %err, "the agent runtime could not start");
                        flag.store(true, Ordering::SeqCst);
                        return;
                    }
                };
                let served = runtime.block_on(ft_agent::server::serve(
                    listener,
                    backend,
                    token,
                    version,
                    async move {
                        let _ = stopped.await;
                    },
                ));
                if let Err(err) = served {
                    tracing::error!(error = %err, "the agent service stopped");
                }
                flag.store(true, Ordering::SeqCst);
            });
        let thread = match spawned {
            Ok(thread) => thread,
            Err(err) => {
                // A thread we could not start is a service that is not there,
                // so the endpoint file must not claim otherwise.
                let _ = std::fs::remove_file(&path);
                return Err(format!("cannot start the agent thread: {err}"));
            }
        };
        inner.running = Some(Running {
            port,
            stop: Some(stop),
            finished,
            thread: Some(thread),
        });
        tracing::info!(
            port,
            endpoint = %path.display(),
            "control service listening on loopback"
        );
        Ok(())
    }

    /// Stops listening and removes the endpoint file. Credentials survive, so
    /// switching straight back on reuses the port clients already know.
    pub fn stop(&self) -> Result<(), String> {
        let running = {
            let mut inner = self
                .inner
                .lock()
                .map_err(|_| "the control service state is poisoned".to_string())?;
            inner.running.take()
        };
        let endpoint = ft_agent::Endpoint::path_in(&self.dir);
        let Some(mut running) = running else {
            let _ = std::fs::remove_file(endpoint);
            return Ok(());
        };
        if let Some(stop) = running.stop.take() {
            let _ = stop.send(());
        }
        // The serving thread owns the listener; wait for it to return so the
        // port is free again before anyone tries to switch the service on.
        let deadline = Instant::now() + STOP_TIMEOUT;
        while !running.finished.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        if running.finished.load(Ordering::SeqCst) {
            if let Some(thread) = running.thread.take() {
                let _ = thread.join();
            }
        } else {
            tracing::warn!(
                port = running.port,
                "the control service did not stop in time"
            );
        }
        let _ = std::fs::remove_file(endpoint);
        tracing::info!(port = running.port, "control service stopped");
        Ok(())
    }

    /// Persists the switch and matches the socket to it.
    pub fn set_enabled(&self, enabled: bool) -> Result<AgentStatus, String> {
        self.deps
            .store
            .set_setting(ENABLED_KEY, if enabled { "1" } else { "0" })
            .map_err(|err| err.to_string())?;
        if enabled {
            self.start()?;
        } else {
            self.stop()?;
        }
        Ok(self.status())
    }

    /// Brings the socket in line with the stored switch, on startup.
    pub fn start_if_enabled(&self) -> Result<(), String> {
        if !self.enabled() {
            tracing::info!("the control service is switched off in settings");
            return Ok(());
        }
        self.start()
    }
}

fn bind_error(err: std::io::Error) -> String {
    format!("cannot bind the control service: {err}")
}

#[tauri::command]
pub fn agent_status(service: tauri::State<'_, Arc<AgentService>>) -> AgentStatus {
    service.status()
}

#[tauri::command]
pub fn agent_service_set(
    service: tauri::State<'_, Arc<AgentService>>,
    enabled: bool,
) -> Result<AgentStatus, String> {
    service.set_enabled(enabled)
}

/// The CLI that speaks to this service, as an absolute path when it sits next
/// to the app. The settings page shows this so a copied configuration points at
/// a binary that exists instead of at whatever `ferriterm-cli` resolves to in
/// some other shell.
#[tauri::command]
pub fn agent_cli_path() -> String {
    let name = if cfg!(windows) {
        "ferriterm-cli.exe"
    } else {
        "ferriterm-cli"
    };
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(name)))
        .filter(|path| path.exists())
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| name.to_string())
}
