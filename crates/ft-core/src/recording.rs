use std::collections::HashMap;
use std::fs::{create_dir_all, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

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
}
