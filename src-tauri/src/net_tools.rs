//! Network toolbox: local ICMP/TCP/UDP probes, traceroute, and an IP
//! geolocation lookup.
//!
//! ICMP and traceroute run the platform binaries directly (never through a
//! shell) so a target can only ever be an address. TCP and UDP probes are
//! implemented here on tokio sockets: `tcping` needs a port and a connect time,
//! which no bundled Windows tool provides, and UDP has no portable binary at
//! all. Every probe emits one line per attempt and a summary at the end.

use std::collections::{HashMap, HashSet};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, State};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::net::{TcpStream, UdpSocket};
use tokio::process::Command;

pub const OUTPUT_EVENT: &str = "net-tool://output";
pub const DONE_EVENT: &str = "net-tool://done";

/// Jobs are tracked by pid so cancellation never has to borrow the child that
/// the waiting task owns. Native probes carry no pid; they poll the cancelled
/// set between attempts instead.
#[derive(Default)]
pub struct NetToolState {
    jobs: Mutex<HashMap<String, u32>>,
    cancelled: Mutex<HashSet<String>>,
    next: AtomicU64,
}

impl NetToolState {
    fn is_cancelled(&self, id: &str) -> bool {
        self.cancelled
            .lock()
            .map(|set| set.contains(id))
            .unwrap_or(false)
    }

    fn clear_cancelled(&self, id: &str) -> bool {
        self.cancelled
            .lock()
            .map(|mut set| set.remove(id))
            .unwrap_or(false)
    }
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct OutputChunk {
    id: String,
    stream: &'static str,
    line: String,
}

/// Aggregate of one probe run. `min/avg/max` are only set when at least one
/// attempt came back, so the interface can tell "0 ms" from "no reply".
#[derive(Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ProbeSummary {
    sent: u32,
    received: u32,
    /// Probes that answered with something other than a reply (UDP only).
    replied: u32,
    min: Option<f64>,
    avg: Option<f64>,
    max: Option<f64>,
}

impl ProbeSummary {
    fn record(&mut self, rtt: Option<f64>) {
        self.sent += 1;
        let Some(rtt) = rtt else {
            return;
        };
        self.received += 1;
        let (min, max) = (self.min.unwrap_or(rtt), self.max.unwrap_or(rtt));
        self.min = Some(min.min(rtt));
        self.max = Some(max.max(rtt));
        let total = self.avg.unwrap_or(0.0) * (self.received - 1) as f64 + rtt;
        self.avg = Some(total / self.received as f64);
    }
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DoneEvent {
    id: String,
    code: Option<i32>,
    cancelled: bool,
    error: Option<String>,
    summary: Option<ProbeSummary>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeoInfo {
    ip: String,
    version: String,
    country: String,
    country_code: String,
    region: String,
    city: String,
    latitude: f64,
    longitude: f64,
    isp: String,
    org: String,
    asn: String,
    timezone: String,
    source: String,
}

pub(crate) fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => {
            // Windows ships ping/tracert as ANSI programs, so a zh-CN host emits
            // GBK bytes. Anything unreadable still surfaces as replacement chars.
            #[cfg(windows)]
            {
                let (text, _, _) = encoding_rs::GBK.decode(bytes);
                text.into_owned()
            }
            #[cfg(not(windows))]
            {
                String::from_utf8_lossy(bytes).into_owned()
            }
        }
    }
}

fn validate_target(raw: &str) -> Result<String, String> {
    let target = raw.trim();
    if target.is_empty() {
        return Err("target is required".to_string());
    }
    if target.len() > 253 {
        return Err("target is too long".to_string());
    }
    let allowed = target
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '-' | '_' | '[' | ']' | '%'));
    if !allowed {
        return Err(
            "target may only contain letters, digits, dots, dashes, underscores and colons"
                .to_string(),
        );
    }
    Ok(target.to_string())
}

/// Arguments for a single ICMP echo. One echo per process keeps the interval
/// under our control: no supported platform exposes "wait N ms between pings"
/// as a flag, and the loop also lets a stop request land between attempts.
#[cfg(windows)]
fn ping_args(timeout_ms: u64, size: u32) -> (&'static str, Vec<String>) {
    (
        "ping",
        vec![
            "-n".to_string(),
            "1".to_string(),
            "-w".to_string(),
            timeout_ms.to_string(),
            "-l".to_string(),
            size.to_string(),
        ],
    )
}

#[cfg(not(windows))]
fn ping_args(timeout_ms: u64, size: u32) -> (&'static str, Vec<String>) {
    (
        "ping",
        vec![
            "-c".to_string(),
            "1".to_string(),
            // `ping -W` takes whole seconds on every platform we ship, so a
            // sub-second timeout rounds up rather than erroring out.
            "-W".to_string(),
            ((timeout_ms + 999) / 1000).to_string(),
            "-s".to_string(),
            size.to_string(),
        ],
    )
}

#[cfg(windows)]
fn traceroute_args(hops: u32, timeout_ms: u64) -> (&'static str, Vec<String>) {
    (
        "tracert",
        vec![
            "-d".to_string(),
            "-h".to_string(),
            hops.to_string(),
            "-w".to_string(),
            timeout_ms.to_string(),
        ],
    )
}

#[cfg(not(windows))]
fn traceroute_args(hops: u32, timeout_ms: u64) -> (&'static str, Vec<String>) {
    (
        "traceroute",
        vec![
            "-n".to_string(),
            "-m".to_string(),
            hops.to_string(),
            "-w".to_string(),
            ((timeout_ms + 999) / 1000).to_string(),
        ],
    )
}

fn emit_line(app: &AppHandle, id: &str, line: impl Into<String>) {
    let _ = app.emit(
        OUTPUT_EVENT,
        OutputChunk {
            id: id.to_string(),
            stream: "out",
            line: line.into(),
        },
    );
}

fn emit_error(app: &AppHandle, id: &str, line: impl Into<String>) {
    let _ = app.emit(
        OUTPUT_EVENT,
        OutputChunk {
            id: id.to_string(),
            stream: "err",
            line: line.into(),
        },
    );
}

/// Pulls the round-trip time out of a native ping reply. Windows prints
/// `time=12ms` / `时间=12ms`, Unix prints `time=12.3 ms`; both are found by
/// scanning for a number followed by `ms`. Returns `None` when the line has no
/// timing at all (a failure line), which is exactly what the caller needs.
fn parse_rtt_ms(line: &str) -> Option<f64> {
    let bytes = line.as_bytes();
    let mut index = 0;
    while index + 1 < bytes.len() {
        if bytes[index] != b'm' || bytes[index + 1] != b's' {
            index += 1;
            continue;
        }
        // Unix prints a space before the unit (`time=9.43 ms`), Windows does
        // not (`time=12ms`); skip blanks before reading the number.
        let mut cursor = index;
        while cursor > 0 && bytes[cursor - 1] == b' ' {
            cursor -= 1;
        }
        let mut start = cursor;
        while start > 0 && (bytes[start - 1].is_ascii_digit() || bytes[start - 1] == b'.') {
            start -= 1;
        }
        if start == cursor {
            index += 2;
            continue;
        }
        if let Ok(value) = line[start..cursor].parse::<f64>() {
            return Some(value);
        }
        index += 2;
    }
    None
}

/// One ICMP echo through the platform binary. Returns the rtt reported by the
/// ping itself, or `None` when the attempt did not come back.
async fn icmp_once(
    app: &AppHandle,
    id: &str,
    target: &str,
    timeout_ms: u64,
    size: u32,
) -> Option<f64> {
    let (program, args) = ping_args(timeout_ms, size);
    let mut command = Command::new(program);
    command
        .args(args)
        .arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        command.creation_flags(0x0800_0000);
    }
    let output = match command.output().await {
        Ok(output) => output,
        Err(err) => {
            emit_error(app, id, format!("cannot start {program}: {err}"));
            return None;
        }
    };
    let text = format!("{}\n{}", decode(&output.stdout), decode(&output.stderr));
    let mut rtt = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        emit_line(app, id, line);
        if rtt.is_none() {
            rtt = parse_rtt_ms(line);
        }
    }
    if output.status.success() {
        rtt.or(Some(0.0))
    } else {
        None
    }
}

fn summarize(kind: &str, target: &str, summary: &ProbeSummary) -> String {
    // UDP has no handshake, so "received" would overstate what the probe knows:
    // a silent port is reported as no reply rather than as a lost packet.
    if kind.starts_with("UDP") {
        return format!(
            "--- {kind} {target}: {} sent, {} replied (no reply may mean filtered or closed)",
            summary.sent, summary.replied
        );
    }
    let loss = if summary.sent == 0 {
        0.0
    } else {
        (summary.sent - summary.received) as f64 * 100.0 / summary.sent as f64
    };
    match (summary.min, summary.avg, summary.max) {
        (Some(min), Some(avg), Some(max)) => format!(
            "--- {kind} {target}: {} sent, {} received, {loss:.0}% loss, min/avg/max = {min:.1}/{avg:.1}/{max:.1} ms",
            summary.sent, summary.received
        ),
        _ => format!(
            "--- {kind} {target}: {} sent, {} received, {loss:.0}% loss",
            summary.sent, summary.received
        ),
    }
}

/// TCP ping: time the connect handshake, then drop the socket at once. Nothing
/// is written to the peer.
async fn tcp_once(target: &str, port: u16, timeout_ms: u64) -> Result<f64, String> {
    let started = Instant::now();
    match tokio::time::timeout(
        Duration::from_millis(timeout_ms),
        TcpStream::connect((target, port)),
    )
    .await
    {
        Ok(Ok(_stream)) => Ok(started.elapsed().as_secs_f64() * 1000.0),
        Ok(Err(err)) => Err(err.to_string()),
        Err(_) => Err("timeout".to_string()),
    }
}

/// UDP probe: send a datagram and wait for anything to come back.
///
/// Without a raw socket there is no way to hear the ICMP port-unreachable that
/// a closed port answers with, so "no reply" genuinely cannot be told apart
/// from a filtered port. The caller labels it that way instead of pretending
/// the packet was lost.
async fn udp_once(target: &str, port: u16, timeout_ms: u64, payload: usize) -> Result<usize, String> {
    let socket = UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|err| err.to_string())?;
    socket
        .connect((target, port))
        .await
        .map_err(|err| err.to_string())?;
    socket
        .send(&vec![0u8; payload])
        .await
        .map_err(|err| err.to_string())?;
    let mut buffer = vec![0u8; 1500];
    match tokio::time::timeout(Duration::from_millis(timeout_ms), socket.recv(&mut buffer)).await {
        Ok(Ok(read)) => Ok(read),
        Ok(Err(err)) => Err(err.to_string()),
        Err(_) => Err("no reply".to_string()),
    }
}

/// One traceroute run. The platform binary already walks the path, so there is
/// nothing to loop over here.
async fn run_traceroute(
    app: AppHandle,
    state: Arc<NetToolState>,
    id: String,
    target: String,
    hops: u32,
    timeout_ms: u64,
) -> (Option<i32>, Option<String>) {
    let (program, args) = traceroute_args(hops, timeout_ms);
    let mut command = Command::new(program);
    command
        .args(args)
        .arg(&target)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        command.creation_flags(0x0800_0000);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => return (None, Some(format!("cannot start {program}: {err}"))),
    };
    if let Ok(mut jobs) = state.jobs.lock() {
        jobs.insert(id.clone(), child.id().unwrap_or_default());
    }
    if let Some(stdout) = child.stdout.take() {
        tokio::spawn(pump(app.clone(), id.clone(), "out", stdout));
    }
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(pump(app.clone(), id.clone(), "err", stderr));
    }
    let result = match child.wait().await {
        Ok(status) => (status.code(), None),
        Err(err) => (None, Some(err.to_string())),
    };
    if let Ok(mut jobs) = state.jobs.lock() {
        jobs.remove(&id);
    }
    result
}

async fn pump<R>(app: AppHandle, id: String, stream: &'static str, reader: R)
where
    R: AsyncRead + Unpin,
{
    let mut reader = BufReader::new(reader);
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        match reader.read_until(b'\n', &mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let text = decode(&buffer);
                let line = text.trim_end_matches(['\r', '\n']).to_string();
                let _ = app.emit(
                    OUTPUT_EVENT,
                    OutputChunk {
                        id: id.clone(),
                        stream,
                        line,
                    },
                );
            }
        }
    }
}

/// Starts a probe against `target` and streams its output.
///
/// `interval`, `timeout` and `size` are honoured by every probe that can use
/// them; `port` is required by the TCP and UDP probes.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn net_tool_start(
    app: AppHandle,
    state: State<'_, Arc<NetToolState>>,
    kind: String,
    target: String,
    port: Option<u16>,
    count: Option<u32>,
    interval: Option<u64>,
    timeout: Option<u64>,
    size: Option<u32>,
    hops: Option<u32>,
) -> Result<String, String> {
    let state = state.inner().clone();
    let target = validate_target(&target)?;
    let count = count.unwrap_or(4).clamp(1, 100);
    // Below ~100 ms the loop is measuring the scheduler, above 10 s the user is
    // better served by a longer timeout than by a longer wait between probes.
    let interval = interval.unwrap_or(1000).clamp(100, 10_000);
    let timeout = timeout.unwrap_or(2000).clamp(100, 10_000);
    let size = size.unwrap_or(56).clamp(1, 65500);
    let hops = hops.unwrap_or(20).clamp(1, 64);
    let port = port.unwrap_or(443);

    let id = format!("{kind}-{}", state.next.fetch_add(1, Ordering::Relaxed) + 1);
    let label = match kind.as_str() {
        "ping" => "ICMP".to_string(),
        "tcping" => format!("TCP :{port}"),
        "udp" => format!("UDP :{port}"),
        "traceroute" => format!("TRACE hops={hops}"),
        other => return Err(format!("unsupported network tool: {other}")),
    };

    let job_id = id.clone();
    let job_state = state.clone();
    tokio::spawn(async move {
        let mut summary = ProbeSummary::default();
        let start_app = app.clone();
        let start_id = job_id.clone();
        emit_line(
            &start_app,
            &start_id,
            format!(
                "{label} {target} count={count} interval={interval}ms timeout={timeout}ms size={size}"
            ),
        );

        let mut code: Option<i32> = None;
        let mut error: Option<String> = None;

        if kind == "traceroute" {
            let (exit, err) = run_traceroute(
                app.clone(),
                job_state.clone(),
                job_id.clone(),
                target.clone(),
                hops,
                timeout,
            )
            .await;
            code = exit;
            error = err;
        } else {
            for seq in 1..=count {
                if job_state.is_cancelled(&job_id) {
                    break;
                }
                match kind.as_str() {
                    "ping" => {
                        let rtt = icmp_once(&app, &job_id, &target, timeout, size).await;
                        summary.record(rtt);
                    }
                    "tcping" => {
                        let line = match tcp_once(&target, port, timeout).await {
                            Ok(rtt) => {
                                summary.record(Some(rtt));
                                format!("seq={seq} port={port} {:.1} ms", rtt)
                            }
                            Err(err) => {
                                summary.record(None);
                                format!("seq={seq} port={port} {err}")
                            }
                        };
                        emit_line(&app, &job_id, line);
                    }
                    "udp" => {
                        let line = match udp_once(&target, port, timeout, size as usize).await {
                            Ok(read) => {
                                summary.sent += 1;
                                summary.received += 1;
                                summary.replied += 1;
                                format!("seq={seq} port={port} reply {read} bytes")
                            }
                            Err(err) => {
                                summary.sent += 1;
                                format!("seq={seq} port={port} {err} (open/filtered is indistinguishable over UDP)")
                            }
                        };
                        emit_line(&app, &job_id, line);
                    }
                    _ => {}
                }
                if seq < count {
                    tokio::time::sleep(Duration::from_millis(interval)).await;
                }
            }
            if !job_state.is_cancelled(&job_id) {
                emit_line(&app, &job_id, summarize(&label, &target, &summary));
            }
        }

        let cancelled = job_state.clear_cancelled(&job_id);
        let event = DoneEvent {
            id: job_id.clone(),
            code,
            cancelled,
            error,
            summary: if kind == "traceroute" {
                None
            } else {
                Some(summary)
            },
        };
        let _ = app.emit(DONE_EVENT, event);
    });

    Ok(id)
}

/// Stops a running probe. Native probes poll the cancelled set between
/// attempts; process-backed probes are killed by pid. The job always emits its
/// terminal event.
#[tauri::command]
pub fn net_tool_cancel(state: State<'_, Arc<NetToolState>>, id: String) -> Result<(), String> {
    let pid = state
        .jobs
        .lock()
        .map_err(|_| "job registry is poisoned".to_string())?
        .get(&id)
        .copied();
    if let Ok(mut set) = state.cancelled.lock() {
        set.insert(id);
    }
    let Some(pid) = pid.filter(|pid| *pid != 0) else {
        return Ok(());
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            // No console of our own: without this the kill flashes a window.
            .creation_flags(0x0800_0000)
            .status();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    Ok(())
}

fn encode_segment(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn text_of(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|item| item.as_str())
        .unwrap_or_default()
        .to_string()
}

fn number_of(value: &serde_json::Value, key: &str) -> f64 {
    value
        .get(key)
        .and_then(|item| item.as_f64())
        .unwrap_or_default()
}

async fn lookup_ipwho(client: &reqwest::Client, target: &str) -> Result<GeoInfo, String> {
    let url = if target.is_empty() {
        "https://ipwho.is/?lang=zh-CN".to_string()
    } else {
        format!("https://ipwho.is/{}?lang=zh-CN", encode_segment(target))
    };
    let response = client
        .get(url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let payload: serde_json::Value = response.json().await.map_err(|err| err.to_string())?;
    if payload.get("success").and_then(|item| item.as_bool()) == Some(false) {
        return Err(text_of(&payload, "message"));
    }
    let connection = payload.get("connection").cloned().unwrap_or_default();
    let timezone = payload.get("timezone").cloned().unwrap_or_default();
    Ok(GeoInfo {
        ip: text_of(&payload, "ip"),
        version: text_of(&payload, "type"),
        country: text_of(&payload, "country"),
        country_code: text_of(&payload, "country_code"),
        region: text_of(&payload, "region"),
        city: text_of(&payload, "city"),
        latitude: number_of(&payload, "latitude"),
        longitude: number_of(&payload, "longitude"),
        isp: text_of(&connection, "isp"),
        org: text_of(&connection, "org"),
        asn: connection
            .get("asn")
            .and_then(|item| item.as_u64())
            .map(|value| format!("AS{value}"))
            .unwrap_or_default(),
        timezone: text_of(&timezone, "id"),
        source: "ipwho.is".to_string(),
    })
}

async fn lookup_ipapi(client: &reqwest::Client, target: &str) -> Result<GeoInfo, String> {
    let url = if target.is_empty() {
        "http://ip-api.com/json/?lang=zh-CN".to_string()
    } else {
        format!(
            "http://ip-api.com/json/{}?lang=zh-CN",
            encode_segment(target)
        )
    };
    let response = client
        .get(url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let payload: serde_json::Value = response.json().await.map_err(|err| err.to_string())?;
    if text_of(&payload, "status") == "fail" {
        return Err(text_of(&payload, "message"));
    }
    let ip = text_of(&payload, "query");
    Ok(GeoInfo {
        version: if ip.contains(':') {
            "IPv6".to_string()
        } else {
            "IPv4".to_string()
        },
        ip,
        country: text_of(&payload, "country"),
        country_code: text_of(&payload, "countryCode"),
        region: text_of(&payload, "regionName"),
        city: text_of(&payload, "city"),
        latitude: number_of(&payload, "lat"),
        longitude: number_of(&payload, "lon"),
        isp: text_of(&payload, "isp"),
        org: text_of(&payload, "org"),
        asn: text_of(&payload, "as"),
        timezone: text_of(&payload, "timezone"),
        source: "ip-api.com".to_string(),
    })
}

/// Resolves the geolocation of an address (or the local public address when the
/// query is empty). Two public providers are tried so a single outage does not
/// break the tool.
#[tauri::command]
pub async fn geoip_lookup(query: String) -> Result<GeoInfo, String> {
    let target = query.trim().to_string();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .user_agent("ferriterm/0.1")
        .build()
        .map_err(|err| err.to_string())?;

    let mut errors = Vec::new();
    match lookup_ipwho(&client, &target).await {
        Ok(info) => return Ok(info),
        Err(err) => errors.push(format!("ipwho.is: {err}")),
    }
    match lookup_ipapi(&client, &target).await {
        Ok(info) => return Ok(info),
        Err(err) => errors.push(format!("ip-api.com: {err}")),
    }
    Err(errors.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_valid_targets() {
        assert_eq!(validate_target(" 8.8.8.8 ").unwrap(), "8.8.8.8");
        assert_eq!(validate_target("example.com").unwrap(), "example.com");
        assert_eq!(validate_target("2001:db8::1").unwrap(), "2001:db8::1");
    }

    #[test]
    fn rejects_shell_syntax() {
        assert!(validate_target("").is_err());
        assert!(validate_target("8.8.8.8; rm -rf /").is_err());
        assert!(validate_target("$(id)").is_err());
        assert!(validate_target("a b").is_err());
    }

    #[test]
    fn decodes_platform_ping_output() {
        let (program, args) = ping_args(2000, 56);
        let output = std::process::Command::new(program)
            .args(args)
            .arg("127.0.0.1")
            .output()
            .expect("ping runs");
        let text = decode(&output.stdout);
        assert!(text.contains("127.0.0.1"), "unexpected output: {text}");
    }

    #[test]
    fn reads_rtt_from_both_locales() {
        assert_eq!(parse_rtt_ms("Reply from 8.8.8.8: bytes=32 time=12ms TTL=118"), Some(12.0));
        assert_eq!(parse_rtt_ms("来自 8.8.8.8 的回复: 字节=32 时间=13ms TTL=118"), Some(13.0));
        assert_eq!(parse_rtt_ms("64 bytes from 1.1.1.1: icmp_seq=1 ttl=57 time=9.43 ms"), Some(9.43));
        assert_eq!(parse_rtt_ms("Reply from 8.8.8.8: bytes=32 time<1ms TTL=118"), Some(1.0));
        assert_eq!(parse_rtt_ms("Request timed out."), None);
    }

    #[test]
    fn summarises_loss_and_latency() {
        let mut summary = ProbeSummary::default();
        summary.record(Some(10.0));
        summary.record(None);
        summary.record(Some(20.0));
        assert_eq!(summary.sent, 3);
        assert_eq!(summary.received, 2);
        assert_eq!(summary.min, Some(10.0));
        assert_eq!(summary.max, Some(20.0));
        assert_eq!(summary.avg, Some(15.0));
        let line = summarize("TCP :443", "example.com", &summary);
        assert!(line.contains("3 sent, 2 received, 33% loss"), "{line}");
        assert!(line.contains("10.0/15.0/20.0 ms"), "{line}");
    }

    #[test]
    fn empty_summary_has_no_latency() {
        let summary = ProbeSummary::default();
        let line = summarize("ICMP", "example.com", &summary);
        assert!(line.contains("0 sent, 0 received, 0% loss"), "{line}");
        assert!(!line.contains("min/avg/max"), "{line}");
    }

    #[test]
    fn udp_summary_counts_replies_not_loss() {
        let mut summary = ProbeSummary::default();
        summary.sent += 1;
        summary.received += 1;
        summary.replied += 1;
        summary.sent += 1;
        let line = summarize("UDP :53", "1.1.1.1", &summary);
        assert!(line.contains("2 sent, 1 replied"), "{line}");
        assert!(line.contains("filtered or closed"), "{line}");
        assert!(!line.contains("loss"), "{line}");
    }
}
