//! Serial-port transport. Sessions share the terminal plumbing with SSH and
//! PTY sessions: bytes read from the device are pushed through the same
//! `TerminalSink`, so recording and the webview events need no special case.

use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use ft_core::{CloseReason, GuardMap, SessionGuard, TerminalSink};
use serde::{Deserialize, Serialize};
use serial2::{CharSize, FlowControl, Parity, SerialPort, Settings, StopBits};

const MAX_PATH_LEN: usize = 256;
const MAX_BAUD_RATE: u32 = 4_000_000;
const READ_POLL: Duration = Duration::from_millis(100);
const WRITE_TIMEOUT: Duration = Duration::from_secs(2);
/// Opening a wedged COM port can block indefinitely on Windows. Move it off
/// the Tokio worker and bound it so the connect command always comes back.
const OPEN_TIMEOUT: Duration = Duration::from_secs(10);
/// A device that keeps returning zero bytes is gone (unplugged / suspended),
/// not idle. Bound the spin so the reader thread can report and exit.
const MAX_EMPTY_READS: u32 = 100;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SerialConfig {
    pub path: String,
    #[serde(default = "default_baud_rate")]
    pub baud_rate: u32,
    #[serde(default = "default_data_bits")]
    pub data_bits: u8,
    #[serde(default = "default_parity")]
    pub parity: String,
    #[serde(default = "default_stop_bits")]
    pub stop_bits: u8,
    #[serde(default = "default_flow_control")]
    pub flow_control: String,
}

fn default_baud_rate() -> u32 {
    115_200
}

fn default_data_bits() -> u8 {
    8
}

fn default_parity() -> String {
    "none".to_string()
}

fn default_stop_bits() -> u8 {
    1
}

fn default_flow_control() -> String {
    "none".to_string()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SerialPortInfo {
    pub path: String,
    pub name: String,
}

pub fn validate(config: &SerialConfig) -> Result<(), String> {
    if config.path.trim().is_empty()
        || config.path.len() > MAX_PATH_LEN
        || config.path.contains('\0')
    {
        return Err("serial port path is invalid".to_string());
    }
    if !(50..=MAX_BAUD_RATE).contains(&config.baud_rate) {
        return Err(format!("unsupported baud rate {}", config.baud_rate));
    }
    if !(5..=8).contains(&config.data_bits) {
        return Err("data bits must be 5, 6, 7 or 8".to_string());
    }
    if !matches!(config.parity.as_str(), "none" | "odd" | "even") {
        return Err(format!("unsupported parity {}", config.parity));
    }
    if !matches!(config.stop_bits, 1 | 2) {
        return Err("stop bits must be 1 or 2".to_string());
    }
    if !matches!(config.flow_control.as_str(), "none" | "rtscts" | "xonxoff") {
        return Err(format!("unsupported flow control {}", config.flow_control));
    }
    Ok(())
}

pub fn list_ports() -> Result<Vec<SerialPortInfo>, String> {
    let ports = SerialPort::available_ports().map_err(|e| e.to_string())?;
    let mut out: Vec<SerialPortInfo> = ports
        .into_iter()
        .filter_map(|path| {
            let name = path.file_name()?.to_string_lossy().into_owned();
            Some(SerialPortInfo {
                path: path.to_string_lossy().into_owned(),
                name,
            })
        })
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out.dedup_by(|a, b| a.path == b.path);
    Ok(out)
}

struct SerialSession {
    port: Arc<SerialPort>,
    guard: SessionGuard,
}

/// A plain `std` mutex is deliberate: the reader is a dedicated OS thread (a
/// blocking `read`), so it must be able to release its own registry entry
/// without needing a Tokio runtime context.
#[derive(Default, Clone)]
pub struct SerialRegistry {
    sessions: Arc<StdMutex<HashMap<String, SerialSession>>>,
    guards: GuardMap,
}

fn sessions_map<T>(lock: &Arc<StdMutex<T>>) -> Result<std::sync::MutexGuard<'_, T>, String> {
    lock.lock().map_err(|_| "serial registry is unavailable".to_string())
}

pub async fn open(
    sink: Arc<dyn TerminalSink>,
    registry: &SerialRegistry,
    id: String,
    config: SerialConfig,
) -> Result<(), String> {
    validate(&config)?;
    // Reserve the id before touching hardware. Two rapid double clicks can no
    // longer both win, and a close during the open is visible to us below.
    let Some(guard) = registry.guards.begin(&id, sink.clone()) else {
        return Err("a serial connection for this session is already in progress".to_string());
    };
    let mut port = match open_port(config.clone()).await {
        Ok(port) => port,
        Err(error) => {
            registry.guards.take_if(&id, &guard);
            return Err(error);
        }
    };
    if let Err(error) = port
        .set_read_timeout(READ_POLL)
        .and_then(|_| port.set_write_timeout(WRITE_TIMEOUT))
    {
        registry.guards.take_if(&id, &guard);
        return Err(format!("failed to configure {}: {error}", config.path));
    }
    if guard.is_stopped() || !registry.guards.is_current(&id, &guard) {
        registry.guards.take_if(&id, &guard);
        return Err("connection cancelled".to_string());
    }

    let port = Arc::new(port);
    // Publish the session before starting the reader: a device that fails
    // immediately must not emit a closed event before open() has returned.
    sessions_map(&registry.sessions)?.insert(
        id.clone(),
        SerialSession {
            port: port.clone(),
            guard: guard.clone(),
        },
    );

    let event_id = id.clone();
    let reader_guard = guard.clone();
    let reader_registry = registry.clone();
    let reader_sink = sink.clone();
    std::thread::spawn(move || {
        let (reason, message) = pump(&*port, reader_sink, event_id.clone(), reader_guard.clone());
        finish(
            &reader_registry,
            &event_id,
            &reader_guard,
            reason,
            message,
        );
    });
    Ok(())
}

async fn open_port(config: SerialConfig) -> Result<SerialPort, String> {
    let path = config.path.clone();
    let task = tokio::task::spawn_blocking(move || {
        SerialPort::open(&config.path, |mut settings: Settings| {
            settings.set_raw();
            settings.set_baud_rate(config.baud_rate)?;
            settings.set_char_size(match config.data_bits {
                5 => CharSize::Bits5,
                6 => CharSize::Bits6,
                7 => CharSize::Bits7,
                _ => CharSize::Bits8,
            });
            settings.set_stop_bits(if config.stop_bits == 2 {
                StopBits::Two
            } else {
                StopBits::One
            });
            settings.set_parity(match config.parity.as_str() {
                "odd" => Parity::Odd,
                "even" => Parity::Even,
                _ => Parity::None,
            });
            settings.set_flow_control(match config.flow_control.as_str() {
                "rtscts" => FlowControl::RtsCts,
                "xonxoff" => FlowControl::XonXoff,
                _ => FlowControl::None,
            });
            Ok(settings)
        })
        .map_err(|e| format!("failed to open {}: {e}", config.path))
    });
    match tokio::time::timeout(OPEN_TIMEOUT, task).await {
        Ok(Ok(result)) => result,
        Ok(Err(join)) => Err(format!("failed to open {path}: {join}")),
        Err(_) => Err(format!(
            "opening {path} timed out after {}s (check that the device is connected and not in use)",
            OPEN_TIMEOUT.as_secs()
        )),
    }
}

/// Reads a device until it stops. Reports *why* it stopped so the webview can
/// distinguish an unplugged cable from a clean shutdown.
fn pump<P: Read>(
    mut port: P,
    sink: Arc<dyn TerminalSink>,
    id: String,
    guard: SessionGuard,
) -> (CloseReason, Option<String>) {
    let mut buf = [0u8; 4096];
    let mut empty_reads = 0u32;
    loop {
        if guard.is_stopped() {
            return (CloseReason::User, None);
        }
        match port.read(&mut buf) {
            Ok(0) => {
                empty_reads += 1;
                if empty_reads >= MAX_EMPTY_READS {
                    return (
                        CloseReason::DeviceRemoved,
                        Some("device stopped responding (unplugged or suspended)".into()),
                    );
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(n) => {
                empty_reads = 0;
                sink.output(&id, buf[..n].to_vec());
            }
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(err) if err.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(err) => {
                let reason = match err.kind() {
                    std::io::ErrorKind::NotFound
                    | std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::UnexpectedEof => CloseReason::DeviceRemoved,
                    std::io::ErrorKind::TimedOut => CloseReason::Timeout,
                    _ => CloseReason::TransportError,
                };
                return (reason, Some(err.to_string()));
            }
        }
    }
}

/// The reader releases its own registry entry. Previously an unplugged device
/// left a dead entry behind forever, which made reconnect impossible.
fn finish(
    registry: &SerialRegistry,
    id: &str,
    guard: &SessionGuard,
    reason: CloseReason,
    message: Option<String>,
) {
    let Some((sink, owned_guard)) = registry.guards.take_if(id, guard) else {
        return; // a newer connection owns this id; stay silent
    };
    if let Ok(mut sessions) = sessions_map(&registry.sessions) {
        if matches!(sessions.get(id), Some(s) if s.guard.same(guard)) {
            sessions.remove(id);
        }
    }
    owned_guard.finish(sink.as_ref(), id, reason, message);
}

pub async fn write(registry: &SerialRegistry, id: &str, data: String) -> Result<(), String> {
    let port = {
        let sessions = sessions_map(&registry.sessions)?;
        match sessions.get(id) {
            Some(session) if !session.guard.is_stopped() => session.port.clone(),
            Some(_) => return Err("serial session is closed; reconnect to continue".to_string()),
            None => return Err("serial session not found; reconnect to continue".to_string()),
        }
    };
    port.write_all(data.as_bytes()).map_err(|e| e.to_string())
}

pub async fn close(registry: &SerialRegistry, id: &str) -> Result<(), String> {
    let taken = registry.guards.take_any(id);
    {
        let mut sessions = sessions_map(&registry.sessions)?;
        if let Some((_, guard)) = &taken {
            if matches!(sessions.get(id), Some(s) if s.guard.same(guard)) {
                sessions.remove(id);
            }
        } else {
            sessions.remove(id);
        }
    }
    if let Some((sink, guard)) = taken {
        guard.finish(sink.as_ref(), id, CloseReason::User, None);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ft_core::ClosedPayload;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    fn config() -> SerialConfig {
        SerialConfig {
            path: "/dev/ttyUSB0".to_string(),
            baud_rate: 115_200,
            data_bits: 8,
            parity: "none".to_string(),
            stop_bits: 1,
            flow_control: "none".to_string(),
        }
    }

    #[test]
    fn validates_config() {
        assert!(validate(&config()).is_ok());
        assert!(validate(&SerialConfig {
            path: " ".to_string(),
            ..config()
        })
        .is_err());
        assert!(validate(&SerialConfig {
            baud_rate: 0,
            ..config()
        })
        .is_err());
        assert!(validate(&SerialConfig {
            data_bits: 9,
            ..config()
        })
        .is_err());
        assert!(validate(&SerialConfig {
            parity: "mark".to_string(),
            ..config()
        })
        .is_err());
        assert!(validate(&SerialConfig {
            stop_bits: 3,
            ..config()
        })
        .is_err());
        assert!(validate(&SerialConfig {
            flow_control: "dsr".to_string(),
            ..config()
        })
        .is_err());
    }

    struct NullSink;

    impl TerminalSink for NullSink {
        fn output(&self, _id: &str, _data: Vec<u8>) {}
        fn closed(&self, _id: &str) {}
    }

    #[tokio::test]
    async fn rejects_missing_device_and_releases_the_reservation() {
        let registry = SerialRegistry::default();
        let sink: Arc<dyn TerminalSink> = Arc::new(NullSink);
        let err = open(
            sink,
            &registry,
            "serial-1".to_string(),
            SerialConfig {
                path: "/dev/definitely-not-a-serial-port".to_string(),
                ..config()
            },
        )
        .await
        .unwrap_err();
        assert!(err.contains("failed to open"));
        assert!(write(&registry, "serial-1", "x".to_string()).await.is_err());
        assert!(!registry.guards.contains("serial-1"));
    }

    #[derive(Default)]
    struct CollectSink {
        data: Mutex<Vec<u8>>,
        closes: Mutex<Vec<CloseReason>>,
        closed: AtomicBool,
    }

    impl TerminalSink for CollectSink {
        fn output(&self, _id: &str, data: Vec<u8>) {
            self.data.lock().unwrap().extend_from_slice(&data);
        }
        fn closed(&self, _id: &str) {
            self.closed.store(true, Ordering::Relaxed);
        }
        fn closed_with(&self, _id: &str, payload: ClosedPayload) {
            self.closed.store(true, Ordering::Relaxed);
            self.closes.lock().unwrap().push(payload.reason);
        }
    }

    #[test]
    fn forwards_device_bytes_to_the_sink() {
        let sink = Arc::new(CollectSink::default());
        let guard = SessionGuard::default();
        let device = std::io::Cursor::new(b"hello from device".to_vec());
        let sink_clone = sink.clone();
        let stop = guard.clone();
        let forwarder = std::thread::spawn(move || {
            pump(device, sink_clone, "serial-1".to_string(), guard)
        });
        for _ in 0..100 {
            if sink.data.lock().unwrap().len() >= 17 {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(sink.data.lock().unwrap().as_slice(), b"hello from device");
        stop.stop();
        let (reason, _) = forwarder.join().unwrap();
        assert_eq!(reason, CloseReason::User);
        // The same guard can still announce the close once, and only once.
        assert!(stop.finish(sink.as_ref(), "serial-1", reason, None));
        assert_eq!(*sink.closes.lock().unwrap(), vec![CloseReason::User]);
        assert!(sink.closed.load(Ordering::Relaxed));
    }

    #[test]
    fn a_removed_device_is_reported_as_such() {
        let sink = Arc::new(CollectSink::default());
        let guard = SessionGuard::default();
        let (reason, _) = pump(
            std::io::Cursor::new(Vec::<u8>::new()),
            sink,
            "serial-2".to_string(),
            guard,
        );
        assert_eq!(reason, CloseReason::DeviceRemoved);
    }

    #[tokio::test]
    async fn finish_removes_only_its_own_generation() {
        let registry = SerialRegistry::default();
        let sink: Arc<dyn TerminalSink> = Arc::new(NullSink);
        let old = registry.guards.begin("serial-3", sink.clone()).unwrap();
        registry.guards.take_any("serial-3");
        let current = registry.guards.begin("serial-3", sink).unwrap();
        // A stale reader must not announce a close or free the live id.
        finish(
            &registry,
            "serial-3",
            &old,
            CloseReason::DeviceRemoved,
            None,
        );
        assert!(registry.guards.is_current("serial-3", &current));
        finish(&registry, "serial-3", &current, CloseReason::User, None);
        assert!(!registry.guards.contains("serial-3"));
    }

    #[test]
    fn close_announces_exactly_once() {
        let sink = Arc::new(CollectSink::default());
        let guard = SessionGuard::default();
        assert!(guard.finish(sink.as_ref(), "s", CloseReason::User, None));
        assert!(!guard.finish(sink.as_ref(), "s", CloseReason::TransportError, None));
        assert_eq!(*sink.closes.lock().unwrap(), vec![CloseReason::User]);
    }
}
