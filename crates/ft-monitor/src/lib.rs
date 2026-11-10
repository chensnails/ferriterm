//! Local system sampling for the monitor panel.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sysinfo::{Disks, LoadAvg, Networks, ProcessesToUpdate, System};

const GPU_CACHE: Duration = Duration::from_secs(15);
const GPU_PROBE_TIMEOUT: Duration = Duration::from_millis(2500);
/// Process tables are a "what is eating the box" list, not a full inventory.
const PROCESS_LIMIT: usize = 20;
const DISK_LIMIT: usize = 8;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuInfo {
    pub name: String,
    pub utilization: Option<f32>,
    pub memory_used: Option<u64>,
    pub memory_total: Option<u64>,
}

/// One mounted filesystem. `used` is total - available, which is what `df`
/// prints and what users compare against.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskUsage {
    pub mount: String,
    pub used: u64,
    pub total: u64,
}

/// Bytes per second over the last sampling interval.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetInterface {
    pub name: String,
    pub rx_rate: u64,
    pub tx_rate: u64,
    /// Bytes received since boot. Ranking needs a counter that does not sit at
    /// zero while the link is idle, and the tooltip shows it as a total.
    pub rx_total: u64,
    pub tx_total: u64,
    /// The one adapter that carries this host's traffic. Only it is shown in
    /// compact readouts; every other adapter stays in the full list.
    pub primary: bool,
}

/// Loopback, container bridges, hypervisor switches and VPN taps never carry
/// the traffic a user means by "the network", so they can be listed but must
/// not win the primary slot on an idle host.
fn is_virtual_link(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [
        "loopback",
        "vethernet",
        "virtualbox",
        "vmware",
        "docker",
        "veth",
        "virbr",
        "br-",
        "bridge",
        "tun",
        "tap",
        "utun",
        "bluetooth",
        "teredo",
        "isatap",
        "wintun",
        "wireguard",
        "npcap",
        "hyper-v",
        "default switch",
    ]
    .iter()
    .any(|part| lower.contains(part))
}

/// Marks the adapter the host routes through. A default route is authoritative
/// when the platform reports one; otherwise the busiest physical adapter wins,
/// and a virtual one is only used when nothing else exists.
fn mark_primary(net: &mut [NetInterface], default_route: Option<&str>) {
    let chosen = default_route
        .filter(|name| !name.is_empty())
        .and_then(|name| net.iter().position(|item| item.name == name))
        .or_else(|| {
            net.iter()
                .enumerate()
                .filter(|(_, item)| !is_virtual_link(&item.name))
                .max_by_key(|(_, item)| item.rx_total + item.tx_total)
                .map(|(index, _)| index)
        })
        .or_else(|| {
            net.iter()
                .enumerate()
                .max_by_key(|(_, item)| item.rx_total + item.tx_total)
                .map(|(index, _)| index)
        });
    if let Some(index) = chosen {
        net[index].primary = true;
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    /// Percent of one core, as `ps`/sysinfo report it.
    pub cpu: f32,
    pub mem: f32,
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
    pub disks: Vec<DiskUsage>,
    pub net: Vec<NetInterface>,
    pub processes: Vec<ProcessInfo>,
}

pub struct Monitor {
    system: System,
    networks: Networks,
    disks: Disks,
    net_at: Instant,
    gpus: Mutex<Option<(Instant, Vec<GpuInfo>)>>,
}

impl Monitor {
    pub fn new() -> Self {
        let mut system = System::new();
        system.refresh_cpu_all();
        system.refresh_memory();
        Self {
            system,
            networks: Networks::new_with_refreshed_list(),
            disks: Disks::new_with_refreshed_list(),
            net_at: Instant::now(),
            gpus: Mutex::new(None),
        }
    }

    pub async fn sample(&mut self) -> MonitorSnapshot {
        // CPU usage is a delta between two refreshes.
        tokio::time::sleep(Duration::from_millis(150)).await;
        self.system.refresh_cpu_all();
        self.system.refresh_memory();
        self.system.refresh_processes(ProcessesToUpdate::All, true);
        let load: LoadAvg = System::load_average();
        let cpu_brand = self
            .system
            .cpus()
            .first()
            .map(|cpu| cpu.brand().trim().to_string())
            .unwrap_or_default();
        let net = self.net_rates();
        let disks = self.disk_list();
        let processes = self.process_list();
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
            disks,
            net,
            processes,
        }
    }

    fn net_rates(&mut self) -> Vec<NetInterface> {
        let elapsed = self.net_at.elapsed().as_secs_f64().max(0.001);
        self.networks.refresh(true);
        self.net_at = Instant::now();
        let mut net: Vec<NetInterface> = self
            .networks
            .iter()
            .filter(|(name, _)| name.as_str() != "lo")
            .map(|(name, data)| NetInterface {
                name: name.clone(),
                rx_rate: (data.received() as f64 / elapsed) as u64,
                tx_rate: (data.transmitted() as f64 / elapsed) as u64,
                rx_total: data.total_received(),
                tx_total: data.total_transmitted(),
                primary: false,
            })
            .collect();
        mark_primary(&mut net, None);
        // The primary adapter leads even while it is idle, so the compact
        // readout never jumps between adapters on a quiet host.
        net.sort_by(|a, b| {
            b.primary
                .cmp(&a.primary)
                .then_with(|| (b.rx_rate + b.tx_rate).cmp(&(a.rx_rate + a.tx_rate)))
        });
        net.truncate(DISK_LIMIT);
        net
    }

    fn disk_list(&mut self) -> Vec<DiskUsage> {
        self.disks.refresh(true);
        self.disks
            .iter()
            .filter(|disk| disk.total_space() > 0)
            .take(DISK_LIMIT)
            .map(|disk| DiskUsage {
                mount: disk.mount_point().to_string_lossy().into_owned(),
                used: disk.total_space().saturating_sub(disk.available_space()),
                total: disk.total_space(),
            })
            .collect()
    }

    fn process_list(&self) -> Vec<ProcessInfo> {
        let total = self.system.total_memory().max(1);
        let mut processes: Vec<ProcessInfo> = self
            .system
            .processes()
            .values()
            .map(|process| ProcessInfo {
                pid: process.pid().as_u32(),
                name: process.name().to_string_lossy().into_owned(),
                cpu: process.cpu_usage(),
                mem: (process.memory() as f32 / total as f32) * 100.0,
            })
            .collect();
        processes.sort_by(|a, b| {
            b.cpu
                .partial_cmp(&a.cpu)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.pid.cmp(&b.pid))
        });
        processes.truncate(PROCESS_LIMIT);
        processes
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

/// Read-only probe for remote Linux hosts. Scalar lines are `key payload`;
/// table rows are prefixed (`diskrow`/`netrow`/`procrow`) so a single flat
/// stream carries every section. The script is a constant and nothing from
/// the UI is interpolated into it.
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
echo "route $(awk '$2 == "00000000" {print $1; exit}' /proc/net/route 2>/dev/null)"
df -Pk 2>/dev/null | awk 'NR > 1 && $2 > 0 {print "diskrow", $6, $3, $2}'
awk -F'[: ]+' 'NR > 2 && $2 != "lo" && $2 != "" {print "netrow", $2, $3, $11}' /proc/net/dev 2>/dev/null
ps -eo pid=,pcpu=,pmem=,comm= 2>/dev/null | head -n 40 | awk '{name = $4; for (i = 5; i <= NF; i++) name = name " " $i; print "procrow", $1, $2, $3, name}'
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
    pub disks: Vec<DiskUsage>,
    pub net: Vec<NetInterface>,
    pub processes: Vec<ProcessInfo>,
    /// Round trip of the probe itself, measured around the SSH exec.
    pub latency_ms: u64,
}

#[derive(Clone, Copy)]
struct CpuCounters {
    total: u64,
    idle: u64,
}

/// Everything the next poll needs from the previous one: CPU counters, the
/// per-interface byte counters, and when the sample was taken.
struct PrevSample {
    cpu: CpuCounters,
    net: HashMap<String, (u64, u64)>,
    at: Instant,
}

/// Keeps the previous sample per session so usage and network rates are
/// deltas between polls instead of averages since boot.
#[derive(Default)]
pub struct RemoteMonitor {
    previous: Mutex<HashMap<String, PrevSample>>,
    /// PIDs from the last rendered process list. The kill command only accepts
    /// these, so a stale or mistyped pid cannot signal an arbitrary process.
    visible: Mutex<HashMap<String, HashSet<u32>>>,
}

impl RemoteMonitor {
    pub fn sample(
        &self,
        session_id: &str,
        output: &str,
        latency_ms: u64,
    ) -> Result<RemoteSnapshot, String> {
        let mut fields: HashMap<&str, &str> = HashMap::new();
        let mut disks: Vec<DiskUsage> = Vec::new();
        let mut net_rows: Vec<(String, u64, u64)> = Vec::new();
        let mut processes: Vec<ProcessInfo> = Vec::new();
        for line in output.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("diskrow ") {
                let mut parts = rest.split_whitespace();
                let mount = parts.next().unwrap_or("").to_string();
                let used = parts
                    .next()
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(0);
                let total = parts
                    .next()
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(0);
                if total > 0 && disks.len() < DISK_LIMIT {
                    disks.push(DiskUsage {
                        mount,
                        used: used * 1024,
                        total: total * 1024,
                    });
                }
            } else if let Some(rest) = line.strip_prefix("netrow ") {
                let mut parts = rest.split_whitespace();
                let name = parts.next().unwrap_or("");
                let rx = parts
                    .next()
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(0);
                let tx = parts
                    .next()
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(0);
                if !name.is_empty() {
                    net_rows.push((name.to_string(), rx, tx));
                }
            } else if let Some(rest) = line.strip_prefix("procrow ") {
                let mut parts = rest.splitn(4, ' ');
                let pid = parts
                    .next()
                    .and_then(|value| value.trim().parse::<u32>().ok());
                let cpu = parts
                    .next()
                    .and_then(|value| value.trim().parse::<f32>().ok())
                    .unwrap_or(0.0);
                let mem = parts
                    .next()
                    .and_then(|value| value.trim().parse::<f32>().ok())
                    .unwrap_or(0.0);
                let name = parts.next().unwrap_or("").trim().to_string();
                if let Some(pid) = pid {
                    if !name.is_empty() {
                        processes.push(ProcessInfo {
                            pid,
                            name,
                            cpu,
                            mem,
                        });
                    }
                }
            } else if let Some((key, value)) = line.split_once(' ') {
                fields.insert(key.trim(), value.trim());
            }
        }
        let counters = parse_cpu(fields.get("cpu").copied().unwrap_or("")).ok_or_else(|| {
            "the remote host does not report /proc/stat; remote monitoring supports Linux hosts"
                .to_string()
        })?;
        let mem = parse_pairs(fields.get("mem").copied().unwrap_or(""));
        let mem_total = mem.get("MemTotal").copied().unwrap_or(0) * 1024;
        if mem_total == 0 {
            return Err("the remote host did not report /proc/meminfo".to_string());
        }

        let now = Instant::now();
        let mut guard = self
            .previous
            .lock()
            .map_err(|_| "monitor state is poisoned".to_string())?;
        let (prev_cpu, prev_net, prev_at) = match guard.get(session_id) {
            Some(sample) => (Some(sample.cpu), sample.net.clone(), Some(sample.at)),
            None => (None, HashMap::new(), None),
        };
        let elapsed = prev_at
            .map(|at| now.duration_since(at).as_secs_f64().max(0.001))
            .unwrap_or(0.0);
        let cpu_usage = match prev_cpu {
            Some(before) if counters.total > before.total => {
                let total = counters.total - before.total;
                let idle = counters.idle.saturating_sub(before.idle);
                (((total.saturating_sub(idle)) as f32 / total as f32) * 100.0).round()
            }
            _ => 0.0,
        };
        let mut net: Vec<NetInterface> = net_rows
            .iter()
            .map(|(name, rx, tx)| {
                let rates = match prev_net.get(name) {
                    Some((prev_rx, prev_tx))
                        if elapsed > 0.0 && *rx >= *prev_rx && *tx >= *prev_tx =>
                    {
                        (
                            ((rx - prev_rx) as f64 / elapsed) as u64,
                            ((tx - prev_tx) as f64 / elapsed) as u64,
                        )
                    }
                    _ => (0, 0),
                };
                NetInterface {
                    name: name.clone(),
                    rx_rate: rates.0,
                    tx_rate: rates.1,
                    rx_total: *rx,
                    tx_total: *tx,
                    primary: false,
                }
            })
            .collect();
        mark_primary(&mut net, fields.get("route").copied().map(str::trim));
        net.sort_by(|a, b| {
            b.primary
                .cmp(&a.primary)
                .then_with(|| (b.rx_rate + b.tx_rate).cmp(&(a.rx_rate + a.tx_rate)))
        });
        guard.insert(
            session_id.to_string(),
            PrevSample {
                cpu: counters,
                net: net_rows
                    .iter()
                    .map(|(name, rx, tx)| (name.clone(), (*rx, *tx)))
                    .collect(),
                at: now,
            },
        );
        drop(guard);

        processes.sort_by(|a, b| {
            b.cpu
                .partial_cmp(&a.cpu)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.pid.cmp(&b.pid))
        });
        processes.truncate(PROCESS_LIMIT);
        if let Ok(mut visible) = self.visible.lock() {
            visible.insert(
                session_id.to_string(),
                processes.iter().map(|process| process.pid).collect(),
            );
        }

        let available = mem.get("MemAvailable").copied().unwrap_or_else(|| {
            mem.get("MemFree").copied().unwrap_or(0)
                + mem.get("Buffers").copied().unwrap_or(0)
                + mem.get("Cached").copied().unwrap_or(0)
        }) * 1024;
        let swap_total = mem.get("SwapTotal").copied().unwrap_or(0) * 1024;
        let swap_free = mem.get("SwapFree").copied().unwrap_or(0) * 1024;
        let load: Vec<f64> = numbers(fields.get("load").copied().unwrap_or(""));
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
            disks,
            net,
            processes,
            latency_ms,
        })
    }

    /// A pid is killable only while it is still in the list the user saw.
    pub fn is_visible(&self, session_id: &str, pid: u32) -> bool {
        self.visible
            .lock()
            .map(|visible| {
                visible
                    .get(session_id)
                    .map(|pids| pids.contains(&pid))
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    }

    /// Called when a session closes so a later reconnect starts fresh.
    pub fn forget(&self, session_id: &str) {
        if let Ok(mut previous) = self.previous.lock() {
            previous.remove(session_id);
        }
        if let Ok(mut visible) = self.visible.lock() {
            visible.remove(session_id);
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
route eth0
diskrow / 7311104 30517344
diskrow /boot/efi 6144 99328
netrow eth0 1500000 400000
netrow docker0 5000 100
procrow 1567231 12.5 3.0 MainThread
procrow 812 0.4 1.2 nginx worker
"#;

    #[test]
    fn parses_remote_probe_output() {
        let monitor = RemoteMonitor::default();
        let first = monitor
            .sample("s1", REMOTE_SAMPLE, 42)
            .expect("first sample");
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
        assert_eq!(first.latency_ms, 42);
        assert_eq!(first.disks.len(), 2);
        assert_eq!(first.disks[0].mount, "/");
        assert_eq!(first.disks[0].total, 30_517_344 * 1024);
        assert_eq!(first.disks[0].used, 7_311_104 * 1024);
        // Process names keep their spaces and the list is CPU-sorted.
        assert_eq!(first.processes.len(), 2);
        assert_eq!(first.processes[0].pid, 1_567_231);
        assert_eq!(first.processes[0].name, "MainThread");
        assert_eq!(first.processes[1].name, "nginx worker");
        // No previous counters yet, so the first poll reports no usage.
        assert_eq!(first.cpu_usage, 0.0);
        assert!(first.net.iter().all(|iface| iface.rx_rate == 0));
        // The default route decides which adapter is "the" network, and only
        // one adapter may claim the slot.
        let primaries: Vec<&str> = first
            .net
            .iter()
            .filter(|iface| iface.primary)
            .map(|iface| iface.name.as_str())
            .collect();
        assert_eq!(primaries, vec!["eth0"]);
    }

    fn iface(name: &str, rx: u64, tx: u64) -> NetInterface {
        NetInterface {
            name: name.to_string(),
            rx_rate: 0,
            tx_rate: 0,
            rx_total: rx,
            tx_total: tx,
            primary: false,
        }
    }

    #[test]
    fn primary_adapter_follows_the_route_then_the_traffic() {
        // Without a route the busiest physical adapter wins, even on an idle
        // host, and a container bridge is never promoted over a real link.
        let mut net = vec![iface("docker0", 9_000_000, 10), iface("ens3", 1_000, 2_000)];
        mark_primary(&mut net, None);
        assert!(net[1].primary);
        assert!(!net[0].primary);

        // An explicit default route beats raw counters.
        let mut net = vec![iface("docker0", 9_000_000, 10), iface("ens3", 1_000, 2_000)];
        mark_primary(&mut net, Some("docker0"));
        assert!(net[0].primary);
        assert!(!net[1].primary);

        // A route naming an adapter that was not reported falls back to traffic.
        let mut net = vec![iface("ens3", 10, 10), iface("ens4", 20, 20)];
        mark_primary(&mut net, Some("ens9"));
        assert!(net[1].primary);
    }

    #[test]
    fn net_rates_are_deltas_between_polls() {
        let monitor = RemoteMonitor::default();
        monitor
            .sample("s1", REMOTE_SAMPLE, 10)
            .expect("first sample");
        let next = REMOTE_SAMPLE
            .replace("netrow eth0 1500000 400000", "netrow eth0 1800000 900000")
            .replace("netrow docker0 5000 100", "netrow docker0 5400 180");
        let second = monitor.sample("s1", &next, 10).expect("second sample");
        let eth0 = second.net.iter().find(|item| item.name == "eth0").unwrap();
        let docker0 = second
            .net
            .iter()
            .find(|item| item.name == "docker0")
            .unwrap();
        assert!(eth0.rx_rate > 0, "rx rate should grow with the counters");
        assert!(eth0.tx_rate > eth0.rx_rate, "tx grew by more bytes");
        assert!(docker0.rx_rate > 0);
    }

    #[test]
    fn only_sampled_pids_are_killable() {
        let monitor = RemoteMonitor::default();
        monitor
            .sample("s1", REMOTE_SAMPLE, 10)
            .expect("first sample");
        assert!(monitor.is_visible("s1", 812));
        assert!(!monitor.is_visible("s1", 1));
        assert!(!monitor.is_visible("other", 812));
        monitor.forget("s1");
        assert!(!monitor.is_visible("s1", 812));
    }

    #[test]
    fn cpu_usage_is_the_delta_between_polls() {
        let monitor = RemoteMonitor::default();
        monitor
            .sample("s1", REMOTE_SAMPLE, 10)
            .expect("first sample");
        // user +200, idle +800 → 20% busy over the interval.
        let next = REMOTE_SAMPLE.replace(
            "cpu  102030 800 15300 2010010",
            "cpu  102230 800 15300 2010810",
        );
        let second = monitor.sample("s1", &next, 10).expect("second sample");
        assert_eq!(second.cpu_usage, 20.0);
        // A reconnect must not reuse counters from the old session.
        monitor.forget("s1");
        let fresh = monitor.sample("s1", &next, 10).expect("fresh sample");
        assert_eq!(fresh.cpu_usage, 0.0);
    }

    #[test]
    fn remote_sample_requires_proc_metrics() {
        let monitor = RemoteMonitor::default();
        let err = monitor.sample("win", "host box\nos \n", 0).unwrap_err();
        assert!(err.contains("/proc/stat"));
        let err = monitor
            .sample("memless", "cpu cpu 1 2 3 4\n", 0)
            .unwrap_err();
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
