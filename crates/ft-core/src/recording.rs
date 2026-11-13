use serde::{Deserialize, Serialize};
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

/// A plain-text session log. Logs are the default record of a session: they are
/// small, greppable, and readable long after the window is gone.
#[derive(Debug, Serialize)]
pub struct SessionLogMeta {
    pub name: String,
    pub title: String,
    pub timestamp: u64,
    pub size: u64,
    pub lines: u64,
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

/// What a session writes to disk.
///
/// `Log` is the default: plain text, both directions, no replay state. `Record`
/// is the asciicast replay and stays output-only, because a replay is meant to
/// be played back and typed secrets would end up on a shared screen. `Both`
/// keeps the audit trail and the replay side by side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CaptureMode {
    Log,
    Record,
    Both,
}

impl CaptureMode {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "log" => Some(Self::Log),
            "record" => Some(Self::Record),
            "both" => Some(Self::Both),
            _ => None,
        }
    }

    fn wants_log(self) -> bool {
        !matches!(self, Self::Record)
    }

    fn wants_cast(self) -> bool {
        !matches!(self, Self::Log)
    }
}

impl Default for CaptureMode {
    fn default() -> Self {
        Self::Log
    }
}

/// Paths a capture opened with, so the interface can tell the user where the
/// files landed without guessing the naming scheme.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturePaths {
    pub log: Option<String>,
    pub cast: Option<String>,
}

struct Recorder {
    log: Option<BufWriter<File>>,
    log_path: Option<PathBuf>,
    cast: Option<BufWriter<File>>,
    cast_path: Option<PathBuf>,
    mode: CaptureMode,
    started: Instant,
    title: String,
    cols: u16,
    rows: u16,
    stamp: u64,
    at_line_start: bool,
}

impl Recorder {
    fn paths(&self) -> CapturePaths {
        CapturePaths {
            log: self
                .log_path
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
            cast: self
                .cast_path
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
        }
    }

    fn open_log(&mut self, dir: &std::path::Path) -> Result<(), String> {
        let path = dir.join(format!("{}-{}.log", safe_name(&self.title), self.stamp));
        let file = open_file(&path)?;
        let mut writer = BufWriter::new(file);
        writeln!(writer, "# ferriterm session log").map_err(|err| err.to_string())?;
        writeln!(writer, "# title: {}", self.title).map_err(|err| err.to_string())?;
        writeln!(writer, "# started: {}", self.stamp).map_err(|err| err.to_string())?;
        writeln!(writer, "# mode: {}", mode_label(self.mode)).map_err(|err| err.to_string())?;
        writer.flush().map_err(|err| err.to_string())?;
        self.log = Some(writer);
        self.log_path = Some(path);
        self.at_line_start = true;
        Ok(())
    }

    fn close_log(&mut self) {
        if let Some(mut writer) = self.log.take() {
            let _ = writer.flush();
        }
        self.log_path = None;
    }

    fn open_cast(&mut self, dir: &std::path::Path) -> Result<(), String> {
        let path = dir.join(format!("{}-{}.cast", safe_name(&self.title), self.stamp));
        let file = open_file(&path)?;
        let mut writer = BufWriter::new(file);
        let header = serde_json::json!({
            "version": 2,
            "width": self.cols,
            "height": self.rows,
            "timestamp": self.stamp,
            "title": self.title,
            "env": { "TERM": "xterm-256color", "SHELL": "/bin/sh" }
        });
        writeln!(writer, "{header}").map_err(|err| err.to_string())?;
        writer.flush().map_err(|err| err.to_string())?;
        self.cast = Some(writer);
        self.cast_path = Some(path);
        Ok(())
    }

    fn close_cast(&mut self) {
        if let Some(mut writer) = self.cast.take() {
            let _ = writer.flush();
        }
        self.cast_path = None;
    }

    /// Writes text to the log. Every line carries its offset from the start of
    /// the session, and input lines are additionally marked with a chevron so a
    /// reader can tell what was typed from what came back.
    fn log_text(&mut self, elapsed: f64, input: bool, text: &str) {
        let Some(writer) = self.log.as_mut() else {
            return;
        };
        let mut buffer = String::with_capacity(text.len() + 24);
        for ch in text.chars() {
            if self.at_line_start {
                buffer.push_str(&format!("[{:>10.3}] ", elapsed));
                if input {
                    buffer.push_str("» ");
                }
                self.at_line_start = false;
            }
            if ch == '\n' {
                buffer.push('\n');
                self.at_line_start = true;
            } else if ch != '\r' {
                buffer.push(ch);
            }
        }
        if buffer.is_empty() {
            return;
        }
        let _ = writer.write_all(buffer.as_bytes());
        let _ = writer.flush();
    }

    fn cast_text(&mut self, elapsed: f64, text: &str) {
        let Some(writer) = self.cast.as_mut() else {
            return;
        };
        let event = serde_json::json!([elapsed, "o", text]);
        let _ = writeln!(writer, "{event}");
        let _ = writer.flush();
    }
}

fn mode_label(mode: CaptureMode) -> &'static str {
    match mode {
        CaptureMode::Log => "log",
        CaptureMode::Record => "record",
        CaptureMode::Both => "both",
    }
}

fn open_file(path: &std::path::Path) -> Result<File, String> {
    OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .map_err(|err| err.to_string())
}

/// Writes session logs (plain text) and optional asciicast replays.
///
/// Both artifacts share a directory and a timestamp, so a log and its replay
/// sort next to each other.
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

    pub fn start(
        &self,
        id: &str,
        title: &str,
        cols: u16,
        rows: u16,
        mode: CaptureMode,
    ) -> Result<CapturePaths, String> {
        create_dir_all(&self.dir).map_err(|err| err.to_string())?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err| err.to_string())?
            .as_secs();
        let mut recorder = Recorder {
            log: None,
            log_path: None,
            cast: None,
            cast_path: None,
            mode,
            started: Instant::now(),
            title: title.to_string(),
            cols,
            rows,
            stamp,
            at_line_start: true,
        };
        if mode.wants_log() {
            recorder.open_log(&self.dir)?;
        }
        if mode.wants_cast() {
            recorder.open_cast(&self.dir)?;
        }
        let paths = recorder.paths();
        self.inner.lock().unwrap().insert(id.to_string(), recorder);
        Ok(paths)
    }

    /// Switches what an already-running session keeps on disk. Missing artifacts
    /// are opened, unwanted ones are flushed and closed.
    pub fn set_mode(&self, id: &str, mode: CaptureMode) -> Result<CapturePaths, String> {
        let dir = self.dir.clone();
        let mut guard = self.inner.lock().unwrap();
        let Some(recorder) = guard.get_mut(id) else {
            return Err("session is not being captured".to_string());
        };
        if recorder.mode == mode {
            return Ok(recorder.paths());
        }
        if mode.wants_log() && recorder.log.is_none() {
            recorder.mode = mode;
            recorder.open_log(&dir)?;
        }
        if !mode.wants_log() && recorder.log.is_some() {
            recorder.close_log();
        }
        if mode.wants_cast() && recorder.cast.is_none() {
            recorder.mode = mode;
            recorder.open_cast(&dir)?;
        }
        if !mode.wants_cast() && recorder.cast.is_some() {
            recorder.close_cast();
        }
        recorder.mode = mode;
        Ok(recorder.paths())
    }

    pub fn mode(&self, id: &str) -> Option<CaptureMode> {
        self.inner.lock().ok()?.get(id).map(|item| item.mode)
    }

    /// Server output: goes to both the log and the replay.
    pub fn write_output(&self, id: &str, data: &[u8]) {
        let mut guard = match self.inner.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        let Some(recorder) = guard.get_mut(id) else {
            return;
        };
        let elapsed = recorder.started.elapsed().as_secs_f64();
        let text = String::from_utf8_lossy(data);
        if recorder.log.is_some() {
            let plain = strip_escapes(&text);
            recorder.log_text(elapsed, false, &plain);
        }
        if recorder.cast.is_some() {
            recorder.cast_text(elapsed, &text);
        }
    }

    /// Typed input: logged, never added to the replay.
    pub fn write_input(&self, id: &str, data: &[u8]) {
        let mut guard = match self.inner.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        let Some(recorder) = guard.get_mut(id) else {
            return;
        };
        if recorder.log.is_none() {
            return;
        }
        let elapsed = recorder.started.elapsed().as_secs_f64();
        let text = String::from_utf8_lossy(data);
        let plain = strip_escapes(&text);
        recorder.log_text(elapsed, true, &plain);
    }

    pub fn stop(&self, id: &str) -> Option<CapturePaths> {
        let mut guard = self.inner.lock().ok()?;
        let mut recorder = guard.remove(id)?;
        // Both paths are read before the writers close, because closing clears
        // the handle that carries them.
        let paths = recorder.paths();
        recorder.close_log();
        recorder.close_cast();
        Some(paths)
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

    pub fn list_logs(&self) -> Result<Vec<SessionLogMeta>, String> {
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(err.to_string()),
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("log") {
                continue;
            }
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let mut title = String::new();
            let mut timestamp = 0u64;
            let mut lines = 0u64;
            for line in text.lines() {
                if let Some(rest) = line.strip_prefix("# title: ") {
                    title = rest.to_string();
                } else if let Some(rest) = line.strip_prefix("# started: ") {
                    timestamp = rest.trim().parse().unwrap_or_default();
                } else if !line.starts_with('#') {
                    lines += 1;
                }
            }
            out.push(SessionLogMeta {
                name: name.to_string(),
                title: if title.is_empty() {
                    name.to_string()
                } else {
                    title
                },
                timestamp,
                size: entry.metadata().map(|meta| meta.len()).unwrap_or(0),
                lines,
            });
        }
        out.sort_by(|a, b| b.timestamp.cmp(&a.timestamp).then(b.name.cmp(&a.name)));
        Ok(out)
    }

    pub fn read(&self, name: &str) -> Result<CastFile, String> {
        let text = std::fs::read_to_string(self.safe_path(name)?).map_err(|err| err.to_string())?;
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

    pub fn read_log(&self, name: &str) -> Result<String, String> {
        std::fs::read_to_string(self.safe_path(name)?).map_err(|err| err.to_string())
    }

    fn safe_path(&self, name: &str) -> Result<PathBuf, String> {
        if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
            return Err("invalid recording name".into());
        }
        Ok(self.dir.join(name))
    }
}

/// Drops ANSI escape sequences and carriage returns so a log reads like plain
/// text in any editor. Cursor movement is lost on purpose: a log is a record of
/// what a session printed, not a replay of how the screen moved.
fn strip_escapes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\u{1b}' {
            if ch == '\t' || ch == '\n' || !ch.is_control() {
                out.push(ch);
            }
            continue;
        }
        match chars.next() {
            // CSI: parameters then a final byte in the @-~ range.
            Some('[') => {
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
            // OSC: runs until BEL or the two-byte string terminator.
            Some(']') => {
                while let Some(next) = chars.next() {
                    if next == '\u{7}' {
                        break;
                    }
                    if next == '\u{1b}' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            // Two-character escapes (charset selection, keypad modes, ...).
            Some(_) => {}
            None => {}
        }
    }
    out
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
        let paths = registry
            .start("s1", "demo host", 80, 24, CaptureMode::Record)
            .unwrap();
        registry.write_output("s1", b"hello\n");
        registry.stop("s1").unwrap();
        let path = paths.cast.expect("cast file");
        let text = std::fs::read_to_string(&path).unwrap();
        let mut lines = text.lines();
        let header: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
        assert_eq!(header["version"], 2);
        assert_eq!(header["width"], 80);
        let event: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
        assert_eq!(event[1], "o");
        assert_eq!(event[2], "hello\n");
        assert!(paths.log.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn logs_both_directions_by_default() {
        let dir = std::env::temp_dir().join(format!("ft-log-{}", std::process::id()));
        let registry = RecorderRegistry::new(&dir);
        let paths = registry.start("s1", "demo", 80, 24, CaptureMode::Log).unwrap();
        registry.write_output("s1", b"\x1b[32mready\x1b[0m\r\n");
        registry.write_input("s1", b"ls -la\n");
        registry.write_output("s1", b"total 0\n");
        registry.stop("s1");
        let text = std::fs::read_to_string(paths.log.expect("log file")).unwrap();
        assert!(text.contains("# title: demo"));
        assert!(text.contains("ready"), "escape sequences are stripped: {text}");
        assert!(!text.contains('\u{1b}'));
        assert!(text.contains("» ls -la"), "input is marked: {text}");
        assert!(text.contains("total 0"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn switching_mode_opens_and_closes_artifacts() {
        let dir = std::env::temp_dir().join(format!("ft-mode-{}", std::process::id()));
        let registry = RecorderRegistry::new(&dir);
        registry.start("s1", "demo", 80, 24, CaptureMode::Log).unwrap();
        registry.write_output("s1", b"first\n");
        let paths = registry.set_mode("s1", CaptureMode::Both).unwrap();
        assert!(paths.log.is_some() && paths.cast.is_some());
        registry.write_output("s1", b"second\n");
        let paths = registry.set_mode("s1", CaptureMode::Record).unwrap();
        assert!(paths.log.is_none());
        registry.write_input("s1", b"ignored\n");
        registry.stop("s1");
        let log = std::fs::read_to_string(
            std::fs::read_dir(&dir)
                .unwrap()
                .flatten()
                .map(|entry| entry.path())
                .find(|path| path.extension().and_then(|ext| ext.to_str()) == Some("log"))
                .unwrap(),
        )
        .unwrap();
        assert!(log.contains("first"));
        assert!(log.contains("second"));
        assert!(!log.contains("ignored"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn lists_and_reads_both_artifacts() {
        let dir = std::env::temp_dir().join(format!("ft-rec-list-{}", std::process::id()));
        let registry = RecorderRegistry::new(&dir);
        registry.start("s1", "demo host", 100, 40, CaptureMode::Both).unwrap();
        registry.write_output("s1", b"first\n");
        registry.write_output("s1", b"\x1b[32mgreen\x1b[0m\n");
        registry.stop("s1");
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

        let logs = registry.list_logs().unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].title, "demo host");
        assert_eq!(logs[0].lines, 2);
        let text = registry.read_log(&logs[0].name).unwrap();
        assert!(text.contains("green"));
        assert_eq!(
            registry.read_log("../escape").unwrap_err(),
            "invalid recording name"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn safe_names_strip_paths() {
        assert_eq!(safe_name("../../etc/passwd"), "etc-passwd");
        assert_eq!(safe_name(""), "session");
    }
}
