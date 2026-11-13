//! Local network inventory for the toolbox. Every collector is independent:
//! if one binary is missing the other two sections still render, and the
//! failure is reported in `sources` instead of looking like an empty result.
//!
//! All process execution happens on a blocking thread because a Tauri command
//! must not stall the WebView while `netstat`/`ipconfig` are starting up.

use ft_core::{self, LocalNetSnapshot, SourceReport};
use std::process::{Command, Stdio};

/// Runs a platform binary and decodes its output. Windows console programs
/// emit either UTF-8 (`netsh`) or the ANSI/GBK code page (`ipconfig`, `netstat`),
/// which `decode` handles for both.
fn run(program: &str, args: &[&str]) -> Result<String, String> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        // The GUI build has no console; keep helpers from flashing one.
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let output = command
        .output()
        .map_err(|err| format!("cannot start {program}: {err}"))?;
    let text = crate::net_tools::decode(&output.stdout);
    if !output.status.success() && text.trim().is_empty() {
        let stderr = crate::net_tools::decode(&output.stderr);
        return Err(format!(
            "{program} exited with {:?}: {}",
            output.status.code(),
            stderr.trim()
        ));
    }
    Ok(text)
}

fn report(snapshot: &mut LocalNetSnapshot, source: &str, result: Result<String, String>) -> Option<String> {
    match result {
        Ok(text) => {
            snapshot.sources.push(SourceReport {
                source: source.to_string(),
                ok: true,
                detail: String::new(),
            });
            Some(text)
        }
        Err(err) => {
            snapshot.sources.push(SourceReport {
                source: source.to_string(),
                ok: false,
                detail: err,
            });
            None
        }
    }
}

fn platform_name() -> &'static str {
    #[cfg(windows)]
    {
        "windows"
    }
    #[cfg(target_os = "linux")]
    {
        "linux"
    }
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        "unknown"
    }
}

#[cfg(windows)]
fn collect_platform(snapshot: &mut LocalNetSnapshot) {
    if let Some(text) = report(snapshot, "adapters", run("ipconfig", &["/all"])) {
        snapshot.adapters = ft_core::local_net::parse_windows_ipconfig(&text);
    }

    if let Some(text) = report(
        snapshot,
        "wifi",
        run("netsh", &["wlan", "show", "interfaces"]),
    ) {
        snapshot.wifi = ft_core::local_net::parse_windows_wlan(&text);
    }

    if let Some(text) = report(snapshot, "connections", run("netstat", &["-ano", "-p", "tcp"])) {
        snapshot.connections = ft_core::local_net::parse_windows_netstat(&text);
    }
}

/// `iw dev` lists physical radios; every one of them gets its own link reading.
#[cfg(target_os = "linux")]
fn wireless_interfaces(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("Interface "))
        .map(|name| name.trim().to_string())
        .collect()
}

#[cfg(target_os = "linux")]
fn collect_platform(snapshot: &mut LocalNetSnapshot) {
    if let Some(text) = report(snapshot, "adapters", run("ip", &["addr"])) {
        snapshot.adapters = ft_core::local_net::parse_linux_ip_addr(&text);
    }

    // Routes are not part of `ip addr`, so they are merged afterwards. A
    // failure here degrades gateways only; adapters stay visible.
    let routes = run("ip", &["-4", "route", "show", "default"])
        .unwrap_or_default()
        + &run("ip", &["-6", "route", "show", "default"]).unwrap_or_default();
    ft_core::local_net::apply_linux_routes(&mut snapshot.adapters, &routes);

    let per_link = run("resolvectl", &["dns"]).ok();
    let matched = per_link
        .as_deref()
        .map(|text| ft_core::local_net::apply_resolvectl_dns(&mut snapshot.adapters, text))
        .unwrap_or(false);
    if !matched {
        let global = std::fs::read_to_string("/etc/resolv.conf")
            .map(|text| ft_core::local_net::parse_resolv_conf(&text))
            .unwrap_or_default();
        for adapter in &mut snapshot.adapters {
            adapter.dns.extend(global.iter().cloned());
            adapter.dns_scope = "global".to_string();
        }
    }

    match run("iw", &["dev"]) {
        Ok(list) => {
            let interfaces = wireless_interfaces(&list);
            for interface in interfaces {
                match run("iw", &["dev", &interface, "link"]) {
                    Ok(link) => snapshot
                        .wifi
                        .push(ft_core::local_net::parse_iw_link(&link, &interface)),
                    Err(err) => snapshot.sources.push(SourceReport {
                        source: "wifi".to_string(),
                        ok: false,
                        detail: format!("{interface}: {err}"),
                    }),
                }
            }
            snapshot.sources.push(SourceReport {
                source: "wifi".to_string(),
                ok: true,
                detail: String::new(),
            });
        }
        Err(err) => {
            // No wireless hardware is a legitimate state, not a crash; report
            // it as a note so the UI can say "no wireless interface".
            snapshot.sources.push(SourceReport {
                source: "wifi".to_string(),
                ok: false,
                detail: err,
            });
        }
    }

    match run("ss", &["-H", "-t", "-a", "-n", "-p"]) {
        Ok(text) => {
            snapshot.connections = ft_core::local_net::parse_ss(&text);
            snapshot.sources.push(SourceReport {
                source: "connections".to_string(),
                ok: true,
                detail: String::new(),
            });
        }
        Err(_) => {
            // Older iproute2 boxes only have netstat.
            if let Some(text) = report(snapshot, "connections", run("netstat", &["-tan"])) {
                snapshot.connections = ft_core::local_net::parse_unix_netstat(&text);
            }
        }
    }
}

/// `networksetup -listallhardwareports` maps the "Wi-Fi" port to a device such
/// as `en0`; without it `wdutil`/airport queries would have no target.
#[cfg(target_os = "macos")]
fn wireless_device(text: &str) -> Option<String> {
    let mut pending = false;
    for line in text.lines() {
        if let Some(port) = line.trim().strip_prefix("Hardware Port:") {
            pending = port.trim().eq_ignore_ascii_case("wi-fi");
        } else if pending {
            if let Some(device) = line.trim().strip_prefix("Device:") {
                return Some(device.trim().to_string());
            }
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn collect_platform(snapshot: &mut LocalNetSnapshot) {
    if let Some(text) = report(snapshot, "adapters", run("ifconfig", &["-a"])) {
        snapshot.adapters = ft_core::local_net::parse_macos_ifconfig(&text);
    }

    let routes = run("route", &["-n", "get", "default"]).unwrap_or_default()
        + &run("route", &["-n", "get", "-inet6", "default"]).unwrap_or_default();
    ft_core::local_net::apply_macos_default_route(&mut snapshot.adapters, &routes);

    let dns = run("scutil", &["--dns"])
        .map(|text| ft_core::local_net::parse_scutil_dns(&text))
        .unwrap_or_else(|_| {
            std::fs::read_to_string("/etc/resolv.conf")
                .map(|text| ft_core::local_net::parse_resolv_conf(&text))
                .unwrap_or_default()
        });
    for adapter in &mut snapshot.adapters {
        adapter.dns.extend(dns.iter().cloned());
        adapter.dns_scope = "global".to_string();
    }

    let device = run("networksetup", &["-listallhardwareports"])
        .ok()
        .and_then(|text| wireless_device(&text))
        .unwrap_or_else(|| "en0".to_string());
    // `wdutil` requires elevated privileges; that is expected, and its missing
    // RSSI is labelled rather than invented.
    let wdutil = run("wdutil", &["info"]).unwrap_or_default();
    match run("networksetup", &["-getairportnetwork", &device]) {
        Ok(airport) => {
            snapshot
                .wifi
                .push(ft_core::local_net::parse_macos_wifi(&wdutil, &airport, &device));
            snapshot.sources.push(SourceReport {
                source: "wifi".to_string(),
                ok: true,
                detail: String::new(),
            });
        }
        Err(err) => snapshot.sources.push(SourceReport {
            source: "wifi".to_string(),
            ok: false,
            detail: err,
        }),
    }

    if let Some(text) = report(snapshot, "connections", run("netstat", &["-an", "-p", "tcp"])) {
        snapshot.connections = ft_core::local_net::parse_unix_netstat(&text);
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
fn collect_platform(snapshot: &mut LocalNetSnapshot) {
    for source in ["adapters", "wifi", "connections"] {
        snapshot.sources.push(SourceReport {
            source: source.to_string(),
            ok: false,
            detail: "unsupported platform".to_string(),
        });
    }
}

fn collect() -> Result<LocalNetSnapshot, String> {
    let mut snapshot = LocalNetSnapshot::empty(platform_name());
    snapshot.generated_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|span| span.as_secs())
        .unwrap_or_default();
    collect_platform(&mut snapshot);
    ft_core::local_net::annotate_gaps(&mut snapshot.adapters);
    Ok(snapshot)
}

/// One-shot snapshot of adapters, WLAN quality and peer connections.
#[tauri::command]
pub async fn local_net_snapshot() -> Result<LocalNetSnapshot, String> {
    tokio::task::spawn_blocking(collect)
        .await
        .map_err(|err| err.to_string())?
}
