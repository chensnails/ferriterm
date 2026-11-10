//! On-disk diagnostics.
//!
//! Events go to `<app data>/logs/ferriterm.log` at INFO and above; the
//! `RUST_LOG` environment variable overrides the filter. The file rotates at
//! 4 MiB and keeps one previous generation, so a long-running install cannot
//! fill the disk no matter how chatty a dependency gets. Panics are written
//! here too, so a crash on a user's machine leaves a record instead of
//! vanishing with the process.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const MAX_BYTES: u64 = 4 * 1024 * 1024;
const LOG_FILE: &str = "ferriterm.log";
const PREVIOUS_FILE: &str = "ferriterm.1.log";

/// `tracing-subscriber` asks for a writer per event. The writer buffers the
/// formatted event and appends it in one locked write when it is dropped.
#[derive(Clone)]
struct RotatingFile {
    inner: Arc<Mutex<State>>,
}

struct State {
    dir: PathBuf,
    file: File,
    bytes: u64,
}

impl RotatingFile {
    fn open(dir: &Path) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        let (file, bytes) = open_append(&dir.join(LOG_FILE))?;
        Ok(Self {
            inner: Arc::new(Mutex::new(State {
                dir: dir.to_path_buf(),
                file,
                bytes,
            })),
        })
    }

    fn append(&self, line: &[u8]) {
        let Ok(mut state) = self.inner.lock() else {
            return;
        };
        if state.bytes > 0 && state.bytes + line.len() as u64 > MAX_BYTES {
            rotate(&mut state);
        }
        if state.file.write_all(line).is_ok() {
            state.bytes += line.len() as u64;
        }
    }
}

fn open_append(path: &Path) -> io::Result<(File, u64)> {
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    let bytes = file.metadata().map(|meta| meta.len()).unwrap_or(0);
    Ok((file, bytes))
}

fn rotate(state: &mut State) {
    let previous = state.dir.join(PREVIOUS_FILE);
    let _ = fs::remove_file(&previous);
    if fs::rename(state.dir.join(LOG_FILE), &previous).is_err() {
        return;
    }
    if let Ok((file, bytes)) = open_append(&state.dir.join(LOG_FILE)) {
        state.file = file;
        state.bytes = bytes;
    }
}

struct EventWriter {
    target: RotatingFile,
    buffer: Vec<u8>,
}

impl Write for EventWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for EventWriter {
    fn drop(&mut self) {
        self.target.append(&self.buffer);
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for RotatingFile {
    type Writer = EventWriter;

    fn make_writer(&'a self) -> EventWriter {
        EventWriter {
            target: self.clone(),
            buffer: Vec::new(),
        }
    }
}

/// Installs the file subscriber and the panic hook. Failure to open the file
/// is returned so startup can fall back to stderr; logging must never be the
/// reason the app cannot run.
pub fn init(dir: &Path) -> io::Result<()> {
    let writer = RotatingFile::open(dir)?;
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        // Third-party crates stay quiet; our own events are the signal.
        tracing_subscriber::EnvFilter::new("warn,ferriterm=info,ferriterm_lib=info")
    });
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .with_writer(writer)
        .try_init()
        .map_err(|err| io::Error::other(err.to_string()))?;
    install_panic_hook();
    Ok(())
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|loc| format!("{}:{}", loc.file(), loc.line()))
            .unwrap_or_else(|| "unknown location".to_string());
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|text| (*text).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "non-string panic payload".to_string());
        let backtrace = std::backtrace::Backtrace::force_capture();
        tracing::error!(target: "panic", %location, backtrace = %backtrace, "panic: {message}");
        // Keep the default stderr output so terminal runs still show it.
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ferriterm-log-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn rotates_when_the_file_reaches_the_cap() {
        let dir = temp_dir("rotate");
        let log = RotatingFile::open(&dir).unwrap();
        let chunk = vec![b'x'; MAX_BYTES as usize];

        log.append(&chunk);
        log.append(&chunk);
        assert_eq!(fs::metadata(dir.join(LOG_FILE)).unwrap().len(), MAX_BYTES);

        // The third line would cross the cap, so the current file moves aside
        // and a fresh one starts with just that line.
        log.append(b"after rotation\n");
        assert_eq!(
            fs::metadata(dir.join(PREVIOUS_FILE)).unwrap().len(),
            MAX_BYTES
        );
        assert_eq!(
            fs::metadata(dir.join(LOG_FILE)).unwrap().len(),
            b"after rotation\n".len() as u64
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn appends_across_reopen() {
        let dir = temp_dir("append");
        RotatingFile::open(&dir).unwrap().append(b"first run\n");
        let reopened = RotatingFile::open(&dir).unwrap();
        reopened.append(b"second run\n");
        let text = fs::read_to_string(dir.join(LOG_FILE)).unwrap();
        assert!(text.ends_with("first run\nsecond run\n"), "{text}");
        fs::remove_dir_all(&dir).unwrap();
    }
}
