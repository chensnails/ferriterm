//! Local host network inventory, kept as pure parsing so every platform's
//! command output can be unit-tested without that platform being available.
//!
//! The Tauri layer only runs binaries and feeds their (locale-dependent) text
//! here. Nothing in this module talks to the OS, and no field is silently
//! dropped: an absent value stays empty/`None` and the front end renders an
//! explicit "not configured / not connected" marker for it.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkStatus {
    Up,
    Down,
    Disabled,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AdapterKind {
    Ethernet,
    Wifi,
    Virtual,
    Loopback,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WifiState {
    Connected,
    Disconnected,
    Disabled,
    Unknown,
}

/// One network adapter. Empty vectors/strings mean "reported but absent";
/// `status` explains why (down, disabled, or no address configured).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterInfo {
    pub name: String,
    pub description: String,
    pub kind: AdapterKind,
    pub status: LinkStatus,
    pub mac: String,
    pub ipv4: Vec<String>,
    pub ipv6: Vec<String>,
    pub gateways: Vec<String>,
    pub dns: Vec<String>,
    /// "interface" or "global": some platforms only expose system-wide DNS.
    pub dns_scope: String,
    /// Front-end translates these keys under `tools.netinfo.notes.*`.
    pub notes: Vec<String>,
}

impl AdapterInfo {
    fn new(name: &str, kind: AdapterKind) -> Self {
        Self {
            name: name.to_string(),
            description: String::new(),
            kind,
            status: LinkStatus::Unknown,
            mac: String::new(),
            ipv4: Vec::new(),
            ipv6: Vec::new(),
            gateways: Vec::new(),
            dns: Vec::new(),
            dns_scope: "interface".to_string(),
            notes: Vec::new(),
        }
    }
}

/// Signal quality of one WLAN interface. `signal_percent` is derived from RSSI
/// on platforms that only report dBm; `rssi_dbm` keeps the original reading.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WifiInfo {
    pub interface: String,
    pub ssid: String,
    pub bssid: String,
    pub mac: String,
    pub state: WifiState,
    pub signal_percent: Option<u8>,
    pub rssi_dbm: Option<i32>,
    pub rx_rate_mbps: Option<f64>,
    pub tx_rate_mbps: Option<f64>,
    pub band: String,
    pub notes: Vec<String>,
}

impl WifiInfo {
    fn new(interface: &str) -> Self {
        Self {
            interface: interface.to_string(),
            ssid: String::new(),
            bssid: String::new(),
            mac: String::new(),
            state: WifiState::Unknown,
            signal_percent: None,
            rssi_dbm: None,
            rx_rate_mbps: None,
            tx_rate_mbps: None,
            band: String::new(),
            notes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerConnection {
    pub protocol: String,
    pub local_addr: String,
    pub local_port: u16,
    pub remote_addr: String,
    pub remote_port: u16,
    pub state: String,
    pub pid: u32,
    pub process: String,
}

/// Whether a collector ran successfully. A failed collector is reported rather
/// than quietly producing an empty section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceReport {
    /// "adapters" | "wifi" | "connections"
    pub source: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalNetSnapshot {
    /// Unix seconds, so the UI can show when the snapshot was taken.
    pub generated_at: u64,
    pub platform: String,
    pub adapters: Vec<AdapterInfo>,
    pub wifi: Vec<WifiInfo>,
    pub connections: Vec<PeerConnection>,
    pub sources: Vec<SourceReport>,
}

impl LocalNetSnapshot {
    pub fn empty(platform: &str) -> Self {
        Self {
            generated_at: 0,
            platform: platform.to_string(),
            adapters: Vec::new(),
            wifi: Vec::new(),
            connections: Vec::new(),
            sources: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Shared text helpers
// ---------------------------------------------------------------------------

/// `Physical Address . . . :` and `物理地址. . . :` both become `physicaladdress`
/// / `物理地址`. Locale-specific keys are matched with `contains` so small
/// spacing or wording differences never lose a field.
fn normalize_key(left: &str) -> String {
    left.chars()
        .filter(|c| !matches!(c, '.' | ' ' | '\t' | '\u{3000}'))
        .collect::<String>()
        .to_ascii_lowercase()
}

fn split_colon(line: &str) -> Option<(&str, &str)> {
    let index = line.find(':')?;
    Some((line[..index].trim(), line[index + 1..].trim()))
}

fn strip_paren(value: &str) -> &str {
    match value.find('(') {
        Some(index) => value[..index].trim(),
        None => value.trim(),
    }
}

fn is_hex_pair(bytes: &[u8], at: usize) -> bool {
    bytes.get(at).is_some_and(u8::is_ascii_hexdigit)
        && bytes.get(at + 1).is_some_and(u8::is_ascii_hexdigit)
}

/// Accepts `00-E0-4C-13-03-00` and `10:6f:d9:75:d3:03`, normalises to dashes.
fn mac_token(text: &str) -> Option<String> {
    for token in text.split(|c: char| !(c.is_ascii_hexdigit() || c == '-' || c == ':')) {
        let bytes = token.as_bytes();
        if bytes.len() != 17 {
            continue;
        }
        let separator = bytes[2];
        if separator != b'-' && separator != b':' {
            continue;
        }
        let mut ok = true;
        for group in 0..6 {
            if !is_hex_pair(bytes, group * 3) {
                ok = false;
                break;
            }
            if group < 5 && bytes[group * 3 + 2] != separator {
                ok = false;
                break;
            }
        }
        if ok {
            return Some(token.to_ascii_uppercase().replace(':', "-"));
        }
    }
    None
}

fn valid_ipv4(token: &str) -> bool {
    let parts: Vec<&str> = token.split('.').collect();
    parts.len() == 4
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.len() <= 3 && part.parse::<u8>().is_ok())
}

/// Pulls IPv4 literals out of a value such as `192.168.8.136(首选)`.
pub fn ipv4_tokens(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    for ch in text.chars().chain(std::iter::once(' ')) {
        if ch.is_ascii_digit() || ch == '.' {
            current.push(ch);
        } else {
            if valid_ipv4(&current) && !out.contains(&current) {
                out.push(current.clone());
            }
            current.clear();
        }
    }
    out
}

pub fn ipv6_tokens(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for token in text.split_whitespace() {
        let clean = strip_paren(token).trim_end_matches(',');
        if clean.contains(':')
            && clean
                .chars()
                .all(|c| c.is_ascii_hexdigit() || matches!(c, ':' | '%' | '.'))
        {
            let value = clean.to_string();
            if !out.contains(&value) {
                out.push(value);
            }
        }
    }
    out
}

fn push_unique(list: &mut Vec<String>, values: impl IntoIterator<Item = String>) {
    for value in values {
        if !list.contains(&value) {
            list.push(value);
        }
    }
}

fn first_number(text: &str) -> Option<f64> {
    let mut current = String::new();
    for ch in text.chars().chain(std::iter::once(' ')) {
        if ch.is_ascii_digit() || ch == '.' {
            current.push(ch);
        } else if !current.is_empty() {
            return current.parse::<f64>().ok();
        }
    }
    None
}

/// `780 Mbps`, `866.7 MBit/s`, `1.2 Gbps` all become Mbps.
fn rate_mbps(text: &str) -> Option<f64> {
    let value = first_number(text)?;
    let lower = text.to_ascii_lowercase();
    let scale = if lower.contains("gbps") || lower.contains("gbit") {
        1000.0
    } else if lower.contains("kbps") || lower.contains("kbit") {
        0.001
    } else {
        1.0
    };
    Some((value * scale * 10.0).round() / 10.0)
}

/// Common conversion for platforms that only expose RSSI. -100 dBm is unusable
/// and -50 dBm or better is full strength.
pub fn signal_percent_from_dbm(dbm: i32) -> u8 {
    ((dbm + 100) * 2).clamp(0, 100) as u8
}

fn classify_adapter(header: &str) -> AdapterKind {
    let lower = header.to_ascii_lowercase();
    if lower.contains("loopback") || header.contains("环回") {
        AdapterKind::Loopback
    } else if header.contains("无线局域网")
        || lower.contains("wireless")
        || lower.contains("wi-fi")
        || lower.contains("wifi")
    {
        AdapterKind::Wifi
    } else if header.contains("未知")
        || header.contains("隧道")
        || lower.contains("unknown")
        || lower.contains("tunnel")
        || lower.contains("virtual")
        || lower.contains("vethernet")
        || lower.contains("bluetooth")
        || header.contains("蓝牙")
        || lower.contains("wintun")
        || lower.contains("tap")
    {
        AdapterKind::Virtual
    } else if header.contains("以太网") || lower.contains("ethernet") {
        AdapterKind::Ethernet
    } else {
        AdapterKind::Other
    }
}

fn adapter_display_name(header: &str) -> String {
    if let Some(index) = header.rfind("适配器 ") {
        return header[index + "适配器 ".len()..].trim().to_string();
    }
    let lower = header.to_ascii_lowercase();
    if let Some(index) = lower.rfind(" adapter ") {
        return header[index + " adapter ".len()..].trim().to_string();
    }
    header.trim().to_string()
}

fn normalize_state(state: &str) -> String {
    let upper = state.trim().to_ascii_uppercase().replace('-', "_");
    match upper.as_str() {
        "ESTAB" | "ESTABLISHED" => "ESTABLISHED".to_string(),
        "LISTEN" | "LISTENING" => "LISTEN".to_string(),
        "SYN_SENT" => "SYN_SENT".to_string(),
        "SYN_RECV" => "SYN_RECEIVED".to_string(),
        "TIME_WAIT" | "TIMEWAIT" => "TIME_WAIT".to_string(),
        "CLOSE_WAIT" | "CLOSEWAIT" => "CLOSE_WAIT".to_string(),
        "FIN_WAIT_1" | "FIN_WAIT1" => "FIN_WAIT_1".to_string(),
        "FIN_WAIT_2" | "FIN_WAIT2" => "FIN_WAIT_2".to_string(),
        "LAST_ACK" => "LAST_ACK".to_string(),
        "CLOSING" => "CLOSING".to_string(),
        "UNKNOWN" | "" => "UNKNOWN".to_string(),
        other => other.to_string(),
    }
}

/// Handles `1.2.3.4:443`, `[::1]:443`, `*:*` and macOS `1.2.3.4.443`.
fn split_addr_port(raw: &str) -> (String, u16) {
    let raw = raw.trim();
    if raw.starts_with('[') {
        if let Some(end) = raw.find(']') {
            let addr = raw[1..end].to_string();
            let port = raw[end + 1..]
                .trim_start_matches(':')
                .parse::<u16>()
                .unwrap_or(0);
            return (addr, port);
        }
    }
    if let Some((addr, port)) = raw.rsplit_once(':') {
        if let Ok(value) = port.parse::<u16>() {
            return (addr.to_string(), value);
        }
        // `0.0.0.0:*` / `[::]:*` are listening wildcards with no peer port.
        if port == "*" && (valid_ipv4(addr) || addr.contains(':')) {
            return (addr.to_string(), 0);
        }
    }
    // macOS netstat prints IPv4 endpoints as four octets plus a dot and a port.
    if let Some((addr, port)) = raw.rsplit_once('.') {
        if valid_ipv4(addr) {
            return (addr.to_string(), port.parse::<u16>().unwrap_or(0));
        }
    }
    (raw.to_string(), 0)
}

fn has(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

// ---------------------------------------------------------------------------
// Windows: ipconfig /all
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum ConfigField {
    None,
    Ipv4,
    Ipv6,
    Gateway,
    Dns,
}

/// Parses both zh-CN and en-US `ipconfig /all`. Continuation lines (extra DNS
/// servers) are attached to the preceding field, which is where Windows prints
/// them without a key.
pub fn parse_windows_ipconfig(text: &str) -> Vec<AdapterInfo> {
    let mut adapters: Vec<AdapterInfo> = Vec::new();
    let mut last_field = ConfigField::None;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some((left, right)) = split_colon(line) else {
            // A bare continuation line: only IP-bearing fields use those.
            if let Some(adapter) = adapters.last_mut() {
                match last_field {
                    ConfigField::Dns => push_unique(&mut adapter.dns, ipv4_tokens(trimmed)),
                    ConfigField::Gateway => {
                        push_unique(&mut adapter.gateways, ipv4_tokens(trimmed));
                        push_unique(&mut adapter.gateways, ipv6_tokens(trimmed));
                    }
                    _ => {}
                }
            }
            continue;
        };

        let is_header = left.matches('.').count() < 4
            && (left.contains("适配器")
                || left.to_ascii_lowercase().contains("adapter"))
            && right.is_empty();
        if is_header {
            adapters.push(AdapterInfo::new(
                &adapter_display_name(left),
                classify_adapter(left),
            ));
            last_field = ConfigField::None;
            continue;
        }

        let key = normalize_key(left);
        let Some(adapter) = adapters.last_mut() else {
            continue;
        };
        last_field = ConfigField::None;

        if has(&key, &["物理地址", "physicaladdress"]) {
            if let Some(mac) = mac_token(right) {
                adapter.mac = mac;
            }
        } else if has(&key, &["描述", "description"]) {
            adapter.description = right.to_string();
        } else if has(&key, &["媒体状态", "mediastate"]) {
            let lower = right.to_ascii_lowercase();
            if has(right, &["禁用"]) || lower.contains("disabled") {
                adapter.status = LinkStatus::Disabled;
            } else if has(right, &["断开"]) || lower.contains("disconnected") {
                adapter.status = LinkStatus::Down;
            }
        } else if has(&key, &["默认网关", "defaultgateway"]) {
            push_unique(&mut adapter.gateways, ipv4_tokens(right));
            push_unique(&mut adapter.gateways, ipv6_tokens(right));
            last_field = ConfigField::Gateway;
        } else if has(&key, &["dns服务器", "dnsservers"]) {
            push_unique(&mut adapter.dns, ipv4_tokens(right));
            push_unique(&mut adapter.dns, ipv6_tokens(right));
            last_field = ConfigField::Dns;
        } else if has(&key, &["ipv4地址", "ipv4address", "ip地址", "ipaddress"])
            || (key.starts_with("ipv4") && key.contains("地址"))
        {
            push_unique(&mut adapter.ipv4, ipv4_tokens(right));
            last_field = ConfigField::Ipv4;
        } else if has(
            &key,
            &[
                "ipv6地址",
                "ipv6address",
                "本地链接ipv6地址",
                "linklocalipv6address",
                "临时ipv6地址",
                "temporaryipv6address",
            ],
        ) {
            push_unique(&mut adapter.ipv6, ipv6_tokens(right));
            last_field = ConfigField::Ipv6;
        }
    }

    for adapter in &mut adapters {
        if adapter.status == LinkStatus::Unknown {
            adapter.status = if adapter.ipv4.is_empty() && adapter.ipv6.is_empty() {
                LinkStatus::Down
            } else {
                LinkStatus::Up
            };
        }
    }
    adapters
}

// ---------------------------------------------------------------------------
// Windows: netsh wlan show interfaces
// ---------------------------------------------------------------------------

/// Both locales put an English alias in parentheses for the rate/signal keys
/// (`接收速率(Receive rate)`), so matching either spelling is enough.
pub fn parse_windows_wlan(text: &str) -> Vec<WifiInfo> {
    let mut out: Vec<WifiInfo> = Vec::new();
    let mut current: Option<WifiInfo> = None;

    for line in text.lines() {
        let Some((left, right)) = split_colon(line) else {
            continue;
        };
        let key = normalize_key(left);
        if key.is_empty() {
            continue;
        }
        if key == "名称" || key == "name" {
            if let Some(previous) = current.take() {
                out.push(previous);
            }
            current = Some(WifiInfo::new(right));
            continue;
        }
        let Some(wifi) = current.as_mut() else {
            continue;
        };

        if key == "ssid" {
            wifi.ssid = right.to_string();
        } else if key == "bssid" {
            wifi.bssid = mac_token(right).unwrap_or_else(|| right.to_string());
        } else if has(&key, &["物理地址", "physicaladdress"]) {
            wifi.mac = mac_token(right).unwrap_or_default();
        } else if has(&key, &["无线电状态", "radiostatus"]) {
            let lower = right.to_ascii_lowercase();
            if has(right, &["硬件 关"]) || lower.contains("hardware off") {
                wifi.state = WifiState::Disabled;
            }
        } else if has(&key, &["状态", "state"]) {
            let lower = right.to_ascii_lowercase();
            if has(right, &["断开"]) || lower.contains("disconnected") {
                wifi.state = WifiState::Disconnected;
            } else if has(right, &["已连接", "连接"]) || lower.contains("connected") {
                wifi.state = WifiState::Connected;
            }
        } else if has(&key, &["信号", "signal"]) && !has(&key, &["无线电"]) {
            if let Some(percent) = first_number(right) {
                wifi.signal_percent = Some(percent.clamp(0.0, 100.0) as u8);
            }
        } else if has(&key, &["接收速率", "receiverate", "receive rate"]) {
            wifi.rx_rate_mbps = rate_mbps(right);
        } else if has(&key, &["传输速率", "transmitrate", "transmit rate"]) {
            wifi.tx_rate_mbps = rate_mbps(right);
        } else if has(&key, &["通道", "channel"]) {
            if let Some(channel) = first_number(right) {
                wifi.band = if channel >= 5000.0 || channel >= 36.0 {
                    format!("5 GHz · {channel:.0}")
                } else {
                    format!("2.4 GHz · {channel:.0}")
                };
            }
        }
    }
    if let Some(last) = current {
        out.push(last);
    }

    for wifi in &mut out {
        if wifi.state == WifiState::Unknown {
            wifi.state = if wifi.ssid.is_empty() {
                WifiState::Disconnected
            } else {
                WifiState::Connected
            };
        }
        if wifi.state != WifiState::Connected {
            wifi.notes.push("notConnected".to_string());
        }
        if wifi.signal_percent.is_none() {
            wifi.notes.push("signalUnavailable".to_string());
        }
        if wifi.rx_rate_mbps.is_none() && wifi.tx_rate_mbps.is_none() {
            wifi.notes.push("rateUnavailable".to_string());
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Connections: Windows netstat / Linux ss / macOS netstat
// ---------------------------------------------------------------------------

pub fn parse_windows_netstat(text: &str) -> Vec<PeerConnection> {
    let mut out = Vec::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            continue;
        }
        let protocol = parts[0].to_ascii_uppercase();
        if !matches!(protocol.as_str(), "TCP" | "TCPV6" | "UDP" | "UDPV6") {
            continue;
        }
        let (local_addr, local_port) = split_addr_port(parts[1]);
        let (remote_addr, remote_port) = split_addr_port(parts[2]);
        let state = if protocol.starts_with("UDP") {
            "UDP".to_string()
        } else {
            normalize_state(parts[3])
        };
        let pid = parts.get(4).and_then(|p| p.parse::<u32>().ok()).unwrap_or(0);
        out.push(PeerConnection {
            protocol,
            local_addr,
            local_port,
            remote_addr,
            remote_port,
            state,
            pid,
            process: String::new(),
        });
    }
    out
}

/// `ss -H -tanp` keeps one fixed-width row per socket; `users:(("x",pid=1,..))`
/// carries both the process name and its pid.
pub fn parse_ss(text: &str) -> Vec<PeerConnection> {
    let mut out = Vec::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 {
            continue;
        }
        // `State Recv-Q ...` is the header when -H was not honoured.
        if parts[0].eq_ignore_ascii_case("state") || !parts[3].contains(':') {
            continue;
        }
        let state = normalize_state(parts[0]);
        if !state
            .chars()
            .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
        {
            continue;
        }
        let (local_addr, local_port) = split_addr_port(parts[3]);
        let (remote_addr, remote_port) = split_addr_port(parts[4]);
        let tail = parts[5..].join(" ");
        let pid = tail
            .split_once("pid=")
            .and_then(|(_, rest)| rest.split(|c: char| !c.is_ascii_digit()).next())
            .and_then(|digits| digits.parse::<u32>().ok())
            .unwrap_or(0);
        let process = tail
            .split_once("((\"")
            .and_then(|(_, rest)| rest.split_once('"').map(|(name, _)| name.to_string()))
            .unwrap_or_default();
        out.push(PeerConnection {
            protocol: if local_addr.contains(':') { "TCP6" } else { "TCP" }.to_string(),
            local_addr,
            local_port,
            remote_addr,
            remote_port,
            state,
            pid,
            process,
        });
    }
    out
}

/// macOS `netstat -an -p tcp` uses dotted ports (`1.2.3.4.443`).
pub fn parse_unix_netstat(text: &str) -> Vec<PeerConnection> {
    let mut out = Vec::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 6 || !parts[0].to_ascii_lowercase().starts_with("tcp") {
            continue;
        }
        let (local_addr, local_port) = split_addr_port(parts[3]);
        let (remote_addr, remote_port) = split_addr_port(parts[4]);
        out.push(PeerConnection {
            protocol: if parts[0].contains('6') || local_addr.contains(':') {
                "TCP6"
            } else {
                "TCP"
            }
            .to_string(),
            local_addr,
            local_port,
            remote_addr,
            remote_port,
            state: normalize_state(parts[5]),
            pid: 0,
            process: String::new(),
        });
    }
    out
}

// ---------------------------------------------------------------------------
// Linux: ip addr / ip route / resolvectl / iw
// ---------------------------------------------------------------------------

pub fn parse_linux_ip_addr(text: &str) -> Vec<AdapterInfo> {
    let mut out: Vec<AdapterInfo> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        let is_header = line.starts_with(|c: char| c.is_ascii_digit()) && line.contains(": <");
        if is_header {
            let name = line
                .splitn(3, ": ")
                .nth(1)
                .unwrap_or_default()
                .trim()
                .to_string();
            let lower = name.to_ascii_lowercase();
            let kind = if lower == "lo"
                || lower.starts_with("lo@")
                || line.contains("LOOPBACK")
            {
                AdapterKind::Loopback
            } else if lower.starts_with("wl") || lower.starts_with("wlan") || lower.contains("wlp") {
                AdapterKind::Wifi
            } else if has(&lower, &["docker", "veth", "virbr", "br-", "tun", "tap", "wg", "vmnet"]) {
                AdapterKind::Virtual
            } else {
                AdapterKind::Ethernet
            };
            let mut adapter = AdapterInfo::new(&name, kind);
            let upper = line.to_ascii_uppercase();
            adapter.status = if upper.contains(" STATE DOWN")
                || (upper.contains("<") && !upper.contains(",UP>") && !upper.contains("UP,"))
            {
                LinkStatus::Down
            } else if upper.contains(" STATE UP") || upper.contains("UP") {
                LinkStatus::Up
            } else {
                LinkStatus::Unknown
            };
            out.push(adapter);
            continue;
        }
        let Some(adapter) = out.last_mut() else {
            continue;
        };
        if let Some(rest) = trimmed.strip_prefix("link/") {
            if let Some(mac) = mac_token(rest) {
                adapter.mac = mac;
            }
        } else if let Some(rest) = trimmed.strip_prefix("inet ") {
            let value = strip_paren(rest.split_whitespace().next().unwrap_or_default());
            if let Some((addr, _)) = value.split_once('/') {
                if valid_ipv4(addr) {
                    push_unique(&mut adapter.ipv4, [addr.to_string()]);
                }
            }
        } else if let Some(rest) = trimmed.strip_prefix("inet6 ") {
            let value = strip_paren(rest.split_whitespace().next().unwrap_or_default());
            if let Some((addr, _)) = value.split_once('/') {
                push_unique(&mut adapter.ipv6, [addr.to_string()]);
            }
        }
    }
    out
}

/// `default via 192.168.8.1 dev enp3s0 ...` is assigned to its device.
pub fn apply_linux_routes(adapters: &mut [AdapterInfo], text: &str) {
    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.first() != Some(&"default") {
            continue;
        }
        let gateway = parts
            .windows(2)
            .find(|w| w[0] == "via")
            .map(|w| w[1].to_string());
        let device = parts
            .windows(2)
            .find(|w| w[0] == "dev")
            .map(|w| w[1].to_string());
        if let (Some(gateway), Some(device)) = (gateway, device) {
            for adapter in adapters.iter_mut() {
                if adapter.name == device {
                    push_unique(&mut adapter.gateways, [gateway.clone()]);
                }
            }
        }
    }
}

pub fn parse_resolv_conf(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("nameserver ") {
            let value = value.split_whitespace().next().unwrap_or_default();
            if !value.is_empty() && !out.contains(&value.to_string()) {
                out.push(value.to_string());
            }
        }
    }
    out
}

/// `Link 2 (enp3s0): 192.168.8.1 8.8.8.8` — per-interface DNS where available.
pub fn apply_resolvectl_dns(adapters: &mut [AdapterInfo], text: &str) -> bool {
    let mut matched = false;
    for line in text.lines() {
        let Some((left, right)) = line.split_once(':') else {
            continue;
        };
        let Some(start) = left.find('(') else { continue };
        let Some(end) = left[start..].find(')') else {
            continue;
        };
        let name = &left[start + 1..start + end];
        let servers: Vec<String> = right
            .split_whitespace()
            .filter(|v| valid_ipv4(v) || v.contains(':'))
            .map(str::to_string)
            .collect();
        if servers.is_empty() {
            continue;
        }
        for adapter in adapters.iter_mut() {
            if adapter.name == name {
                push_unique(&mut adapter.dns, servers.clone());
                matched = true;
            }
        }
    }
    matched
}

/// `iw dev wlan0 link` output. `Not connected.` is a normal, reportable state.
pub fn parse_iw_link(text: &str, interface: &str) -> WifiInfo {
    let mut wifi = WifiInfo::new(interface);
    let lower_all = text.to_ascii_lowercase();
    if lower_all.contains("not connected") || lower_all.contains("not associated") {
        wifi.state = WifiState::Disconnected;
    } else if lower_all.contains("connected to") {
        wifi.state = WifiState::Connected;
        wifi.bssid = mac_token(text).unwrap_or_default();
    }
    for line in text.lines() {
        let Some((left, right)) = split_colon(line) else {
            continue;
        };
        let key = normalize_key(left);
        if key == "ssid" {
            wifi.ssid = right.to_string();
        } else if key == "signal" {
            if let Some(dbm) = first_number(right).map(|v| -v.abs() as i32) {
                wifi.rssi_dbm = Some(dbm);
                wifi.signal_percent = Some(signal_percent_from_dbm(dbm));
            }
        } else if has(&key, &["rxbitrate", "rx rate"]) {
            wifi.rx_rate_mbps = rate_mbps(right);
        } else if has(&key, &["txbitrate", "tx rate"]) {
            wifi.tx_rate_mbps = rate_mbps(right);
        } else if key == "freq" {
            if let Some(mhz) = first_number(right) {
                wifi.band = if mhz >= 5000.0 {
                    "5 GHz".to_string()
                } else {
                    "2.4 GHz".to_string()
                };
            }
        }
    }
    if wifi.state == WifiState::Unknown {
        wifi.state = if wifi.ssid.is_empty() {
            WifiState::Disconnected
        } else {
            WifiState::Connected
        };
    }
    if wifi.state != WifiState::Connected {
        wifi.notes.push("notConnected".to_string());
    }
    if wifi.signal_percent.is_none() {
        wifi.notes.push("signalUnavailable".to_string());
    }
    if wifi.rx_rate_mbps.is_none() && wifi.tx_rate_mbps.is_none() {
        wifi.notes.push("rateUnavailable".to_string());
    }
    wifi
}

// ---------------------------------------------------------------------------
// macOS: ifconfig / route / scutil / wdutil / networksetup
// ---------------------------------------------------------------------------

pub fn parse_macos_ifconfig(text: &str) -> Vec<AdapterInfo> {
    let mut out: Vec<AdapterInfo> = Vec::new();
    for line in text.lines() {
        if let Some((name, flags)) = line.split_once(": flags=") {
            if !line.starts_with(char::is_whitespace) && !name.is_empty() {
                let lower = name.to_ascii_lowercase();
                let kind = if lower.starts_with("lo") {
                    AdapterKind::Loopback
                } else if has(&lower, &["utun", "tun", "tap", "vmnet", "bridge", "vmenet"]) {
                    AdapterKind::Virtual
                } else if lower.starts_with("en") {
                    AdapterKind::Ethernet
                } else {
                    AdapterKind::Other
                };
                let mut adapter = AdapterInfo::new(name, kind);
                adapter.status = if flags.to_ascii_uppercase().contains("UP") {
                    LinkStatus::Up
                } else {
                    LinkStatus::Down
                };
                out.push(adapter);
                continue;
            }
        }
        let Some(adapter) = out.last_mut() else {
            continue;
        };
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("inet ") {
            let value = strip_paren(rest.split_whitespace().next().unwrap_or_default());
            if valid_ipv4(value) {
                push_unique(&mut adapter.ipv4, [value.to_string()]);
            }
        } else if let Some(rest) = trimmed.strip_prefix("inet6 ") {
            let value = strip_paren(rest.split_whitespace().next().unwrap_or_default());
            push_unique(&mut adapter.ipv6, [value.to_string()]);
        } else if let Some(rest) = trimmed.strip_prefix("ether ") {
            adapter.mac = mac_token(rest).unwrap_or_else(|| rest.to_string());
        } else if let Some(status) = trimmed.strip_prefix("status: ") {
            // "inactive" contains "active", so compare the whole token.
            adapter.status = if status.trim() == "active" {
                LinkStatus::Up
            } else {
                LinkStatus::Down
            };
        }
    }
    out
}

pub fn apply_macos_default_route(adapters: &mut [AdapterInfo], text: &str) {
    let mut gateway = String::new();
    let mut device = String::new();
    for line in text.lines() {
        if let Some((left, right)) = split_colon(line) {
            match left.trim().to_ascii_lowercase().as_str() {
                "gateway" => gateway = right.to_string(),
                "interface" => device = right.to_string(),
                _ => {}
            }
        }
    }
    if !gateway.is_empty() && !device.is_empty() {
        for adapter in adapters.iter_mut() {
            if adapter.name == device {
                push_unique(&mut adapter.gateways, [gateway.clone()]);
            }
        }
    }
}

pub fn parse_scutil_dns(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        if let Some((left, right)) = split_colon(line) {
            if left.trim().to_ascii_lowercase().starts_with("nameserver[") {
                let value = right.trim();
                if (valid_ipv4(value) || value.contains(':')) && !out.contains(&value.to_string()) {
                    out.push(value.to_string());
                }
            }
        }
    }
    out
}

/// `wdutil info` needs elevated privileges; `networksetup` still gives the
/// SSID. Both are merged into one WLAN record with honest gaps.
pub fn parse_macos_wifi(wdutil: &str, airport: &str, interface: &str) -> WifiInfo {
    let mut wifi = WifiInfo::new(interface);
    if let Some((_, ssid)) = airport.split_once(':') {
        let ssid = ssid.trim();
        if !ssid.is_empty() && !ssid.to_ascii_lowercase().contains("not associated") {
            wifi.ssid = ssid.to_string();
        }
    }
    for line in wdutil.lines().chain(airport.lines()) {
        let Some((left, right)) = split_colon(line) else {
            continue;
        };
        let key = normalize_key(left);
        if key == "ssid" {
            wifi.ssid = right.to_string();
        } else if has(&key, &["rssi", "signal"]) {
            if let Some(value) = first_number(right) {
                let dbm = -(value.abs() as i32);
                wifi.rssi_dbm = Some(dbm);
                wifi.signal_percent = Some(signal_percent_from_dbm(dbm));
            }
        } else if has(&key, &["transmitrate", "txrate"]) {
            wifi.tx_rate_mbps = rate_mbps(right);
        } else if has(&key, &["receiverate", "rxrate"]) {
            wifi.rx_rate_mbps = rate_mbps(right);
        }
    }
    wifi.state = if wifi.ssid.is_empty() {
        WifiState::Disconnected
    } else {
        WifiState::Connected
    };
    if wifi.state != WifiState::Connected {
        wifi.notes.push("notConnected".to_string());
    }
    if wifi.signal_percent.is_none() {
        wifi.notes.push("signalUnavailable".to_string());
        wifi.notes.push("needsElevation".to_string());
    }
    if wifi.rx_rate_mbps.is_none() && wifi.tx_rate_mbps.is_none() {
        wifi.notes.push("rateUnavailable".to_string());
    }
    wifi
}

/// Marks gaps on adapters so the UI can label them instead of showing a blank
/// row that looks like a rendering failure.
pub fn annotate_gaps(adapters: &mut [AdapterInfo]) {
    for adapter in adapters.iter_mut() {
        if adapter.mac.is_empty() {
            adapter.notes.push("noMac".to_string());
        }
        if adapter.ipv4.is_empty() && adapter.ipv6.is_empty() {
            adapter.notes.push("noAddress".to_string());
        }
        if adapter.gateways.is_empty() {
            adapter.notes.push("noGateway".to_string());
        }
        if adapter.dns.is_empty() {
            adapter.notes.push("noDns".to_string());
        }
        if adapter.dns_scope == "global" {
            adapter.notes.push("dnsGlobal".to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZH_IPCONFIG: &str = r#"
Windows IP 配置

以太网适配器 以太网 5:

   媒体状态  . . . . . . . . . . . . : 媒体已断开连接
   描述. . . . . . . . . . . . . . . : Realtek USB GbE Family Controller #3
   物理地址. . . . . . . . . . . . . : 00-E0-4C-13-03-00

以太网适配器 以太网:

   连接特定的 DNS 后缀 . . . . . . . : lan
   描述. . . . . . . . . . . . . . . : Intel(R) Ethernet Controller I225-V
   物理地址. . . . . . . . . . . . . : 58-47-CA-71-3A-E8
   本地链接 IPv6 地址. . . . . . . . : fe80::ebe7:aa5f:52e:99ce%19(首选)
   IPv4 地址 . . . . . . . . . . . . : 192.168.8.136(首选)
   默认网关. . . . . . . . . . . . . : 192.168.8.1
   DNS 服务器  . . . . . . . . . . . : 192.168.8.1
                                       8.8.8.8

无线局域网适配器 WLAN:

   媒体状态  . . . . . . . . . . . . : 媒体已断开连接
   描述. . . . . . . . . . . . . . . : RZ608 Wi-Fi 6E 80MHz
   物理地址. . . . . . . . . . . . . : 10-6F-D9-75-D3-03
"#;

    #[test]
    fn ipconfig_handles_multiple_adapters_and_missing_fields() {
        let list = parse_windows_ipconfig(ZH_IPCONFIG);
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].name, "以太网 5");
        assert_eq!(list[0].status, LinkStatus::Down);
        assert!(list[0].ipv4.is_empty());
        assert_eq!(list[0].mac, "00-E0-4C-13-03-00");

        assert_eq!(list[1].name, "以太网");
        assert_eq!(list[1].status, LinkStatus::Up);
        assert_eq!(list[1].ipv4, vec!["192.168.8.136"]);
        assert_eq!(list[1].ipv6, vec!["fe80::ebe7:aa5f:52e:99ce%19"]);
        assert_eq!(list[1].gateways, vec!["192.168.8.1"]);
        assert_eq!(list[1].dns, vec!["192.168.8.1", "8.8.8.8"]);

        assert_eq!(list[2].kind, AdapterKind::Wifi);
        assert_eq!(list[2].status, LinkStatus::Down);
    }

    const EN_IPCONFIG: &str = r#"
Windows IP Configuration

Wireless LAN adapter Wi-Fi:

   Media State . . . . . . . . . . . : Media disconnected
   Description . . . . . . . . . . . : Intel(R) Wi-Fi 6 AX201
   Physical Address. . . . . . . . . : 10-6F-D9-75-D3-03

Ethernet adapter Ethernet:

   Description . . . . . . . . . . . : Realtek PCIe GbE
   Physical Address. . . . . . . . . : 58-47-CA-71-3A-E8
   IPv4 Address. . . . . . . . . . . : 10.0.0.5(Preferred)
   Default Gateway . . . . . . . . . : 10.0.0.1
   DNS Servers . . . . . . . . . . . : 10.0.0.1
                                       1.1.1.1
"#;

    #[test]
    fn ipconfig_english_locale() {
        let list = parse_windows_ipconfig(EN_IPCONFIG);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "Wi-Fi");
        assert_eq!(list[0].kind, AdapterKind::Wifi);
        assert_eq!(list[0].status, LinkStatus::Down);
        assert_eq!(list[1].ipv4, vec!["10.0.0.5"]);
        assert_eq!(list[1].dns, vec!["10.0.0.1", "1.1.1.1"]);
    }

    const ZH_WLAN: &str = r#"
系统上有 1 个接口:

    名称                   : WLAN
    说明                   : RZ608 Wi-Fi 6E 80MHz
    物理地址               : 10:6f:d9:75:d3:03
    状态                   : 已断开连接
    无线电状态             : 硬件 开
                             软件 开
"#;

    const ZH_WLAN_CONNECTED: &str = r#"
系统上有 1 个接口:

    名称                   : WLAN
    SSID                   : Office-5G
    BSSID                  : aa:bb:cc:dd:ee:ff
    物理地址               : 10:6f:d9:75:d3:03
    状态                   : 已连接
    通道                   : 36
    接收速率(Receive rate)  : 780 Mbps
    传输速率(Transmit rate) : 780 Mbps
    信号(Signal)           : 88%
"#;

    #[test]
    fn wlan_disconnected_and_connected() {
        let off = parse_windows_wlan(ZH_WLAN);
        assert_eq!(off.len(), 1);
        assert_eq!(off[0].state, WifiState::Disconnected);
        assert_eq!(off[0].signal_percent, None);
        assert!(off[0].notes.contains(&"notConnected".to_string()));

        let on = parse_windows_wlan(ZH_WLAN_CONNECTED);
        assert_eq!(on[0].ssid, "Office-5G");
        assert_eq!(on[0].state, WifiState::Connected);
        assert_eq!(on[0].signal_percent, Some(88));
        assert_eq!(on[0].rx_rate_mbps, Some(780.0));
        assert_eq!(on[0].band, "5 GHz · 36");
        assert_eq!(on[0].bssid, "AA-BB-CC-DD-EE-FF");
    }

    const EN_WLAN: &str = r#"
There is 1 interface on the system:

    Name                   : Wi-Fi
    State                  : connected
    SSID                   : Guest
    Signal                 : 71%
    Receive rate           : 433.3 Mbps
    Transmit rate          : 390 Mbps
"#;

    #[test]
    fn wlan_english_locale() {
        let list = parse_windows_wlan(EN_WLAN);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].state, WifiState::Connected);
        assert_eq!(list[0].signal_percent, Some(71));
        assert_eq!(list[0].rx_rate_mbps, Some(433.3));
    }

    const WIN_NETSTAT: &str = r#"
活动连接

  协议  本地地址          外部地址        状态           PID
  TCP    0.0.0.0:135            0.0.0.0:0              LISTENING       1696
  TCP    127.0.0.1:7897         127.0.0.1:49275        ESTABLISHED     24240
  TCP    [::1]:5173             [::1]:60801            ESTABLISHED     22708
  TCP    10.0.0.5:51234         1.1.1.1:443            TIME_WAIT       0
"#;

    #[test]
    fn netstat_windows_ipv4_and_ipv6() {
        let list = parse_windows_netstat(WIN_NETSTAT);
        assert_eq!(list.len(), 4);
        assert_eq!(list[1].remote_addr, "127.0.0.1");
        assert_eq!(list[1].remote_port, 49275);
        assert_eq!(list[1].state, "ESTABLISHED");
        assert_eq!(list[1].pid, 24240);
        assert_eq!(list[2].local_addr, "::1");
        assert_eq!(list[2].local_port, 5173);
        assert_eq!(list[3].state, "TIME_WAIT");
    }

    const LINUX_IP: &str = r#"
1: lo: <LOOPBACK,UP,LOWER_UP> mtu 65536 state UNKNOWN group default
    link/loopback 00:00:00:00:00:00
    inet 127.0.0.1/8 scope host lo
    inet6 ::1/128 scope host
2: enp3s0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 state UP group default
    link/ether 58:47:ca:71:3a:e8
    inet 192.168.8.136/24 brd 192.168.8.255 scope global dynamic enp3s0
    inet6 fe80::ebe7/64 scope link
3: wlan0: <NO-CARRIER,BROADCAST,MULTICAST,UP> mtu 1500 state DOWN group default
    link/ether 10:6f:d9:75:d3:03
"#;

    #[test]
    fn linux_ip_addr_and_routes() {
        let mut list = parse_linux_ip_addr(LINUX_IP);
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].kind, AdapterKind::Loopback);
        assert_eq!(list[1].name, "enp3s0");
        assert_eq!(list[1].ipv4, vec!["192.168.8.136"]);
        assert_eq!(list[1].mac, "58-47-CA-71-3A-E8");
        assert_eq!(list[2].status, LinkStatus::Down);
        apply_linux_routes(
            &mut list,
            "default via 192.168.8.1 dev enp3s0 proto dhcp src 192.168.8.1 metric 100",
        );
        assert_eq!(list[1].gateways, vec!["192.168.8.1"]);
        assert!(list[0].gateways.is_empty());
        assert!(apply_resolvectl_dns(
            &mut list,
            "Link 2 (enp3s0): 192.168.8.1 8.8.8.8"
        ));
        assert_eq!(list[1].dns, vec!["192.168.8.1", "8.8.8.8"]);
    }

    const IW_LINK: &str = r#"
Connected to aa:bb:cc:dd:ee:ff (on wlan0)
        SSID: Office-5G
        freq: 5180
        signal: -45 dBm
        rx bitrate: 780.0 MBit/s
        tx bitrate: 866.7 MBit/s
"#;

    #[test]
    fn linux_iw_link_and_ss() {
        let wifi = parse_iw_link(IW_LINK, "wlan0");
        assert_eq!(wifi.state, WifiState::Connected);
        assert_eq!(wifi.ssid, "Office-5G");
        assert_eq!(wifi.rssi_dbm, Some(-45));
        assert_eq!(wifi.signal_percent, Some(100));
        assert_eq!(wifi.rx_rate_mbps, Some(780.0));
        assert_eq!(wifi.band, "5 GHz");

        let disconnected = parse_iw_link("Not connected.\n", "wlan0");
        assert_eq!(disconnected.state, WifiState::Disconnected);

        let conns = parse_ss(
            "ESTAB 0 0 127.0.0.1:5173 1.1.1.1:443 users:((\"ferriterm\",pid=123,fd=3))\nLISTEN 0 0 0.0.0.0:22 0.0.0.0:*\n",
        );
        assert_eq!(conns.len(), 2);
        assert_eq!(conns[0].state, "ESTABLISHED");
        assert_eq!(conns[0].remote_port, 443);
        assert_eq!(conns[0].pid, 123);
        assert_eq!(conns[0].process, "ferriterm");
        assert_eq!(conns[1].state, "LISTEN");
    }

    const MACOS_IFCONFIG: &str = r#"
lo0: flags=8049<UP,LOOPBACK,RUNNING,MULTICAST> mtu 16384
	inet 127.0.0.1 netmask 0xff000000
en0: flags=8863<UP,BROADCAST,RUNNING,SIMPLEX,MULTICAST> mtu 1500
	ether 10:6f:d9:75:d3:03
	inet 192.168.8.136 netmask 0xffffff00 broadcast 192.168.8.255
	inet6 fe80::1c2b prefixlen 64 scopeid 0x4
	status: active
en1: flags=8822<BROADCAST,SMART,SIMPLEX,MULTICAST> mtu 1500
	status: inactive
"#;

    #[test]
    fn macos_ifconfig_route_dns_wifi() {
        let mut list = parse_macos_ifconfig(MACOS_IFCONFIG);
        assert_eq!(list.len(), 3);
        assert_eq!(list[1].name, "en0");
        assert_eq!(list[1].ipv4, vec!["192.168.8.136"]);
        assert_eq!(list[1].mac, "10-6F-D9-75-D3-03");
        assert_eq!(list[2].status, LinkStatus::Down);
        apply_macos_default_route(
            &mut list,
            "   route to: default\ndestination: default\n    gateway: 192.168.8.1\n  interface: en0\n",
        );
        assert_eq!(list[1].gateways, vec!["192.168.8.1"]);
        assert_eq!(
            parse_scutil_dns("  nameserver[0] : 192.168.8.1\n  nameserver[1] : 8.8.8.8\n"),
            vec!["192.168.8.1", "8.8.8.8"]
        );
        let wifi = parse_macos_wifi(
            "SSID : Office-5G\nRSSI : -55\nTRANSMIT RATE : 866 Mbps\n",
            "Current Wi-Fi Network: Office-5G",
            "en0",
        );
        assert_eq!(wifi.state, WifiState::Connected);
        assert_eq!(wifi.rssi_dbm, Some(-55));
        assert_eq!(wifi.tx_rate_mbps, Some(866.0));

        let conns = parse_unix_netstat(
            "tcp4       0      0  192.168.8.136.54321    1.1.1.1.443           ESTABLISHED\n",
        );
        assert_eq!(conns.len(), 1);
        assert_eq!(conns[0].remote_addr, "1.1.1.1");
        assert_eq!(conns[0].remote_port, 443);
    }

    #[test]
    fn helpers_are_locale_independent() {
        assert_eq!(parse_resolv_conf("# comment\nnameserver 8.8.8.8\nnameserver 1.1.1.1\n").len(), 2);
        assert_eq!(signal_percent_from_dbm(-100), 0);
        assert_eq!(signal_percent_from_dbm(-75), 50);
        assert_eq!(rate_mbps("1.2 Gbps"), Some(1200.0));
        assert_eq!(mac_token("GUID 0e31ee02-8fb6-44b7 / 10:6f:d9:75:d3:03"), Some("10-6F-D9-75-D3-03".to_string()));
        assert_eq!(split_addr_port("1.2.3.4.443"), ("1.2.3.4".to_string(), 443));
    }
}
