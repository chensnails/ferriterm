//! Serial-port transport. Sessions share the terminal plumbing with SSH and
//! PTY sessions: bytes read from the device are pushed through the same
//! `TerminalSink`, so recording and the webview events need no special case.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ft_core::TerminalSink;
use serde::{Deserialize, Serialize};
use serial2::{CharSize, FlowControl, Parity, SerialPort, Settings, StopBits};
use tokio::sync::Mutex;

const MAX_PATH_LEN: usize = 256;
const MAX_BAUD_RATE: u32 = 4_000_000;

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
    closed: Arc<AtomicBool>,
}

#[derive(Default, Clone)]
pub struct SerialRegistry {
    sessions: Arc<Mutex<HashMap<String, SerialSession>>>,
}

pub async fn open(
    sink: Arc<dyn TerminalSink>,
    registry: &SerialRegistry,
    id: String,
    config: SerialConfig,
) -> Result<(), String> {
    validate(&config)?;
    if registry.sessions.lock().await.contains_key(&id) {
        return Err(format!("serial session {id} already exists"));
    }
    let mut port = SerialPort::open(&config.path, |mut settings: Settings| {
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
    .map_err(|e| format!("failed to open {}: {e}", config.path))?;
    port.set_read_timeout(Duration::from_millis(100))
        .map_err(|e| e.to_string())?;
    port.set_write_timeout(Duration::from_secs(2))
        .map_err(|e| e.to_string())?;

    let port = Arc::new(port);
    let closed = Arc::new(AtomicBool::new(false));
    let event_id = id.clone();
    let reader_port = port.clone();
    let reader_closed = closed.clone();
    std::thread::spawn(move || pump(&*reader_port, sink, event_id, reader_closed));

    registry
        .sessions
        .lock()
        .await
        .insert(id, SerialSession { port, closed });
    Ok(())
}

fn pump<P: std::io::Read>(
    mut port: P,
    sink: Arc<dyn TerminalSink>,
    id: String,
    closed: Arc<AtomicBool>,
) {
    let mut buf = [0u8; 4096];
    loop {
        if closed.load(Ordering::Relaxed) {
            break;
        }
        match port.read(&mut buf) {
            Ok(0) => std::thread::sleep(Duration::from_millis(5)),
            Ok(n) => sink.output(&id, buf[..n].to_vec()),
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(err) if err.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(_) => break,
        }
    }
    sink.closed(&id);
}

pub async fn write(registry: &SerialRegistry, id: &str, data: String) -> Result<(), String> {
    let port = {
        let sessions = registry.sessions.lock().await;
        match sessions.get(id) {
            Some(session) => session.port.clone(),
            None => return Err("serial session not found".to_string()),
        }
    };
    port.write_all(data.as_bytes()).map_err(|e| e.to_string())
}

pub async fn close(registry: &SerialRegistry, id: &str) -> Result<(), String> {
    let session = registry.sessions.lock().await.remove(id);
    if let Some(session) = session {
        session.closed.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[tokio::test]
    async fn rejects_missing_device() {
        let registry = SerialRegistry::default();
        let sink = Arc::new(crate::tests::NullSink);
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
    }

    struct NullSink;

    impl TerminalSink for NullSink {
        fn output(&self, _id: &str, _data: Vec<u8>) {}
        fn closed(&self, _id: &str) {}
    }

    #[derive(Default)]
    struct CollectSink {
        data: std::sync::Mutex<Vec<u8>>,
        closed: AtomicBool,
    }

    impl TerminalSink for CollectSink {
        fn output(&self, _id: &str, data: Vec<u8>) {
            self.data.lock().unwrap().extend_from_slice(&data);
        }
        fn closed(&self, _id: &str) {
            self.closed.store(true, Ordering::Relaxed);
        }
    }

    #[test]
    fn forwards_device_bytes_to_the_sink() {
        let sink = Arc::new(CollectSink::default());
        let closed = Arc::new(AtomicBool::new(false));
        let device = std::io::Cursor::new(b"hello from device".to_vec());
        let stop = closed.clone();
        let forwarder = {
            let sink = sink.clone();
            std::thread::spawn(move || pump(device, sink, "serial-1".to_string(), stop))
        };
        for _ in 0..100 {
            if sink.data.lock().unwrap().len() >= 17 {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(sink.data.lock().unwrap().as_slice(), b"hello from device");
        closed.store(true, Ordering::Relaxed);
        forwarder.join().unwrap();
        assert!(sink.closed.load(Ordering::Relaxed));
    }
}
