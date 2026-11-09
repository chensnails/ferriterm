//! Local system sampling for the monitor panel.

use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sysinfo::{LoadAvg, System};

const GPU_CACHE: Duration = Duration::from_secs(15);
const GPU_PROBE_TIMEOUT: Duration = Duration::from_millis(2500);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuInfo {
    pub name: String,
    pub utilization: Option<f32>,
    pub memory_used: Option<u64>,
    pub memory_total: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorSnapshot {
    pub cpu_usage: f32,
    pub cpu_brand: String,
    pub cpu_cores: usize,
    pub mem_used: u64,
    pub mem_total: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    pub load1: f64,
    pub load5: f64,
    pub load15: f64,
    pub uptime_secs: u64,
    pub host_name: String,
    pub os_name: String,
    pub kernel_version: String,
    pub gpus: Vec<GpuInfo>,
}

pub struct Monitor {
    system: System,
    gpus: Mutex<Option<(Instant, Vec<GpuInfo>)>>,
}

impl Monitor {
    pub fn new() -> Self {
        let mut system = System::new();
        system.refresh_cpu_all();
        system.refresh_memory();
        Self {
            system,
            gpus: Mutex::new(None),
        }
    }

    pub async fn sample(&mut self) -> MonitorSnapshot {
        // CPU usage is a delta between two refreshes.
        tokio::time::sleep(Duration::from_millis(150)).await;
        self.system.refresh_cpu_all();
        self.system.refresh_memory();
        let load: LoadAvg = System::load_average();
        let cpu_brand = self
            .system
            .cpus()
            .first()
            .map(|cpu| cpu.brand().trim().to_string())
            .unwrap_or_default();
        MonitorSnapshot {
            cpu_usage: self.system.global_cpu_usage(),
            cpu_brand,
            cpu_cores: self.system.cpus().len(),
            mem_used: self.system.used_memory(),
            mem_total: self.system.total_memory(),
            swap_used: self.system.used_swap(),
            swap_total: self.system.total_swap(),
            load1: load.one,
            load5: load.five,
            load15: load.fifteen,
            uptime_secs: System::uptime(),
            host_name: System::host_name().unwrap_or_default(),
            os_name: System::long_os_version()
                .or_else(System::name)
                .unwrap_or_default(),
            kernel_version: System::kernel_version().unwrap_or_default(),
            gpus: self.gpus().await,
        }
    }

    async fn gpus(&self) -> Vec<GpuInfo> {
        if let Ok(cache) = self.gpus.lock() {
            if let Some((at, list)) = cache.as_ref() {
                if at.elapsed() < GPU_CACHE {
                    return list.clone();
                }
            }
        }
        let list = tokio::task::spawn_blocking(detect_gpus)
            .await
            .unwrap_or_default();
        if let Ok(mut cache) = self.gpus.lock() {
            *cache = Some((Instant::now(), list.clone()));
        }
        list
    }
}

impl Default for Monitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Read-only probe for remote Linux hosts. Every line is `key payload`; the
/// script is a constant and nothing from the UI is interpolated into it.
pub const REMOTE_PROBE: &str = r#"
LC_ALL=C
export LC_ALL
echo "host $(uname -n 2>/dev/null)"
echo "os $(sed -n 's/^PRETTY_NAME=//p' /etc/os-release 2>/dev/null | head -n 1)"
echo "kernel $(uname -r 2>/dev/null)"
echo "cores $(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 0)"
echo "uptime $(cut -d' ' -f1 /proc/uptime 2>/dev/null)"
echo "load $(cut -d' ' -f1-3 /proc/loadavg 2>/dev/null)"
echo "mem $(awk '/^(MemTotal|MemAvailable|MemFree|Buffers|Cached|SwapTotal|SwapFree):/ {gsub(":", ""); printf "%s=%s ", $1, $2}' /proc/meminfo 2>/dev/null)"
echo "cpu $(head -n 1 /proc/stat 2>/dev/null)"
echo "disk $(df -Pk / 2>/dev/null | awk 'NR==2 {print $2, $3}')"
"#;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSnapshot {
    pub cpu_usage: f32,
    pub cpu_cores: usize,
    pub mem_used: u64,
    pub mem_total: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    pub load1: f64,
    pub load5: f64,
    pub load15: f64,
    pub uptime_secs: u64,
    pub host_name: String,
    pub os_name: String,
    pub kernel_version: String,
    pub disk_used: u64,
    pub disk_total: u64,
}

#[derive(Clone, Copy)]
struct CpuCounters {
    total: u64,
    idle: u64,
}

/// Keeps the previous CPU counters per session so usage is the delta between
/// polls instead of an average since boot.
#[derive(Default)]
pub struct RemoteMonitor {
    previous: Mutex<HashMap<String, CpuCounters>>,
}

impl RemoteMonitor {
    pub fn sample(&self, session_id: &str, output: &str) -> Result<RemoteSnapshot, String> {
        let mut fields = HashMap::new();
        for line in output.lines() {
            if let Some((key, value)) = line.split_once(' ') {
                fields.insert(key.trim(), value.trim());
            }
        }
        let counters = parse_cpu(fields.get("cpu").copied().unwrap_or("")).ok_or_else(|| {
            "the remote host does not report /proc/stat; remote monitoring supports Linux hosts"
                .to_string()
        })?;
        let cpu_usage = {
            let mut previous = self
                .previous
                .lock()
                .map_err(|_| "monitor state is poisoned".to_string())?;
            let usage = match previous.get(session_id) {
                Some(before) if counters.total > before.total => {
                    let total = counters.total - before.total;
                    let idle = counters.idle.saturating_sub(before.idle);
                    ((total.saturating_sub(idle)) as f32 / total as f32) * 100.0
                }
                _ => 0.0,
            };
            previous.insert(session_id.to_string(), counters);
            usage.round()
        };
        let mem = parse_pairs(fields.get("mem").copied().unwrap_or(""));
        let mem_total = mem.get("MemTotal").copied().unwrap_or(0) * 1024;
        if mem_total == 0 {
            return Err("the remote host did not report /proc/meminfo".to_string());
        }
        let available = mem.get("MemAvailable").copied().unwrap_or_else(|| {
            mem.get("MemFree").copied().unwrap_or(0)
                + mem.get("Buffers").copied().unwrap_or(0)
                + mem.get("Cached").copied().unwrap_or(0)
        }) * 1024;
        let swap_total = mem.get("SwapTotal").copied().unwrap_or(0) * 1024;
        let swap_free = mem.get("SwapFree").copied().unwrap_or(0) * 1024;
        let load: Vec<f64> = numbers(fields.get("load").copied().unwrap_or(""));
        let disk: Vec<u64> = numbers(fields.get("disk").copied().unwrap_or(""));
        let os_name = fields
            .get("os")
            .copied()
            .unwrap_or("")
            .trim_matches('"')
            .to_string();
        Ok(RemoteSnapshot {
            cpu_usage,
            cpu_cores: fields
                .get("cores")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0),
            mem_used: mem_total.saturating_sub(available),
            mem_total,
            swap_used: swap_total.saturating_sub(swap_free),
            swap_total,
            load1: load.first().copied().unwrap_or(0.0),
            load5: load.get(1).copied().unwrap_or(0.0),
            load15: load.get(2).copied().unwrap_or(0.0),
            uptime_secs: fields
                .get("uptime")
                .and_then(|value| value.parse::<f64>().ok())
                .unwrap_or(0.0) as u64,
            host_name: fields.get("host").copied().unwrap_or("").to_string(),
            os_name,
            kernel_version: fields.get("kernel").copied().unwrap_or("").to_string(),
            disk_used: disk.get(1).copied().unwrap_or(0) * 1024,
            disk_total: disk.first().copied().unwrap_or(0) * 1024,
        })
    }

    /// Called when a session closes so a later reconnect starts fresh.
    pub fn forget(&self, session_id: &str) {
        if let Ok(mut previous) = self.previous.lock() {
            previous.remove(session_id);
        }
    }
}

/// `/proc/stat`'s first line: user nice system idle iowait irq softirq steal.
fn parse_cpu(line: &str) -> Option<CpuCounters> {
    let mut parts = line.split_whitespace();
    if parts.next()? != "cpu" {
        return None;
    }
    let values: Vec<u64> = parts.filter_map(|value| value.parse().ok()).collect();
    if values.len() < 4 {
        return None;
    }
    Some(CpuCounters {
        idle: values[3] + values.get(4).copied().unwrap_or(0),
        total: values.iter().take(8).sum(),
    })
}

fn parse_pairs(line: &str) -> HashMap<String, u64> {
    line.split_whitespace()
        .filter_map(|pair| pair.split_once('='))
        .filter_map(|(key, value)| value.parse().ok().map(|value| (key.to_string(), value)))
        .collect()
}

fn numbers<T: std::str::FromStr>(line: &str) -> Vec<T> {
    line.split_whitespace()
        .filter_map(|value| value.parse().ok())
        .collect()
}

fn run_probe(program: &str, args: &[&str]) -> Option<String> {
    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + GPU_PROBE_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(_) => return None,
        }
    }
    let output = child.wait_with_output().ok()?;
    String::from_utf8(output.stdout).ok()
}

#[cfg(target_os = "macos")]
fn detect_gpus() -> Vec<GpuInfo> {
    run_probe("system_profiler", &["SPDisplaysDataType", "-json"])
        .map(|raw| parse_system_profiler(&raw))
        .unwrap_or_default()
}

#[cfg(target_os = "linux")]
fn detect_gpus() -> Vec<GpuInfo> {
    if let Some(raw) = run_probe(
        "nvidia-smi",
        &[
            "--query-gpu=name,utilization.gpu,memory.used,memory.total",
            "--format=csv,noheader,nounits",
        ],
    ) {
        let parsed = parse_nvidia_smi(&raw);
        if !parsed.is_empty() {
            return parsed;
        }
    }
    run_probe("lspci", &[])
        .map(|raw| parse_lspci(&raw))
        .unwrap_or_default()
}

#[cfg(target_os = "windows")]
fn detect_gpus() -> Vec<GpuInfo> {
    run_probe(
        "powershell",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-CimInstance Win32_VideoController | Select-Object Name,AdapterRAM | ConvertTo-Json -Compress",
        ],
    )
    .map(|raw| parse_windows_gpus(&raw))
    .unwrap_or_default()
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn detect_gpus() -> Vec<GpuInfo> {
    Vec::new()
}

#[cfg(any(test, target_os = "macos"))]
fn parse_memory_label(raw: &str) -> Option<u64> {
    let trimmed = raw.trim();
    let number: String = trimmed
        .chars()
        .take_while(|ch| ch.is_ascii_digit() || *ch == '.')
        .collect();
    let value: f64 = number.parse().ok()?;
    let unit = trimmed[number.len()..].trim().to_ascii_lowercase();
    let factor = if unit.starts_with("gb") || unit.starts_with("gib") {
        1024f64.powi(3)
    } else if unit.starts_with("mb") || unit.starts_with("mib") {
        1024f64.powi(2)
    } else if unit.starts_with("kb") || unit.starts_with("kib") {
        1024f64
    } else {
        1f64
    };
    Some((value * factor) as u64)
}

#[cfg(any(test, target_os = "macos"))]
fn parse_system_profiler(raw: &str) -> Vec<GpuInfo> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return Vec::new();
    };
    let Some(entries) = value
        .get("SPDisplaysDataType")
        .and_then(|items| items.as_array())
    else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            let name = ["sppci_model", "_name", "spdisplays_model"]
                .iter()
                .find_map(|key| entry.get(*key).and_then(|v| v.as_str()))
                .map(str::trim)
                .filter(|name| !name.is_empty())?
                .to_string();
            let memory_total = [
                "spdisplays_vram",
                "spdisplays_vram_shared",
                "spdisplays_vram_dynamic",
            ]
            .iter()
            .find_map(|key| entry.get(*key).and_then(|v| v.as_str()))
            .and_then(parse_memory_label);
            Some(GpuInfo {
                name,
                utilization: None,
                memory_used: None,
                memory_total,
            })
        })
        .collect()
}

#[cfg(any(test, target_os = "linux"))]
fn parse_nvidia_smi(raw: &str) -> Vec<GpuInfo> {
    raw.lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split(',').map(str::trim).collect();
            if fields.len() < 4 || fields[0].is_empty() {
                return None;
            }
            Some(GpuInfo {
                name: fields[0].to_string(),
                utilization: fields[1].parse::<f32>().ok(),
                memory_used: fields[2].parse::<u64>().ok().map(|mib| mib * 1024 * 1024),
                memory_total: fields[3].parse::<u64>().ok().map(|mib| mib * 1024 * 1024),
            })
        })
        .collect()
}

#[cfg(any(test, target_os = "linux"))]
fn parse_lspci(raw: &str) -> Vec<GpuInfo> {
    raw.lines()
        .filter(|line| line.contains("VGA compatible controller") || line.contains("3D controller"))
        .filter_map(|line| {
            let rest = line
                .split_once("controller: ")
                .map(|(_, rest)| rest)
                .unwrap_or(line);
            let name = rest.trim();
            (!name.is_empty()).then(|| GpuInfo {
                name: name.to_string(),
                utilization: None,
                memory_used: None,
                memory_total: None,
            })
        })
        .collect()
}

#[cfg(any(test, target_os = "windows"))]
fn parse_windows_gpus(raw: &str) -> Vec<GpuInfo> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return Vec::new();
    };
    let entries = match value {
        serde_json::Value::Array(items) => items,
        other => vec![other],
    };
    entries
        .into_iter()
        .filter_map(|entry| {
            let name = entry.get("Name")?.as_str()?.trim();
            if name.is_empty() {
                return None;
            }
            Some(GpuInfo {
                name: name.to_string(),
                utilization: None,
                memory_used: None,
                memory_total: entry.get("AdapterRAM").and_then(|v| v.as_u64()),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_system_profiler_displays() {
        let raw = r#"{"SPDisplaysDataType":[{"sppci_model":"Apple M2 Pro","spdisplays_vram_shared":"16 GB"},{"_name":"LG UltraFine"}]}"#;
        let gpus = parse_system_profiler(raw);
        assert_eq!(gpus.len(), 2);
        assert_eq!(gpus[0].name, "Apple M2 Pro");
        assert_eq!(gpus[0].memory_total, Some(16 * 1024 * 1024 * 1024));
        assert_eq!(gpus[1].name, "LG UltraFine");
    }

    #[test]
    fn parses_nvidia_smi_csv() {
        let gpus = parse_nvidia_smi(
            "NVIDIA GeForce RTX 3080, 42, 2048, 10240\nNVIDIA T400, [N/A], 10, 2048\n",
        );
        assert_eq!(gpus.len(), 2);
        assert_eq!(gpus[0].name, "NVIDIA GeForce RTX 3080");
        assert_eq!(gpus[0].utilization, Some(42.0));
        assert_eq!(gpus[0].memory_used, Some(2 * 1024 * 1024 * 1024));
        assert_eq!(gpus[1].utilization, None);
        assert_eq!(gpus[1].memory_total, Some(2 * 1024 * 1024 * 1024));
    }

    #[test]
    fn parses_lspci_graphics_lines() {
        let raw = "00:02.0 VGA compatible controller: Intel Corporation UHD Graphics 620 (rev 07)\n00:1f.3 Audio device: Intel Corporation\n";
        let gpus = parse_lspci(raw);
        assert_eq!(gpus.len(), 1);
        assert!(gpus[0].name.contains("UHD Graphics 620"));
    }

    #[test]
    fn parses_windows_adapter_json() {
        let raw = r#"[{"Name":"NVIDIA GeForce RTX 4070","AdapterRAM":8589934592}]"#;
        let gpus = parse_windows_gpus(raw);
        assert_eq!(gpus.len(), 1);
        assert_eq!(gpus[0].memory_total, Some(8 * 1024 * 1024 * 1024));
    }

    #[test]
    fn memory_labels_accept_units() {
        assert_eq!(parse_memory_label("8 GB"), Some(8 * 1024 * 1024 * 1024));
        assert_eq!(parse_memory_label("512 MB"), Some(512 * 1024 * 1024));
        assert_eq!(parse_memory_label("n/a"), None);
    }

    const REMOTE_SAMPLE: &str = r#"host nas-01
os "Armbian 24.8.1 bookworm"
kernel 6.6.44-current-sunxi64
cores 4
uptime 3623.55
load 0.15 0.09 0.04
mem MemTotal=2031244 MemAvailable=1523100 MemFree=610500 Buffers=84032 Cached=822568 SwapTotal=1048572 SwapFree=1048572 
cpu cpu  102030 800 15300 2010010 2400 0 320 0 0 0
disk 30517344 7311104
"#;

    #[test]
    fn parses_remote_probe_output() {
        let monitor = RemoteMonitor::default();
        let first = monitor.sample("s1", REMOTE_SAMPLE).expect("first sample");
        assert_eq!(first.host_name, "nas-01");
        assert_eq!(first.os_name, "Armbian 24.8.1 bookworm");
        assert_eq!(first.kernel_version, "6.6.44-current-sunxi64");
        assert_eq!(first.cpu_cores, 4);
        assert_eq!(first.uptime_secs, 3623);
        assert_eq!(first.mem_total, 2_031_244 * 1024);
        assert_eq!(first.mem_used, (2_031_244 - 1_523_100) * 1024);
        assert_eq!(first.swap_used, 0);
        assert_eq!(first.swap_total, 1_048_572 * 1024);
        assert_eq!(first.load1, 0.15);
        assert_eq!(first.disk_total, 30_517_344 * 1024);
        assert_eq!(first.disk_used, 7_311_104 * 1024);
        // No previous counters yet, so the first poll reports no usage.
        assert_eq!(first.cpu_usage, 0.0);
    }

    #[test]
    fn cpu_usage_is_the_delta_between_polls() {
        let monitor = RemoteMonitor::default();
        monitor.sample("s1", REMOTE_SAMPLE).expect("first sample");
        // user +200, idle +800 → 20% busy over the interval.
        let next = REMOTE_SAMPLE.replace(
            "cpu  102030 800 15300 2010010",
            "cpu  102230 800 15300 2010810",
        );
        let second = monitor.sample("s1", &next).expect("second sample");
        assert_eq!(second.cpu_usage, 20.0);
        // A reconnect must not reuse counters from the old session.
        monitor.forget("s1");
        let fresh = monitor.sample("s1", &next).expect("fresh sample");
        assert_eq!(fresh.cpu_usage, 0.0);
    }

    #[test]
    fn remote_sample_requires_proc_metrics() {
        let monitor = RemoteMonitor::default();
        let err = monitor.sample("win", "host box\nos \n").unwrap_err();
        assert!(err.contains("/proc/stat"));
        let err = monitor.sample("memless", "cpu cpu 1 2 3 4\n").unwrap_err();
        assert!(err.contains("/proc/meminfo"));
    }

    #[test]
    fn remote_probe_only_reads() {
        for forbidden in ["rm ", "sudo", "reboot", "shutdown", "mkfs", "dd if="] {
            assert!(
                !REMOTE_PROBE.contains(forbidden),
                "probe must not contain {forbidden}"
            );
        }
    }
}
