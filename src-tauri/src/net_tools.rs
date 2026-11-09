//! Network toolbox: local ping / traceroute runs plus an IP geolocation lookup.
//!
//! The probes run the platform binaries directly (never through a shell) so a
//! target can only ever be an address, and the output of the child process is
//! republished as line events the toolbox console can append to.

use std::collections::{HashMap, HashSet};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Emitter, State};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;

pub const OUTPUT_EVENT: &str = "net-tool://output";
pub const DONE_EVENT: &str = "net-tool://done";

/// Jobs are tracked by pid so cancellation never has to borrow the child that
/// the waiting task owns.
#[derive(Default)]
pub struct NetToolState {
    jobs: Mutex<HashMap<String, u32>>,
    cancelled: Mutex<HashSet<String>>,
    next: AtomicU64,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct OutputChunk {
    id: String,
    stream: &'static str,
    line: String,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DoneEvent {
    id: String,
    code: Option<i32>,
    cancelled: bool,
    error: Option<String>,
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

fn decode(bytes: &[u8]) -> String {
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
    let allowed = target.chars().all(|c| {
        c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '-' | '_' | '[' | ']' | '%')
    });
    if !allowed {
        return Err("target may only contain letters, digits, dots, dashes, underscores and colons".to_string());
    }
    Ok(target.to_string())
}

#[cfg(windows)]
fn platform_command(
    kind: &str,
    count: u32,
    hops: u32,
) -> Result<(&'static str, Vec<String>), String> {
    match kind {
        "ping" => Ok((
            "ping",
            vec!["-n".to_string(), count.to_string(), "-w".to_string(), "1000".to_string()],
        )),
        "traceroute" => Ok((
            "tracert",
            vec![
                "-d".to_string(),
                "-h".to_string(),
                hops.to_string(),
                "-w".to_string(),
                "1000".to_string(),
            ],
        )),
        other => Err(format!("unsupported network tool: {other}")),
    }
}

#[cfg(not(windows))]
fn platform_command(
    kind: &str,
    count: u32,
    hops: u32,
) -> Result<(&'static str, Vec<String>), String> {
    match kind {
        "ping" => Ok((
            "ping",
            vec!["-c".to_string(), count.to_string(), "-W".to_string(), "2".to_string()],
        )),
        "traceroute" => Ok((
            "traceroute",
            vec!["-n".to_string(), "-m".to_string(), hops.to_string()],
        )),
        other => Err(format!("unsupported network tool: {other}")),
    }
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
                let line = text
                    .trim_end_matches(|c| c == '\r' || c == '\n')
                    .to_string();
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

/// Starts `ping` or `traceroute` against `target` and streams its output.
#[tauri::command]
pub async fn net_tool_start(
    app: AppHandle,
    state: State<'_, Arc<NetToolState>>,
    kind: String,
    target: String,
    count: Option<u32>,
    hops: Option<u32>,
) -> Result<String, String> {
    let state = state.inner().clone();
    let target = validate_target(&target)?;
    let count = count.unwrap_or(4).clamp(1, 20);
    let hops = hops.unwrap_or(20).clamp(1, 64);
    let (program, args) = platform_command(&kind, count, hops)?;

    let id = format!(
        "{kind}-{}",
        state.next.fetch_add(1, Ordering::Relaxed) + 1
    );

    let mut command = Command::new(program);
    command
        .args(args)
        .arg(&target)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        // The GUI build has no console, keep the helper from flashing one.
        command.creation_flags(0x0800_0000);
    }

    let mut child = command
        .spawn()
        .map_err(|err| format!("cannot start {program}: {err}"))?;
    let pid = child.id().unwrap_or_default();

    if let Some(stdout) = child.stdout.take() {
        tokio::spawn(pump(app.clone(), id.clone(), "out", stdout));
    }
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(pump(app.clone(), id.clone(), "err", stderr));
    }

    if let Ok(mut jobs) = state.jobs.lock() {
        jobs.insert(id.clone(), pid);
    }

    let finish_id = id.clone();
    let finish_app = app.clone();
    tokio::spawn(async move {
        let status = child.wait().await;
        let cancelled = state
            .cancelled
            .lock()
            .map(|mut set| set.remove(&finish_id))
            .unwrap_or(false);
        if let Ok(mut map) = state.jobs.lock() {
            map.remove(&finish_id);
        }
        let event = match status {
            Ok(status) => DoneEvent {
                id: finish_id,
                code: status.code(),
                cancelled,
                error: None,
            },
            Err(err) => DoneEvent {
                id: finish_id,
                code: None,
                cancelled,
                error: Some(err.to_string()),
            },
        };
        let _ = finish_app.emit(DONE_EVENT, event);
    });

    Ok(id)
}

/// Stops a running probe. The waiting task still emits the terminal event.
#[tauri::command]
pub fn net_tool_cancel(state: State<'_, Arc<NetToolState>>, id: String) -> Result<(), String> {
    let pid = state
        .jobs
        .lock()
        .map_err(|_| "job registry is poisoned".to_string())?
        .get(&id)
        .copied();
    let Some(pid) = pid else {
        return Ok(());
    };
    if let Ok(mut set) = state.cancelled.lock() {
        set.insert(id);
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
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
    value.get(key).and_then(|item| item.as_f64()).unwrap_or_default()
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
        format!("http://ip-api.com/json/{}?lang=zh-CN", encode_segment(target))
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
        version: if ip.contains(':') { "IPv6".to_string() } else { "IPv4".to_string() },
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
        let (program, args) = platform_command("ping", 1, 4).unwrap();
        let output = std::process::Command::new(program)
            .args(args)
            .arg("127.0.0.1")
            .output()
            .expect("ping runs");
        let text = decode(&output.stdout);
        assert!(text.contains("127.0.0.1"), "unexpected output: {text}");
    }

    #[test]
    fn rejects_unknown_kind() {
        assert!(platform_command("curl", 1, 4).is_err());
    }
}
