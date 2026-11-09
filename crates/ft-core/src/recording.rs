use serde::Serialize;
use std::collections::HashMap;
use std::fs::{create_dir_all, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize)]
pub struct RecordingMeta {
    pub name: String,
    pub title: String,
    pub timestamp: u64,
    pub size: u64,
    pub width: u16,
    pub height: u16,
    pub duration: f64,
}

#[derive(Debug, Serialize)]
pub struct CastHeader {
    pub width: u16,
    pub height: u16,
    pub timestamp: u64,
    pub title: String,
}

#[derive(Debug, Serialize)]
pub struct CastEvent {
    pub time: f64,
    pub data: String,
}

#[derive(Debug, Serialize)]
pub struct CastFile {
    pub header: CastHeader,
    pub events: Vec<CastEvent>,
    pub duration: f64,
}

struct Recorder {
    writer: BufWriter<File>,
    started: Instant,
    path: PathBuf,
}

/// Writes asciicast v2 files containing server output only. User input is never
/// recorded so typed secrets do not leak into session logs.
#[derive(Clone)]
pub struct RecorderRegistry {
    dir: PathBuf,
    inner: Arc<Mutex<HashMap<String, Recorder>>>,
}

impl RecorderRegistry {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn start(&self, id: &str, title: &str, cols: u16, rows: u16) -> Result<PathBuf, String> {
        create_dir_all(&self.dir).map_err(|err| err.to_string())?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err| err.to_string())?
            .as_secs();
        let path = self.dir.join(format!("{}-{stamp}.cast", safe_name(title)));
        let file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&path)
            .map_err(|err| err.to_string())?;
        let mut writer = BufWriter::new(file);
        let header = serde_json::json!({
            "version": 2,
            "width": cols,
            "height": rows,
            "timestamp": stamp,
            "title": title,
            "env": { "TERM": "xterm-256color", "SHELL": "/bin/sh" }
        });
        writeln!(writer, "{header}").map_err(|err| err.to_string())?;
        writer.flush().map_err(|err| err.to_string())?;
        self.inner.lock().unwrap().insert(
            id.to_string(),
            Recorder {
                writer,
                started: Instant::now(),
                path: path.clone(),
            },
        );
        Ok(path)
    }

    pub fn write(&self, id: &str, data: &[u8]) {
        let mut guard = self.inner.lock().unwrap();
        let Some(recorder) = guard.get_mut(id) else {
            return;
        };
        let elapsed = recorder.started.elapsed().as_secs_f64();
        let text = String::from_utf8_lossy(data);
        let event = serde_json::json!([elapsed, "o", text]);
        let _ = writeln!(recorder.writer, "{event}");
        let _ = recorder.writer.flush();
    }

    pub fn stop(&self, id: &str) -> Option<PathBuf> {
        let mut guard = self.inner.lock().unwrap();
        let recorder = guard.remove(id)?;
        if let Ok(mut file) = recorder.writer.into_inner() {
            let _ = file.flush();
        }
        Some(recorder.path)
    }

    pub fn list(&self) -> Result<Vec<RecordingMeta>, String> {
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(err.to_string()),
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("cast") {
                continue;
            }
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let mut lines = text.lines();
            let Some(header) = lines.next().and_then(parse_header) else {
                continue;
            };
            let mut duration: f64 = 0.0;
            for line in lines {
                if let Some(event) = parse_event(line) {
                    duration = duration.max(event.time);
                }
            }
            out.push(RecordingMeta {
                name: name.to_string(),
                title: header.title,
                timestamp: header.timestamp,
                size: entry.metadata().map(|meta| meta.len()).unwrap_or(0),
                width: header.width,
                height: header.height,
                duration,
            });
        }
        out.sort_by(|a, b| b.timestamp.cmp(&a.timestamp).then(b.name.cmp(&a.name)));
        Ok(out)
    }

    pub fn read(&self, name: &str) -> Result<CastFile, String> {
        if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
            return Err("invalid recording name".into());
        }
        let text = std::fs::read_to_string(self.dir.join(name)).map_err(|err| err.to_string())?;
        let mut lines = text.lines();
        let header = lines
            .next()
            .and_then(parse_header)
            .ok_or_else(|| "unsupported asciicast header".to_string())?;
        let mut events = Vec::new();
        let mut duration: f64 = 0.0;
        for line in lines {
            if let Some(event) = parse_event(line) {
                duration = duration.max(event.time);
                events.push(event);
            }
        }
        Ok(CastFile {
            header,
            events,
            duration,
        })
    }
}

fn parse_header(line: &str) -> Option<CastHeader> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    if value.get("version")?.as_u64()? != 2 {
        return None;
    }
    let dimension = |key: &str, default: u16| {
        value
            .get(key)
            .and_then(|item| item.as_u64())
            .and_then(|number| u16::try_from(number).ok())
            .unwrap_or(default)
    };
    Some(CastHeader {
        width: dimension("width", 80),
        height: dimension("height", 24),
        timestamp: value
            .get("timestamp")
            .and_then(|item| item.as_u64())
            .unwrap_or(0),
        title: value
            .get("title")
            .and_then(|item| item.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

fn parse_event(line: &str) -> Option<CastEvent> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let parts = value.as_array()?;
    if parts.len() < 3 || parts[1].as_str()? != "o" {
        return None;
    }
    Some(CastEvent {
        time: parts[0].as_f64()?,
        data: parts[2].as_str()?.to_string(),
    })
}

fn safe_name(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('-');
    if trimmed.is_empty() {
        "session".into()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_asciicast_v2_events() {
        let dir = std::env::temp_dir().join(format!("ft-rec-{}", std::process::id()));
        let registry = RecorderRegistry::new(&dir);
        let path = registry.start("s1", "demo host", 80, 24).unwrap();
        registry.write("s1", b"hello\n");
        let stopped = registry.stop("s1").unwrap();
        assert_eq!(path, stopped);
        let text = std::fs::read_to_string(&path).unwrap();
        let mut lines = text.lines();
        let header: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
        assert_eq!(header["version"], 2);
        assert_eq!(header["width"], 80);
        let event: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
        assert_eq!(event[1], "o");
        assert_eq!(event[2], "hello\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn safe_names_strip_paths() {
        assert_eq!(safe_name("../../etc/passwd"), "etc-passwd");
        assert_eq!(safe_name(""), "session");
    }

    #[test]
    fn lists_and_reads_cast_files() {
        let dir = std::env::temp_dir().join(format!("ft-rec-list-{}", std::process::id()));
        let registry = RecorderRegistry::new(&dir);
        registry.start("s1", "demo host", 100, 40).unwrap();
        registry.write("s1", b"first\n");
        registry.write("s1", b"\x1b[32mgreen\x1b[0m\n");
        let path = registry.stop("s1").unwrap();
        let list = registry.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "demo host");
        assert_eq!(list[0].width, 100);
        assert_eq!(list[0].height, 40);
        assert!(list[0].size > 0);
        let cast = registry.read(&list[0].name).unwrap();
        assert_eq!(cast.header.width, 100);
        assert_eq!(cast.events.len(), 2);
        assert_eq!(cast.events[1].data, "\x1b[32mgreen\x1b[0m\n");
        assert!(cast.duration >= 0.0);
        assert_eq!(
            registry.read("../escape").unwrap_err(),
            "invalid recording name"
        );
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
