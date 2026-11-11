//! Long-running leak soak for the session stack.
//!
//! Cycles SSH shell sessions, SFTP transfers, vault writes and audit appends
//! against the loopback fixture, sampling resident memory, open descriptors and
//! thread count so a leak shows up as a slope instead of a hang.
//!
//! Run `testenv/soak.sh` (it starts the fixture), or by hand:
//!
//! ```sh
//! cargo run --release -p ferriterm --example loopback_sshd &
//! FERRITERM_SOAK_SECONDS=60 FERRITERM_SOAK_OUT=/tmp/soak \
//!   cargo run --release -p ferriterm --example soak
//! ```
//!
//! Environment:
//! - `FERRITERM_SOAK_SECONDS` run length, default 259200 (72h)
//! - `FERRITERM_SOAK_INTERVAL_MS` pause between cycles, default 2000
//! - `FERRITERM_SOAK_SAMPLE_MS` metrics cadence, default 60000
//! - `FERRITERM_SOAK_PORT` fixture port, default 2222
//! - `FERRITERM_SOAK_OUT` output directory, default `soak-run`

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ft_core::{ServerKeyVerifier, SessionRegistry, TerminalSink};
use ft_policy::{evaluate, AuditEntry, AuditLog, Choice, Context, Origin, Policy, Verdict};
use ft_security::CredentialStore;
use ft_vfs::{read_text, write_text, SftpRegistry, SftpVfs, Vfs};
use russh::keys::PublicKey;

/// Allowed growth over the baseline before the soak declares a leak. FDs and
/// threads are hard signals; RSS drifts with the allocator, so it needs both an
/// absolute and a relative cushion.
const FD_SLACK: u64 = 32;
const THREAD_SLACK: u64 = 16;
const RSS_SLACK_KB: u64 = 64 * 1024;
const WARMUP: Duration = Duration::from_secs(120);
const RSS_GUARD_AFTER: Duration = Duration::from_secs(30 * 60);
const CYCLE_TIMEOUT: Duration = Duration::from_secs(45);

struct Config {
    duration: Duration,
    interval: Duration,
    sample: Duration,
    port: u16,
    out: PathBuf,
}

impl Config {
    fn from_env() -> Self {
        fn secs(name: &str, default: u64) -> u64 {
            std::env::var(name)
                .ok()
                .and_then(|raw| raw.parse().ok())
                .unwrap_or(default)
        }
        Self {
            duration: Duration::from_secs(secs("FERRITERM_SOAK_SECONDS", 72 * 3600)),
            interval: Duration::from_millis(secs("FERRITERM_SOAK_INTERVAL_MS", 2000)),
            sample: Duration::from_millis(secs("FERRITERM_SOAK_SAMPLE_MS", 60_000)),
            port: secs("FERRITERM_SOAK_PORT", 2222) as u16,
            out: std::env::var_os("FERRITERM_SOAK_OUT")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("soak-run")),
        }
    }
}

#[derive(Default)]
struct CollectSink {
    buf: Mutex<Vec<u8>>,
    closed: AtomicBool,
}

impl TerminalSink for CollectSink {
    fn output(&self, _id: &str, data: Vec<u8>) {
        self.buf.lock().unwrap().extend_from_slice(&data);
    }

    fn closed(&self, _id: &str) {
        self.closed.store(true, Ordering::SeqCst);
    }
}

impl CollectSink {
    fn contains(&self, needle: &str) -> bool {
        String::from_utf8_lossy(&self.buf.lock().unwrap()).contains(needle)
    }
}

struct TrustLocalServer;

impl ServerKeyVerifier for TrustLocalServer {
    fn verify<'a>(
        &'a self,
        _host: &'a str,
        _port: u16,
        _key: &'a PublicKey,
    ) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async { true })
    }
}

fn connect_args(port: u16) -> ft_ssh::SshConnectArgs {
    ft_ssh::SshConnectArgs {
        host: "127.0.0.1".into(),
        port,
        username: "test".into(),
        password: Some("ferriterm-test".into()),
        key_path: None,
        key_id: None,
        auth_method: "password".into(),
        key_passphrase: None,
        keyboard_interactive: false,
        agent: false,
        x11: false,
        jump: Vec::new(),
        algorithms: None,
        encoding: None,
        keepalive: Some(10),
        host_id: None,
        cols: 80,
        rows: 24,
    }
}

async fn open(registry: &SessionRegistry, port: u16, id: &str) -> Result<Arc<CollectSink>, String> {
    let sink = Arc::new(CollectSink::default());
    ft_ssh::connect(
        sink.clone(),
        registry,
        id.to_string(),
        connect_args(port),
        ft_ssh::ConnectContext::new(Arc::new(TrustLocalServer)),
    )
    .await?;
    Ok(sink)
}

/// Polls the terminal buffer because output arrives asynchronously; the
/// fixture answers in milliseconds, so a long wait means something is wrong.
async fn wait_for(sink: &CollectSink, needle: &str) -> Result<(), String> {
    for _ in 0..200 {
        if sink.contains(needle) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!("no {needle:?} within 10s"))
}

async fn wait_closed(sink: &CollectSink) -> Result<(), String> {
    for _ in 0..200 {
        if sink.closed.load(Ordering::SeqCst) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err("channel never reported closed".into())
}

async fn assert_registry_empty(registry: &SessionRegistry) -> Result<(), String> {
    let sessions = registry.sessions.lock().await;
    if sessions.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} session(s) left in the registry: {:?}",
            sessions.len(),
            sessions.keys().collect::<Vec<_>>()
        ))
    }
}

async fn shell_cycle(registry: &SessionRegistry, port: u16, n: u64) -> Result<(), String> {
    let id = "soak-shell";
    let sink = open(registry, port, id).await?;
    let body = async {
        wait_for(&sink, "test@loopback").await?;
        ft_ssh::write(registry, id, "whoami\r".into()).await?;
        wait_for(&sink, "test\r\n").await?;
        ft_ssh::write(registry, id, format!("echo soak-{n}\r")).await?;
        wait_for(&sink, &format!("soak-{n}")).await?;
        ft_ssh::write(registry, id, "exit\r".into()).await?;
        wait_closed(&sink).await
    }
    .await;
    let _ = ft_ssh::disconnect(registry, id).await;
    if let Err(err) = body {
        return Err(format!("shell cycle {n}: {err}"));
    }
    assert_registry_empty(registry).await
}

async fn sftp_cycle(
    registry: &SessionRegistry,
    sftp: &SftpRegistry,
    port: u16,
    n: u64,
) -> Result<(), String> {
    let id = "soak-sftp";
    let sink = open(registry, port, id).await?;
    let body = async {
        wait_for(&sink, "test@loopback").await?;
        let connection = sftp.get(registry, id).await?;
        let vfs = SftpVfs::new(connection);
        let home = vfs.home().await?;
        let path = format!("{home}/soak-{n}.txt");
        let payload = format!("soak payload {n}");
        write_text(&vfs, &path, &payload, None).await?;
        let read = read_text(&vfs, &path, 64 * 1024).await?;
        if read.content != payload {
            return Err(format!("read back {:?}, wrote {payload:?}", read.content));
        }
        vfs.remove_file(&path).await?;
        Ok::<(), String>(())
    }
    .await;
    sftp.invalidate(id).await;
    let _ = ft_ssh::disconnect(registry, id).await;
    if let Err(err) = body {
        return Err(format!("sftp cycle {n}: {err}"));
    }
    assert_registry_empty(registry).await
}

fn vault_cycle(dir: &Path, n: u64) -> Result<(), String> {
    let store = CredentialStore::new(dir.to_path_buf());
    let first = format!("secret-{n}");
    store.set("soak", &first).map_err(|e| e.to_string())?;
    let got = store.get("soak").map_err(|e| e.to_string())?;
    if got.as_deref() != Some(first.as_str()) {
        return Err(format!("vault returned {got:?} after writing {first:?}"));
    }
    // A second write rotates the backup generation, which is the path that
    // failed on Windows when the copy was flushed read-only.
    let second = format!("secret-{n}-b");
    store.set("soak", &second).map_err(|e| e.to_string())?;
    store.delete("soak").map_err(|e| e.to_string())?;
    Ok(())
}

fn audit_cycle(log: &AuditLog, n: u64) -> Result<(), String> {
    let policy = Policy::default();
    for command in ["ls -la", "cat /etc/hostname", "rm -rf /"] {
        let decision = evaluate(&policy, command, &Context::host("soak"));
        let mut entry = AuditEntry {
            id: String::new(),
            ts: String::new(),
            host: "soak".into(),
            command: command.into(),
            risk: decision.risk,
            reasons: decision.reasons.clone(),
            rule: decision.rule.clone(),
            verdict: decision.verdict,
            choice: match decision.verdict {
                Verdict::Allow => Choice::Auto,
                Verdict::Ask => Choice::Cancelled,
                Verdict::Deny => Choice::Denied,
            },
            mode: decision.mode,
            origin: Some(Origin::Cli),
            prev_hash: String::new(),
            hash: String::new(),
        };
        log.append(&mut entry, now_secs())
            .map_err(|e| format!("audit append: {e}"))?;
    }
    if n % 100 == 0 {
        let months = log.months().map_err(|e| format!("audit months: {e}"))?;
        for month in months {
            let report = log
                .verify(&month)
                .map_err(|e| format!("audit verify: {e}"))?;
            if !report.ok {
                return Err(format!(
                    "audit chain broke in {month} at {:?} ({:?})",
                    report.broken_at, report.broken_reason
                ));
            }
        }
    }
    Ok(())
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Clone, Copy, Default)]
struct Metrics {
    rss_kb: u64,
    fds: u64,
    threads: u64,
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn fd_count() -> u64 {
    #[cfg(target_os = "macos")]
    let dir = "/dev/fd";
    #[cfg(target_os = "linux")]
    let dir = "/proc/self/fd";
    std::fs::read_dir(dir)
        .map(|entries| entries.filter_map(|entry| entry.ok()).count() as u64)
        .unwrap_or(0)
}

#[cfg(target_os = "macos")]
fn sample() -> Metrics {
    let pid = std::process::id().to_string();
    let rss = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", pid.as_str()])
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .and_then(|text| text.split_whitespace().next()?.parse().ok())
        .unwrap_or(0);
    let threads = std::process::Command::new("ps")
        .args(["-M", "-p", pid.as_str()])
        .output()
        .ok()
        .map(|out| {
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .filter(|line| !line.trim().is_empty())
                .count()
                .saturating_sub(1) as u64
        })
        .unwrap_or(0);
    Metrics {
        rss_kb: rss,
        fds: fd_count(),
        threads,
    }
}

#[cfg(target_os = "linux")]
fn sample() -> Metrics {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let mut metrics = Metrics {
        fds: fd_count(),
        ..Metrics::default()
    };
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            metrics.rss_kb = rest
                .split_whitespace()
                .next()
                .and_then(|value| value.parse().ok())
                .unwrap_or(0);
        } else if let Some(rest) = line.strip_prefix("Threads:") {
            metrics.threads = rest.trim().parse().unwrap_or(0);
        }
    }
    metrics
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn sample() -> Metrics {
    Metrics::default()
}

struct Shared {
    cycles: AtomicU64,
    failure: Mutex<Option<String>>,
}

impl Shared {
    fn fail(&self, reason: String) {
        let mut slot = self.failure.lock().unwrap();
        if slot.is_none() {
            *slot = Some(reason);
        }
    }

    fn reason(&self) -> Option<String> {
        self.failure.lock().unwrap().clone()
    }
}

struct Sampler<'a> {
    out: &'a Path,
    start: Instant,
    shared: &'a Shared,
    baseline: Option<Metrics>,
    samples: u64,
}

impl Sampler<'_> {
    fn tick(&mut self, registry: &SessionRegistry) {
        let metrics = sample();
        let elapsed = self.start.elapsed();
        let cycles = self.shared.cycles.load(Ordering::Relaxed);
        let sessions = registry.sessions.try_lock().map(|s| s.len()).unwrap_or(0);

        let line = format!(
            "{},{},{},{},{},{},{}\n",
            now_secs(),
            elapsed.as_secs(),
            cycles,
            metrics.rss_kb,
            metrics.fds,
            metrics.threads,
            sessions
        );
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.out.join("metrics.csv"))
        {
            use std::io::Write;
            let _ = file.write_all(line.as_bytes());
        }
        self.samples += 1;
        println!(
            "sample {}: {:5}s cycles={cycles} rss={}KB fds={} threads={} sessions={sessions}",
            self.samples,
            elapsed.as_secs(),
            metrics.rss_kb,
            metrics.fds,
            metrics.threads
        );

        if elapsed >= WARMUP && self.baseline.is_none() {
            self.baseline = Some(metrics);
            println!(
                "baseline: rss={}KB fds={} threads={}",
                metrics.rss_kb, metrics.fds, metrics.threads
            );
            return;
        }
        let Some(base) = self.baseline else { return };
        if metrics.fds > base.fds + FD_SLACK {
            self.shared.fail(format!(
                "file descriptors grew from {} to {}",
                base.fds, metrics.fds
            ));
        }
        if metrics.threads > base.threads + THREAD_SLACK {
            self.shared.fail(format!(
                "threads grew from {} to {}",
                base.threads, metrics.threads
            ));
        }
        if elapsed >= RSS_GUARD_AFTER
            && metrics.rss_kb > base.rss_kb + RSS_SLACK_KB
            && metrics.rss_kb > base.rss_kb * 3 / 2
        {
            self.shared.fail(format!(
                "resident memory grew from {}KB to {}KB",
                base.rss_kb, metrics.rss_kb
            ));
        }
    }
}

async fn run(cfg: Config) -> Result<(), String> {
    std::fs::create_dir_all(&cfg.out).map_err(|e| format!("create {}: {e}", cfg.out.display()))?;
    std::fs::write(
        cfg.out.join("metrics.csv"),
        "unix,elapsed_secs,cycles,rss_kb,fds,threads,sessions\n",
    )
    .map_err(|e| format!("write metrics header: {e}"))?;

    let registry = SessionRegistry::default();
    let sftp = SftpRegistry::default();
    let vault_dir = cfg.out.join("vault");
    let audit = AuditLog::new(cfg.out.join("audit"));
    let shared = Shared {
        cycles: AtomicU64::new(0),
        failure: Mutex::new(None),
    };
    let start = Instant::now();
    println!(
        "soak: {}s, interval {:?}, fixture 127.0.0.1:{}, out {}",
        cfg.duration.as_secs(),
        cfg.interval,
        cfg.port,
        cfg.out.display()
    );

    let mut sampler = Sampler {
        out: &cfg.out,
        start,
        shared: &shared,
        baseline: None,
        samples: 0,
    };
    sampler.tick(&registry);
    let mut last_sample = Instant::now();
    let mut cycle = 0u64;
    let mut outcome = Ok(());

    while start.elapsed() < cfg.duration {
        if let Some(reason) = shared.reason() {
            outcome = Err(reason);
            break;
        }
        cycle += 1;
        shared.cycles.store(cycle, Ordering::Relaxed);

        let step = async {
            shell_cycle(&registry, cfg.port, cycle).await?;
            if cycle % 15 == 0 {
                sftp_cycle(&registry, &sftp, cfg.port, cycle).await?;
            }
            if cycle % 150 == 0 {
                vault_cycle(&vault_dir, cycle)?;
            }
            if cycle % 10 == 0 {
                audit_cycle(&audit, cycle)?;
            }
            Ok::<(), String>(())
        };
        match tokio::time::timeout(CYCLE_TIMEOUT, step).await {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                outcome = Err(err);
                break;
            }
            Err(_) => {
                outcome = Err(format!(
                    "cycle {cycle} hung for {}s",
                    CYCLE_TIMEOUT.as_secs()
                ));
                break;
            }
        }

        if cycle % 30 == 0 {
            println!("cycle {cycle} ok after {:?}", start.elapsed());
        }
        if last_sample.elapsed() >= cfg.sample {
            sampler.tick(&registry);
            last_sample = Instant::now();
        }
        tokio::time::sleep(cfg.interval).await;
    }

    sampler.tick(&registry);
    if matches!(outcome, Ok(())) {
        if let Some(reason) = shared.reason() {
            outcome = Err(reason);
        }
    }
    let final_metrics = sample();
    let elapsed = start.elapsed();
    let summary = format!(
        "result: {}\nreason: {}\nplanned_secs: {}\nelapsed_secs: {}\ncycles: {}\nsamples: {}\nfinal_rss_kb: {}\nfinal_fds: {}\nfinal_threads: {}\nsessions_left: {}\n",
        if outcome.is_ok() { "PASS" } else { "FAIL" },
        outcome.clone().err().unwrap_or_default(),
        cfg.duration.as_secs(),
        elapsed.as_secs(),
        shared.cycles.load(Ordering::Relaxed),
        sampler.samples,
        final_metrics.rss_kb,
        final_metrics.fds,
        final_metrics.threads,
        registry.sessions.lock().await.len()
    );
    std::fs::write(cfg.out.join("summary.txt"), &summary)
        .map_err(|e| format!("write summary: {e}"))?;
    print!("{summary}");
    outcome
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let cfg = Config::from_env();
    match run(cfg).await {
        Ok(()) => println!("SOAK_PASS"),
        Err(err) => {
            eprintln!("SOAK_FAIL: {err}");
            std::process::exit(1);
        }
    }
}
