//! FTP-backed virtual filesystem (RFC 959, passive mode only).
//!
//! Every operation opens its own control connection and closes it again. FTP
//! keeps a lot of per-connection state (transfer type, pending data channel),
//! so sharing one connection between concurrent transfers would interleave
//! commands and deadlock as soon as a job read from and wrote to the same
//! server. A short login per call costs two round trips and buys correctness;
//! the transfer engine already runs six jobs in parallel.
//!
//! Fields that FTP cannot express are degraded the way the WebDAV backend
//! degrades them: directories report `0755`, files report `0644`, ownership is
//! zero, timestamps come from `MLSD`/`MDTM` (second precision at best) and
//! symlinks are reported as their bare entry.

use std::net::SocketAddr;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::timeutil::{civil_from_days, parse_month, unix_from_parts, unix_now};
use crate::{FileEntry, OpenMode, Vfs, VfsFile, VfsFuture};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REPLY_TIMEOUT: Duration = Duration::from_secs(30);
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_REPLY_LINE: usize = 8 * 1024;
const MAX_LISTING_BYTES: usize = 8 * 1024 * 1024;
const DEFAULT_PORT: u16 = 21;

#[derive(Clone)]
pub struct FtpVfs {
    host: String,
    port: u16,
    username: String,
    password: String,
    /// Directory that `/` maps to. Empty means the login directory.
    root: String,
}

impl FtpVfs {
    pub fn new(
        host: &str,
        port: Option<u16>,
        username: &str,
        password: &str,
        root: &str,
    ) -> Result<Self, String> {
        let host = host.trim().to_string();
        if host.is_empty() {
            return Err("FTP host is required".to_string());
        }
        // A scheme or a path here means someone pasted a URL; failing loudly is
        // friendlier than a DNS error on `ftp://host`.
        if host.contains("://") || host.contains('/') || host.contains(char::is_whitespace) {
            return Err(format!(
                "invalid FTP host {host:?}: enter a host name or IP address, not a URL"
            ));
        }
        let port = port.unwrap_or(DEFAULT_PORT);
        if port == 0 {
            return Err("FTP port must be between 1 and 65535".to_string());
        }
        let root = root.trim().trim_end_matches('/').to_string();
        if root.contains('\r') || root.contains('\n') {
            return Err("the FTP root directory contains an invalid character".to_string());
        }
        Ok(Self {
            host,
            port,
            username: username.to_string(),
            password: password.to_string(),
            root,
        })
    }

    /// Maps a VFS path (`/`, `/dir/file`) onto the remote path, honouring the
    /// configured root. Rejects control characters because the result is
    /// written straight into a protocol line.
    fn remote_path(&self, path: &str) -> Result<String, String> {
        if path.contains('\r') || path.contains('\n') {
            return Err("the path contains an invalid character".to_string());
        }
        let relative = path.trim().trim_matches('/');
        let joined = match (self.root.as_str(), relative.is_empty()) {
            (_, true) => {
                if self.root.is_empty() {
                    "/".to_string()
                } else {
                    self.root.clone()
                }
            }
            ("", false) => format!("/{relative}"),
            (root, false) => format!("{root}/{relative}"),
        };
        Ok(joined)
    }

    fn vfs_path(&self, name: &str) -> String {
        let name = name.trim_start_matches('/');
        if name.is_empty() {
            "/".to_string()
        } else {
            format!("/{name}")
        }
    }

    async fn connect(&self) -> Result<FtpConn, String> {
        let address = format!("{}:{}", self.host, self.port);
        let stream = timeout(CONNECT_TIMEOUT, TcpStream::connect(&address))
            .await
            .map_err(|_| format!("timed out connecting to {address}"))?
            .map_err(|err| format!("cannot connect to {address}: {err}"))?;
        let _ = stream.set_nodelay(true);
        let peer = stream
            .peer_addr()
            .map_err(|err| format!("cannot inspect the FTP connection: {err}"))?;
        let mut conn = FtpConn {
            reader: BufReader::new(stream),
            peer,
            features: Features::default(),
        };

        let mut greeting = conn.read_reply().await?;
        // 120 means "service ready shortly": the real 220 follows.
        if greeting.code == 120 {
            greeting = conn.read_reply().await?;
        }
        check(greeting, &[220], "greeting")?;

        let user = if self.username.is_empty() {
            "anonymous".to_string()
        } else {
            self.username.clone()
        };
        let reply = conn.send(&format!("USER {user}"), &[230, 331]).await?;
        if reply.code == 331 {
            let reply = conn
                .send(&format!("PASS {}", self.password), &[230, 202])
                .await?;
            if reply.code == 202 {
                return Err("the FTP server accepted the user but refused the login".to_string());
            }
        }
        conn.features = conn.read_features().await;
        // UTF-8 file names are an extension; servers that do not know the
        // command answer 5xx and keep their default encoding.
        if conn.features.utf8 {
            let _ = conn.send("OPTS UTF8 ON", &[200]).await;
        }
        conn.send("TYPE I", &[200]).await?;
        Ok(conn)
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Features {
    mlsd: bool,
    mfmt: bool,
    utf8: bool,
}

struct FtpConn {
    reader: BufReader<TcpStream>,
    peer: SocketAddr,
    features: Features,
}

struct Reply {
    code: u16,
    text: String,
}

fn check(reply: Reply, expected: &[u16], what: &str) -> Result<Reply, String> {
    if expected.contains(&reply.code) {
        return Ok(reply);
    }
    let text = reply.text.trim();
    Err(if text.is_empty() {
        format!("FTP {what} failed with status {}", reply.code)
    } else {
        format!("FTP {what} failed ({}): {text}", reply.code)
    })
}

impl FtpConn {
    async fn read_line(&mut self) -> Result<String, String> {
        let mut buffer = Vec::new();
        let read = timeout(REPLY_TIMEOUT, self.reader.read_until(b'\n', &mut buffer))
            .await
            .map_err(|_| "timed out waiting for the FTP server".to_string())?
            .map_err(|err| format!("FTP read failed: {err}"))?;
        if read == 0 {
            return Err("the FTP server closed the connection".to_string());
        }
        if buffer.len() > MAX_REPLY_LINE {
            return Err("the FTP server sent an oversized reply line".to_string());
        }
        Ok(String::from_utf8_lossy(&buffer)
            .trim_end_matches(['\r', '\n'])
            .to_string())
    }

    async fn read_reply(&mut self) -> Result<Reply, String> {
        let first = self.read_line().await?;
        let code = parse_code(&first)?;
        let mut text = String::new();
        // `211-Features:` opens a multi-line reply that ends with `211 ...`.
        if first.as_bytes().get(3) == Some(&b'-') {
            text.push_str(first.get(4..).unwrap_or_default());
            loop {
                let line = self.read_line().await?;
                let closes = line.len() >= 4
                    && line.as_bytes().get(3) == Some(&b' ')
                    && line.get(..3).and_then(|value| value.parse::<u16>().ok()) == Some(code);
                text.push('\n');
                if closes {
                    text.push_str(line.get(4..).unwrap_or_default());
                    break;
                }
                text.push_str(&line);
            }
        } else {
            text.push_str(first.get(4..).unwrap_or_default());
        }
        Ok(Reply { code, text })
    }

    async fn send(&mut self, command: &str, expected: &[u16]) -> Result<Reply, String> {
        let line = format!("{command}\r\n");
        timeout(
            REPLY_TIMEOUT,
            self.reader.get_mut().write_all(line.as_bytes()),
        )
        .await
        .map_err(|_| "timed out sending an FTP command".to_string())?
        .map_err(|err| format!("FTP write failed: {err}"))?;
        let reply = self.read_reply().await?;
        check(reply, expected, command_verb(command))
    }

    async fn read_features(&mut self) -> Features {
        let mut features = Features::default();
        let Ok(reply) = self.send("FEAT", &[211]).await else {
            return features;
        };
        for line in reply.text.lines() {
            let upper = line.trim().to_ascii_uppercase();
            // Some servers reply with a bare name, others with `NAME args`;
            // only the name matters here.
            match upper.split_whitespace().next().unwrap_or_default() {
                "MLSD" | "MLST" => features.mlsd = true,
                "MFMT" => features.mfmt = true,
                "UTF8" => features.utf8 = true,
                _ => {}
            }
        }
        features
    }

    /// Opens a passive data channel. `EPSV` is tried first because it works
    /// over IPv6 and survives NAT rewriting of the address in `PASV` replies.
    async fn passive(&mut self) -> Result<TcpStream, String> {
        match self.send("EPSV", &[229]).await {
            Ok(reply) => {
                let port = parse_epsv_port(&reply.text).ok_or_else(|| {
                    format!("the FTP server sent an unusable EPSV reply: {}", reply.text)
                })?;
                let address = SocketAddr::new(self.peer.ip(), port);
                timeout(CONNECT_TIMEOUT, TcpStream::connect(address))
                    .await
                    .map_err(|_| format!("timed out opening the FTP data channel to {address}"))?
                    .map_err(|err| format!("cannot open the FTP data channel: {err}"))
            }
            Err(epsv_error) => {
                let reply = self.send("PASV", &[227]).await.map_err(|pasv_error| {
                    format!("{epsv_error}; passive mode is unavailable ({pasv_error})")
                })?;
                let (host, port) = parse_pasv_reply(&reply.text).ok_or_else(|| {
                    format!("the FTP server sent an unusable PASV reply: {}", reply.text)
                })?;
                // A server behind NAT often advertises an address we cannot
                // reach; the control connection already knows the real one.
                let unreachable = host.is_unspecified()
                    || (host.is_loopback() && !self.peer.ip().is_loopback());
                let address = SocketAddr::new(if unreachable { self.peer.ip() } else { host }, port);
                timeout(CONNECT_TIMEOUT, TcpStream::connect(address))
                    .await
                    .map_err(|_| format!("timed out opening the FTP data channel to {address}"))?
                    .map_err(|err| format!("cannot open the FTP data channel: {err}"))
            }
        }
    }

    /// Consumes the completion reply that follows a data transfer, keeping the
    /// control connection in sync for the next command.
    async fn finish_data(&mut self) -> Result<(), String> {
        let reply = self.read_reply().await?;
        if matches!(reply.code, 226 | 250) {
            Ok(())
        } else {
            check(reply, &[226, 250], "transfer").map(|_| ())
        }
    }
}

fn command_verb(command: &str) -> &str {
    command.split_whitespace().next().unwrap_or(command)
}

fn parse_code(line: &str) -> Result<u16, String> {
    line.get(..3)
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| format!("the FTP server sent an unreadable reply: {line}"))
}

/// `229 Entering Extended Passive Mode (|||6446|)`.
fn parse_epsv_port(text: &str) -> Option<u16> {
    let start = text.find('(')?;
    let end = text[start..].find(')')? + start;
    let inner = text.get(start + 1..end)?;
    let delimiter = inner.chars().next()?;
    inner
        .split(delimiter)
        .find(|part| !part.is_empty())?
        .parse()
        .ok()
}

/// `227 Entering Passive Mode (192,168,1,4,195,80)`.
fn parse_pasv_reply(text: &str) -> Option<(std::net::IpAddr, u16)> {
    let start = text.find('(')?;
    let end = text[start..].find(')')? + start;
    let inner = text.get(start + 1..end)?;
    let numbers: Vec<u16> = inner
        .split(',')
        .filter_map(|part| part.trim().parse::<u16>().ok())
        .collect();
    if numbers.len() != 6 || numbers.iter().take(4).any(|value| *value > 255) {
        return None;
    }
    let ip = std::net::IpAddr::V4(std::net::Ipv4Addr::new(
        numbers[0] as u8,
        numbers[1] as u8,
        numbers[2] as u8,
        numbers[3] as u8,
    ));
    Some((ip, numbers[4] * 256 + numbers[5]))
}

struct ParsedEntry {
    name: String,
    is_dir: bool,
    size: u64,
    mode: u32,
    mtime: u64,
}

/// `MLSD` line: `type=file;size=42;modify=20240131120000; report.txt`.
fn parse_mlsd_line(line: &str) -> Option<ParsedEntry> {
    let (facts, name) = line.split_once(' ')?;
    // A fact list always looks like `key=value;...`; anything else is a
    // `LIST` line that the caller should try with its own parser.
    if !facts.contains(';') || !facts.contains('=') {
        return None;
    }
    let name = name.trim();
    if name.is_empty() || name == "." || name == ".." {
        return None;
    }
    let mut kind = "";
    let mut size = 0u64;
    let mut mtime = 0u64;
    for fact in facts.split(';') {
        let Some((key, value)) = fact.split_once('=') else {
            continue;
        };
        match key.to_ascii_lowercase().as_str() {
            "type" => kind = value,
            "size" => size = value.parse().unwrap_or(0),
            "modify" => mtime = parse_ftp_timestamp(value),
            _ => {}
        }
    }
    let lower = kind.to_ascii_lowercase();
    if lower.is_empty() || lower == "cdir" || lower == "pdir" {
        return None;
    }
    let is_dir = lower == "dir";
    Some(ParsedEntry {
        name: name.to_string(),
        is_dir,
        size: if is_dir { 0 } else { size },
        mode: if is_dir { 0o755 } else { 0o644 },
        mtime,
    })
}

/// Unix `LIST` line: `-rw-r--r-- 1 owner group 42 Jan 31 12:00 report.txt`.
fn parse_unix_list_line(line: &str) -> Option<ParsedEntry> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() < 9 {
        return None;
    }
    let permissions = fields[0];
    let kind = permissions.chars().next()?;
    if !matches!(kind, '-' | 'd' | 'l') || permissions.len() != 10 {
        return None;
    }
    let size: u64 = fields[4].parse().ok()?;
    let month = parse_month(fields[5])?;
    let day: u32 = fields[6].parse().ok()?;
    let mtime = match parse_list_clock(fields[7]) {
        Some((hour, minute, second)) => {
            // A clock time carries no year: it is the most recent occurrence,
            // so anything "in the future" is really last year's.
            let now = unix_now();
            let this_year = civil_from_days((now / 86_400) as i64).0;
            let stamp = unix_from_parts(this_year, month, day, hour, minute, second);
            if stamp > now + 86_400 {
                unix_from_parts(this_year - 1, month, day, hour, minute, second)
            } else {
                stamp
            }
        }
        None => {
            let year: i64 = fields[7].parse().ok()?;
            unix_from_parts(year, month, day, 0, 0, 0)
        }
    };
    let mut name = fields[8..].join(" ");
    // Symlinks come as `name -> target`; the entry itself is the name.
    if let Some((left, _)) = name.split_once(" -> ") {
        name = left.to_string();
    }
    if name.is_empty() || name == "." || name == ".." {
        return None;
    }
    let is_dir = kind == 'd';
    Some(ParsedEntry {
        name,
        is_dir,
        size: if is_dir { 0 } else { size },
        mode: if is_dir { 0o755 } else { 0o644 },
        mtime,
    })
}

/// Parses the seventh `LIST` column when it is a clock time (`12:00`).
fn parse_list_clock(value: &str) -> Option<(u64, u64, u64)> {
    let (hour, rest) = value.split_once(':')?;
    let minute = rest.get(..2)?;
    let second = rest.get(3..5).unwrap_or("00");
    Some((
        hour.parse().ok()?,
        minute.parse().ok()?,
        second.parse().ok()?,
    ))
}

/// IIS `LIST` line: `01-31-24  12:00PM       <DIR>          folder`.
fn parse_dos_list_line(line: &str) -> Option<ParsedEntry> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() < 4 {
        return None;
    }
    let mut date_parts = fields[0].split('-');
    let month: u32 = date_parts.next()?.parse().ok()?;
    let day: u32 = date_parts.next()?.parse().ok()?;
    let year: i64 = date_parts.next()?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let year = if year < 100 { 2000 + year } else { year };
    let (hour, minute) = parse_dos_time(fields[1])?;
    let mut index = 2;
    let mut is_dir = false;
    let mut size = 0u64;
    if fields[index].eq_ignore_ascii_case("<DIR>") {
        is_dir = true;
        index += 1;
    } else {
        size = fields[index].replace(',', "").parse().ok()?;
        index += 1;
    }
    let name = fields.get(index..)?.join(" ");
    if name.is_empty() || name == "." || name == ".." {
        return None;
    }
    Some(ParsedEntry {
        name,
        is_dir,
        size: if is_dir { 0 } else { size },
        mode: if is_dir { 0o755 } else { 0o644 },
        mtime: unix_from_parts(year, month, day, hour, minute, 0),
    })
}

fn parse_dos_time(value: &str) -> Option<(u64, u64)> {
    let upper = value.to_ascii_uppercase();
    let (clock, meridiem) = match upper.strip_suffix("AM") {
        Some(clock) => (clock, "AM"),
        None => (upper.strip_suffix("PM")?, "PM"),
    };
    let (hour, minute) = clock.split_once(':')?;
    let mut hour: u64 = hour.parse().ok()?;
    let minute: u64 = minute.parse().ok()?;
    if hour == 12 {
        hour = 0;
    }
    if meridiem == "PM" {
        hour += 12;
    }
    Some((hour, minute))
}

/// `MLSD`/`MDTM` timestamps: `YYYYMMDDHHMMSS` with an optional fraction.
fn parse_ftp_timestamp(value: &str) -> u64 {
    let digits: String = value
        .trim()
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    if digits.len() < 14 {
        return 0;
    }
    let number = |range: std::ops::Range<usize>| digits.get(range).and_then(|v| v.parse::<i64>().ok());
    match (
        number(0..4),
        number(4..6),
        number(6..8),
        number(8..10),
        number(10..12),
        number(12..14),
    ) {
        (Some(year), Some(month), Some(day), Some(hour), Some(minute), Some(second)) => {
            unix_from_parts(
                year,
                month as u32,
                day as u32,
                hour as u64,
                minute as u64,
                second as u64,
            )
        }
        _ => 0,
    }
}

fn parse_listing(text: &str) -> Vec<ParsedEntry> {
    let mut entries = Vec::new();
    for line in text.lines() {
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() || line.starts_with("total ") {
            continue;
        }
        let parsed = parse_mlsd_line(line)
            .or_else(|| parse_unix_list_line(line))
            .or_else(|| parse_dos_list_line(line));
        if let Some(entry) = parsed {
            entries.push(entry);
        }
    }
    entries
}

struct FtpFile {
    conn: Option<FtpConn>,
    read: Option<OwnedReadHalf>,
    write: Option<OwnedWriteHalf>,
    path: String,
    mode: OpenMode,
    /// Byte offset the data channel is currently positioned at.
    pos: u64,
    /// A data transfer is open and its completion reply is still unread.
    active: bool,
}

impl FtpFile {
    /// Closes the data channel and consumes the `226` that ends the transfer.
    async fn finish_transfer(&mut self) -> Result<(), String> {
        self.read = None;
        if let Some(mut write) = self.write.take() {
            let _ = write.shutdown().await;
        }
        if !self.active {
            return Ok(());
        }
        self.active = false;
        match self.conn.as_mut() {
            Some(conn) => conn.finish_data().await,
            None => Ok(()),
        }
    }

    async fn start_read(&mut self, offset: u64) -> Result<(), String> {
        self.finish_transfer().await?;
        let conn = self
            .conn
            .as_mut()
            .ok_or_else(|| "the FTP file is closed".to_string())?;
        let data = conn.passive().await?;
        if offset > 0 {
            conn.send(&format!("REST {offset}"), &[350]).await?;
        }
        conn.send(&format!("RETR {}", self.path), &[125, 150])
            .await?;
        let (read, _write) = data.into_split();
        self.read = Some(read);
        self.pos = offset;
        self.active = true;
        Ok(())
    }

    async fn start_write(&mut self, append: bool) -> Result<(), String> {
        self.finish_transfer().await?;
        let conn = self
            .conn
            .as_mut()
            .ok_or_else(|| "the FTP file is closed".to_string())?;
        let data = conn.passive().await?;
        let command = if append {
            format!("APPE {}", self.path)
        } else {
            format!("STOR {}", self.path)
        };
        conn.send(&command, &[125, 150]).await?;
        let (_read, write) = data.into_split();
        self.write = Some(write);
        self.active = true;
        Ok(())
    }
}

impl VfsFile for FtpFile {
    fn read<'a>(&'a mut self, offset: u64, len: u32) -> VfsFuture<'a, Vec<u8>> {
        Box::pin(async move {
            if !matches!(self.mode, OpenMode::Read) {
                return Err("the FTP file is not open for reading".to_string());
            }
            if self.read.is_none() || offset != self.pos {
                self.start_read(offset).await?;
            }
            let mut buffer = vec![0u8; len as usize];
            let mut filled = 0usize;
            while filled < buffer.len() {
                let read = self
                    .read
                    .as_mut()
                    .ok_or_else(|| "the FTP data channel is closed".to_string())?;
                let read = timeout(TRANSFER_TIMEOUT, read.read(&mut buffer[filled..]))
                    .await
                    .map_err(|_| "timed out waiting for FTP data".to_string())?
                    .map_err(|err| format!("FTP download failed: {err}"))?;
                if read == 0 {
                    break;
                }
                filled += read;
            }
            buffer.truncate(filled);
            self.pos += filled as u64;
            Ok(buffer)
        })
    }

    fn write<'a>(&'a mut self, offset: u64, data: &'a [u8]) -> VfsFuture<'a, ()> {
        Box::pin(async move {
            if !matches!(self.mode, OpenMode::Write { .. }) {
                return Err("the FTP file is not open for writing".to_string());
            }
            if offset != self.pos {
                return Err(format!(
                    "FTP uploads must be sequential: attempted to write at {offset} while the stream is at {}",
                    self.pos
                ));
            }
            if self.write.is_none() {
                let append = matches!(self.mode, OpenMode::Write { truncate: false });
                self.start_write(append).await?;
            }
            let write = self
                .write
                .as_mut()
                .ok_or_else(|| "the FTP data channel is closed".to_string())?;
            timeout(TRANSFER_TIMEOUT, write.write_all(data))
                .await
                .map_err(|_| "timed out sending FTP data".to_string())?
                .map_err(|err| format!("FTP upload failed: {err}"))?;
            self.pos += data.len() as u64;
            Ok(())
        })
    }

    fn close<'a>(&'a mut self) -> VfsFuture<'a, ()> {
        Box::pin(async move {
            let transferred = self.finish_transfer().await;
            if let Some(mut conn) = self.conn.take() {
                let _ = conn.send("QUIT", &[221]).await;
            }
            transferred
        })
    }
}

impl Vfs for FtpVfs {
    fn list<'a>(&'a self, path: &'a str, show_hidden: bool) -> VfsFuture<'a, Vec<FileEntry>> {
        Box::pin(async move {
            let remote = self.remote_path(path)?;
            let mut conn = self.connect().await?;
            if !conn.features.mlsd {
                // `LIST` output is human-oriented, so ask for ASCII transfer.
                conn.send("TYPE A", &[200]).await?;
            }
            let command = if conn.features.mlsd {
                format!("MLSD {remote}")
            } else {
                format!("LIST {remote}")
            };
            let data = conn.passive().await?;
            conn.send(&command, &[125, 150]).await?;
            let text = read_listing(data).await?;
            conn.finish_data().await?;

            let mut out = Vec::new();
            for entry in parse_listing(&text) {
                if !show_hidden && entry.name.starts_with('.') {
                    continue;
                }
                // A listing of `/` can report fully qualified names on some
                // servers; only the leaf belongs in the table.
                if entry.name.contains('/') {
                    continue;
                }
                out.push(FileEntry {
                    path: self.vfs_path(&entry.name),
                    name: entry.name,
                    size: entry.size,
                    mode: entry.mode,
                    uid: 0,
                    gid: 0,
                    mtime: entry.mtime,
                    is_dir: entry.is_dir,
                    is_symlink: false,
                    owner: None,
                    group: None,
                });
            }
            out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
            Ok(out)
        })
    }

    fn stat<'a>(&'a self, path: &'a str) -> VfsFuture<'a, FileEntry> {
        Box::pin(async move {
            let remote = self.remote_path(path)?;
            let name = path
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_string();
            let mut conn = self.connect().await?;
            if conn.features.mlsd {
                if let Ok(reply) = conn.send(&format!("MLST {remote}"), &[250]).await {
                    for line in reply.text.lines() {
                        // The facts are followed by the full remote path, so
                        // the name comes from the request instead.
                        if let Some(mut entry) = parse_mlsd_line(line.trim()) {
                            entry.name = name.clone();
                            return Ok(FileEntry {
                                path: self.vfs_path(&name),
                                name,
                                size: entry.size,
                                mode: entry.mode,
                                uid: 0,
                                gid: 0,
                                mtime: entry.mtime,
                                is_dir: entry.is_dir,
                                is_symlink: false,
                                owner: None,
                                group: None,
                            });
                        }
                    }
                }
            }

            // Fallback: ask the parent directory. It costs one listing but
            // works on servers that only implement the base specification.
            let parent = crate::parent_path(path).unwrap_or_else(|| "/".to_string());
            self.list(&parent, true)
                .await?
                .into_iter()
                .find(|entry| entry.name == name)
                .ok_or_else(|| format!("{path} was not found on the FTP server"))
        })
    }

    fn home<'a>(&'a self) -> VfsFuture<'a, String> {
        Box::pin(async { Ok("/".to_string()) })
    }

    fn mkdir<'a>(&'a self, path: &'a str) -> VfsFuture<'a, ()> {
        Box::pin(async move {
            let remote = self.remote_path(path)?;
            let mut conn = self.connect().await?;
            // 257 is the documented success; some servers answer 250.
            conn.send(&format!("MKD {remote}"), &[257, 250]).await?;
            let _ = conn.send("QUIT", &[221]).await;
            Ok(())
        })
    }

    fn remove_file<'a>(&'a self, path: &'a str) -> VfsFuture<'a, ()> {
        Box::pin(async move {
            let remote = self.remote_path(path)?;
            let mut conn = self.connect().await?;
            conn.send(&format!("DELE {remote}"), &[250]).await?;
            let _ = conn.send("QUIT", &[221]).await;
            Ok(())
        })
    }

    fn remove_dir_all<'a>(&'a self, path: &'a str) -> VfsFuture<'a, ()> {
        Box::pin(async move {
            let remote = self.remote_path(path)?;
            let mut conn = self.connect().await?;
            conn.send("TYPE A", &[200]).await?;
            let mut pending = vec![remote];
            let mut visited = 0usize;
            while let Some(dir) = pending.pop() {
                visited += 1;
                if visited > 4096 {
                    return Err("the FTP directory tree is too large to remove".to_string());
                }
                let data = conn.passive().await?;
                conn.send(&format!("LIST {dir}"), &[125, 150]).await?;
                let text = read_listing(data).await?;
                conn.finish_data().await?;
                for entry in parse_listing(&text) {
                    let child = format!("{dir}/{}", entry.name);
                    if entry.is_dir {
                        pending.push(child);
                    } else {
                        conn.send(&format!("DELE {child}"), &[250]).await?;
                    }
                }
                conn.send(&format!("RMD {dir}"), &[250]).await?;
            }
            let _ = conn.send("QUIT", &[221]).await;
            Ok(())
        })
    }

    fn rename<'a>(&'a self, from: &'a str, to: &'a str) -> VfsFuture<'a, ()> {
        Box::pin(async move {
            let from = self.remote_path(from)?;
            let to = self.remote_path(to)?;
            let mut conn = self.connect().await?;
            conn.send(&format!("RNFR {from}"), &[350]).await?;
            conn.send(&format!("RNTO {to}"), &[250]).await?;
            let _ = conn.send("QUIT", &[221]).await;
            Ok(())
        })
    }

    fn chmod<'a>(&'a self, _path: &'a str, _mode: u32) -> VfsFuture<'a, ()> {
        Box::pin(async { Err("FTP does not support POSIX permissions".to_string()) })
    }

    fn set_times<'a>(&'a self, path: &'a str, mtime: u64) -> VfsFuture<'a, ()> {
        Box::pin(async move {
            // `MFMT` is an extension; without it the copy simply keeps the
            // server's own timestamp, which is what FTP has always done.
            let remote = self.remote_path(path)?;
            let mut conn = self.connect().await?;
            if conn.features.mfmt {
                let stamp = format_timestamp(mtime);
                let _ = conn.send(&format!("MFMT {stamp} {remote}"), &[213]).await;
            }
            let _ = conn.send("QUIT", &[221]).await;
            Ok(())
        })
    }

    fn open<'a>(&'a self, path: &'a str, mode: OpenMode) -> VfsFuture<'a, Box<dyn VfsFile + Send>> {
        Box::pin(async move {
            let remote = self.remote_path(path)?;
            // Resuming appends at the end of what is already there, so the
            // stream starts at the current size rather than at zero.
            let start = match mode {
                OpenMode::Read => 0,
                OpenMode::Write { truncate: true } => 0,
                OpenMode::Write { truncate: false } => self.stat(path).await.map(|e| e.size).unwrap_or(0),
            };
            let conn = self.connect().await?;
            let mut file = FtpFile {
                conn: Some(conn),
                read: None,
                write: None,
                path: remote,
                mode,
                pos: start,
                active: false,
            };
            // Opening eagerly surfaces "no such file" here instead of on the
            // first read, which is what the editor's error state expects.
            match mode {
                OpenMode::Read => {
                    file.start_read(0).await?;
                    file.pos = 0;
                }
                OpenMode::Write { truncate } => file.start_write(!truncate).await?,
            }
            Ok(Box::new(file) as Box<dyn VfsFile + Send>)
        })
    }
}

/// `YYYYMMDDHHMMSS`, the format `MFMT` expects.
fn format_timestamp(unix: u64) -> String {
    let (year, month, day) = civil_from_days((unix / 86_400) as i64);
    let seconds = unix % 86_400;
    format!(
        "{year:04}{month:02}{day:02}{:02}{:02}{:02}",
        seconds / 3_600,
        (seconds % 3_600) / 60,
        seconds % 60
    )
}

async fn read_listing(mut data: TcpStream) -> Result<String, String> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    loop {
        let read = timeout(TRANSFER_TIMEOUT, data.read(&mut chunk))
            .await
            .map_err(|_| "timed out reading the FTP directory listing".to_string())?
            .map_err(|err| format!("FTP listing failed: {err}"))?;
        if read == 0 {
            break;
        }
        if buffer.len() + read > MAX_LISTING_BYTES {
            return Err("the FTP directory listing is too large".to_string());
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    Ok(String::from_utf8_lossy(&buffer).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};
    use tokio::net::TcpListener;

    #[test]
    fn parses_epsv_and_pasv_replies() {
        assert_eq!(
            parse_epsv_port("Entering Extended Passive Mode (|||6446|)"),
            Some(6446)
        );
        let (ip, port) = parse_pasv_reply("Entering Passive Mode (192,168,1,4,195,80)").unwrap();
        assert_eq!(ip.to_string(), "192.168.1.4");
        assert_eq!(port, 195 * 256 + 80);
        assert!(parse_pasv_reply("nonsense").is_none());
    }

    #[test]
    fn parses_mlsd_lines() {
        let entry = parse_mlsd_line("type=file;size=42;modify=20240131120000; report.txt")
            .expect("mlsd line");
        assert_eq!(entry.name, "report.txt");
        assert!(!entry.is_dir);
        assert_eq!(entry.size, 42);
        assert_eq!(entry.mtime, 1_706_702_400);
        assert_eq!(entry.mode, 0o644);

        let dir = parse_mlsd_line("type=dir;modify=20240131120000; Photos").unwrap();
        assert!(dir.is_dir);
        assert_eq!(dir.mode, 0o755);

        // Current and parent directory pseudo-entries are noise.
        assert!(parse_mlsd_line("type=cdir; . ").is_none());
        assert!(parse_mlsd_line("type=pdir; .. ").is_none());
    }

    #[test]
    fn parses_unix_and_dos_listing_lines() {
        let file = parse_unix_list_line("-rw-r--r-- 1 ftp ftp 1024 Jan 31 12:00 notes.txt")
            .expect("unix line");
        assert_eq!(file.name, "notes.txt");
        assert_eq!(file.size, 1024);
        assert_eq!(file.mode, 0o644);

        let dir = parse_unix_list_line("drwxr-xr-x 2 ftp ftp 4096 Jan 31 2024 archive").unwrap();
        assert!(dir.is_dir);
        assert_eq!(dir.mtime, unix_from_parts(2024, 1, 31, 0, 0, 0));

        let link =
            parse_unix_list_line("lrwxrwxrwx 1 ftp ftp 7 Jan 31 12:00 current -> v2").unwrap();
        assert_eq!(link.name, "current");
        assert!(!link.is_dir);

        let dos = parse_dos_list_line("01-31-24  12:00PM       <DIR>          photos").unwrap();
        assert!(dos.is_dir);
        assert_eq!(dos.mtime, unix_from_parts(2024, 1, 31, 12, 0, 0));

        // A bare `total` header must not become an entry.
        assert!(parse_listing("total 8").is_empty());
    }

    #[test]
    fn rejects_hosts_that_are_really_urls() {
        assert!(FtpVfs::new("", None, "", "", "").is_err());
        assert!(FtpVfs::new("ftp://example.com", None, "", "", "").is_err());
        assert!(FtpVfs::new("example.com/path", None, "", "", "").is_err());
        assert!(FtpVfs::new("example.com", Some(0), "", "", "").is_err());
        assert!(FtpVfs::new("example.com", None, "", "", "").is_ok());
    }

    #[test]
    fn maps_paths_under_the_configured_root() {
        let vfs = FtpVfs::new("host", None, "", "", "/pub/files/").unwrap();
        assert_eq!(vfs.remote_path("/").unwrap(), "/pub/files");
        assert_eq!(vfs.remote_path("/a/b.txt").unwrap(), "/pub/files/a/b.txt");
        assert_eq!(vfs.vfs_path("a/b.txt"), "/a/b.txt");
        assert!(vfs.remote_path("/a\nDELE x").is_err());

        let plain = FtpVfs::new("host", None, "", "", "").unwrap();
        assert_eq!(plain.remote_path("/").unwrap(), "/");
        assert_eq!(plain.remote_path("/a").unwrap(), "/a");
    }

    #[test]
    fn formats_mfmt_timestamps() {
        assert_eq!(format_timestamp(1_706_702_400), "20240131120000");
    }

    type ServerState = (
        Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
        Arc<Mutex<Vec<String>>>,
        Arc<Mutex<Vec<String>>>,
        bool,
    );

    /// A tiny in-memory FTP server: enough of the protocol for the client
    /// tests below, including `MLSD`, `LIST`, passive mode and every command
    /// the mount UI can reach.
    struct MockServer {
        port: u16,
        files: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
        dirs: Arc<Mutex<Vec<String>>>,
        requests: Arc<Mutex<Vec<String>>>,
        _handle: tokio::task::JoinHandle<()>,
    }

    impl MockServer {
        async fn start(mlsd: bool) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let files = Arc::new(Mutex::new(BTreeMap::new()));
            let dirs = Arc::new(Mutex::new(vec!["/".to_string()]));
            let requests = Arc::new(Mutex::new(Vec::new()));
            files
                .lock()
                .unwrap()
                .insert("/hello.txt".to_string(), b"hello ftp".to_vec());
            files
                .lock()
                .unwrap()
                .insert("/spaced name.txt".to_string(), b"spaces".to_vec());
            dirs.lock().unwrap().push("/folder".to_string());

            let state: ServerState = (
                Arc::clone(&files),
                Arc::clone(&dirs),
                Arc::clone(&requests),
                mlsd,
            );
            let handle = tokio::spawn(async move {
                loop {
                    let Ok((stream, _)) = listener.accept().await else {
                        break;
                    };
                    let state = state.clone();
                    tokio::spawn(async move {
                        let _ = serve(stream, state).await;
                    });
                }
            });
            Self {
                port,
                files,
                dirs,
                requests,
                _handle: handle,
            }
        }

        fn vfs(&self) -> FtpVfs {
            FtpVfs::new("127.0.0.1", Some(self.port), "user", "pass", "").unwrap()
        }

        fn commands(&self) -> Vec<String> {
            self.requests.lock().unwrap().clone()
        }
    }

    async fn serve(stream: TcpStream, state: ServerState) -> std::io::Result<()> {
        let (files, dirs, requests, mlsd) = state;
        let (read, mut write) = stream.into_split();
        let mut reader = BufReader::new(read);
        let mut data_listener: Option<TcpListener> = None;
        let mut rename_from: Option<String> = None;
        let mut restart_at = 0usize;
        write.write_all(b"220 mock ready\r\n").await?;

        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).await? == 0 {
                return Ok(());
            }
            let line = line.trim_end().to_string();
            if line.is_empty() {
                continue;
            }
            requests.lock().unwrap().push(line.clone());
            let (verb, argument) = match line.split_once(' ') {
                Some((verb, argument)) => (verb.to_ascii_uppercase(), argument.trim().to_string()),
                None => (line.to_ascii_uppercase(), String::new()),
            };

            match verb.as_str() {
                "USER" => write.write_all(b"331 password please\r\n").await?,
                "PASS" => write.write_all(b"230 logged in\r\n").await?,
                "FEAT" => {
                    let mut text = String::from("211-Features:\r\n");
                    if mlsd {
                        text.push_str(" MLSD\r\n MLST type*;size*;modify*;\r\n");
                    }
                    text.push_str(" SIZE\r\n MDTM\r\n UTF8\r\n211 End\r\n");
                    write.write_all(text.as_bytes()).await?;
                }
                "OPTS" | "TYPE" | "NOOP" => write.write_all(b"200 ok\r\n").await?,
                "EPSV" => {
                    let listener = TcpListener::bind("127.0.0.1:0").await?;
                    let port = listener.local_addr()?.port();
                    data_listener = Some(listener);
                    write
                        .write_all(
                            format!("229 Entering Extended Passive Mode (|||{port}|)\r\n")
                                .as_bytes(),
                        )
                        .await?;
                }
                "MLSD" | "LIST" => {
                    write.write_all(b"150 opening data\r\n").await?;
                    let entries = listing(&files, &dirs, &argument, mlsd);
                    if let Some(listener) = data_listener.take() {
                        let (mut data, _) = listener.accept().await?;
                        data.write_all(entries.as_bytes()).await?;
                        data.shutdown().await?;
                    }
                    write.write_all(b"226 done\r\n").await?;
                }
                "RETR" => {
                    let content = files.lock().unwrap().get(&argument).cloned();
                    match content {
                        Some(content) => {
                            write.write_all(b"150 opening data\r\n").await?;
                            if let Some(listener) = data_listener.take() {
                                let (mut data, _) = listener.accept().await?;
                                let start = restart_at.min(content.len());
                                data.write_all(&content[start..]).await?;
                                data.shutdown().await?;
                            }
                            write.write_all(b"226 done\r\n").await?;
                        }
                        None => write.write_all(b"550 no such file\r\n").await?,
                    }
                    restart_at = 0;
                }
                // Proves the client sent the offset; the RETR handler above
                // actually skips it.
                "REST" => {
                    restart_at = argument.parse().unwrap_or(0);
                    write.write_all(b"350 restarting\r\n").await?;
                }
                "STOR" | "APPE" => {
                    write.write_all(b"150 opening data\r\n").await?;
                    let mut buffer = Vec::new();
                    if let Some(listener) = data_listener.take() {
                        let (mut data, _) = listener.accept().await?;
                        data.read_to_end(&mut buffer).await?;
                    }
                    {
                        let mut files = files.lock().unwrap();
                        if verb == "APPE" {
                            files.entry(argument).or_default().extend(buffer);
                        } else {
                            files.insert(argument, buffer);
                        }
                    }
                    write.write_all(b"226 done\r\n").await?;
                }
                "SIZE" => {
                    let size = files
                        .lock()
                        .unwrap()
                        .get(&argument)
                        .map(|content| content.len());
                    match size {
                        Some(size) => write.write_all(format!("213 {size}\r\n").as_bytes()).await?,
                        None => write.write_all(b"550 no such file\r\n").await?,
                    }
                }
                "MLST" => {
                    let reply = {
                        let files = files.lock().unwrap();
                        let dirs = dirs.lock().unwrap();
                        if let Some(content) = files.get(&argument) {
                            Some(format!(
                                "250-Listing\r\n type=file;size={};modify=20240131120000; {argument}\r\n250 End\r\n",
                                content.len()
                            ))
                        } else if dirs.iter().any(|dir| dir == &argument) {
                            Some(format!(
                                "250-Listing\r\n type=dir;modify=20240131120000; {argument}\r\n250 End\r\n"
                            ))
                        } else {
                            None
                        }
                    };
                    match reply {
                        Some(reply) => write.write_all(reply.as_bytes()).await?,
                        None => write.write_all(b"550 no such path\r\n").await?,
                    }
                }
                "MDTM" => write.write_all(b"213 20240131120000\r\n").await?,
                "MKD" => {
                    dirs.lock().unwrap().push(argument.clone());
                    write
                        .write_all(format!("257 \"{argument}\" created\r\n").as_bytes())
                        .await?;
                }
                "RMD" => {
                    dirs.lock().unwrap().retain(|dir| dir != &argument);
                    write.write_all(b"250 removed\r\n").await?;
                }
                "DELE" => {
                    let removed = files.lock().unwrap().remove(&argument).is_some();
                    if removed {
                        write.write_all(b"250 deleted\r\n").await?;
                    } else {
                        write.write_all(b"550 no such file\r\n").await?;
                    }
                }
                "RNFR" => {
                    rename_from = Some(argument);
                    write.write_all(b"350 ready\r\n").await?;
                }
                "RNTO" => {
                    if let Some(from) = rename_from.take() {
                        let mut files = files.lock().unwrap();
                        if let Some(content) = files.remove(&from) {
                            files.insert(argument, content);
                        }
                    }
                    write.write_all(b"250 renamed\r\n").await?;
                }
                "QUIT" => {
                    write.write_all(b"221 bye\r\n").await?;
                    return Ok(());
                }
                _ => write.write_all(b"502 not implemented\r\n").await?,
            }
        }
    }

    fn listing(
        files: &Mutex<BTreeMap<String, Vec<u8>>>,
        dirs: &Mutex<Vec<String>>,
        path: &str,
        mlsd: bool,
    ) -> String {
        let prefix = if path == "/" {
            "/".to_string()
        } else {
            format!("{}/", path.trim_end_matches('/'))
        };
        let mut out = String::new();
        for (name, content) in files.lock().unwrap().iter() {
            let Some(child) = name.strip_prefix(prefix.as_str()) else {
                continue;
            };
            if child.is_empty() || child.contains('/') {
                continue;
            }
            if mlsd {
                out.push_str(&format!(
                    "type=file;size={};modify=20240131120000; {child}\r\n",
                    content.len()
                ));
            } else {
                out.push_str(&format!(
                    "-rw-r--r-- 1 ftp ftp {} Jan 31 12:00 {child}\r\n",
                    content.len()
                ));
            }
        }
        for dir in dirs.lock().unwrap().iter() {
            let Some(child) = dir.strip_prefix(prefix.as_str()) else {
                continue;
            };
            if child.is_empty() || child.contains('/') {
                continue;
            }
            if mlsd {
                out.push_str(&format!("type=dir;modify=20240131120000; {child}\r\n"));
            } else {
                out.push_str(&format!("drwxr-xr-x 2 ftp ftp 4096 Jan 31 12:00 {child}\r\n"));
            }
        }
        out
    }

    #[tokio::test]
    async fn lists_directories_over_mlsd() {
        let server = MockServer::start(true).await;
        let entries = server.vfs().list("/", false).await.unwrap();
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, vec!["folder", "hello.txt", "spaced name.txt"]);
        assert!(entries[0].is_dir);
        assert_eq!(entries[1].size, 9);
        assert_eq!(entries[1].path, "/hello.txt");
        assert_eq!(entries[1].mtime, 1_706_702_400);
        assert!(server.commands().iter().any(|line| line == "MLSD /"));
    }

    #[tokio::test]
    async fn falls_back_to_list_when_mlsd_is_missing() {
        let server = MockServer::start(false).await;
        let entries = server.vfs().list("/", true).await.unwrap();
        assert_eq!(entries.len(), 3);
        assert!(entries.iter().any(|entry| entry.name == "hello.txt"));
        assert!(server
            .commands()
            .iter()
            .any(|line| line.starts_with("LIST ")));
        assert!(!server.commands().iter().any(|line| line.starts_with("MLSD")));
    }

    #[tokio::test]
    async fn hides_dot_files_unless_asked() {
        let server = MockServer::start(true).await;
        server
            .files
            .lock()
            .unwrap()
            .insert("/.hidden".to_string(), b"x".to_vec());
        let vfs = server.vfs();
        let visible = vfs.list("/", false).await.unwrap();
        assert!(!visible.iter().any(|entry| entry.name == ".hidden"));
        let all = vfs.list("/", true).await.unwrap();
        assert!(all.iter().any(|entry| entry.name == ".hidden"));
    }

    #[tokio::test]
    async fn downloads_a_file_in_chunks() {
        let server = MockServer::start(true).await;
        let vfs = server.vfs();
        let mut file = vfs.open("/hello.txt", OpenMode::Read).await.unwrap();
        assert_eq!(&file.read(0, 4).await.unwrap(), b"hell");
        assert_eq!(&file.read(4, 64).await.unwrap(), b"o ftp");
        // Reading past the end returns an empty chunk rather than an error.
        assert!(file.read(9, 16).await.unwrap().is_empty());
        file.close().await.unwrap();
    }

    #[tokio::test]
    async fn uploads_and_replaces_a_file() {
        let server = MockServer::start(true).await;
        let vfs = server.vfs();
        let mut file = vfs
            .open("/upload.txt", OpenMode::Write { truncate: true })
            .await
            .unwrap();
        file.write(0, b"abc").await.unwrap();
        file.write(3, b"def").await.unwrap();
        file.close().await.unwrap();
        assert_eq!(
            server.files.lock().unwrap().get("/upload.txt").cloned(),
            Some(b"abcdef".to_vec())
        );
    }

    #[tokio::test]
    async fn resumes_an_upload_where_the_target_left_off() {
        let server = MockServer::start(true).await;
        server
            .files
            .lock()
            .unwrap()
            .insert("/resume.txt".to_string(), b"abc".to_vec());
        let mut file = server
            .vfs()
            .open("/resume.txt", OpenMode::Write { truncate: false })
            .await
            .unwrap();
        // The transfer engine resumes at the size it measured.
        file.write(3, b"def").await.unwrap();
        file.close().await.unwrap();
        assert_eq!(
            server.files.lock().unwrap().get("/resume.txt").cloned(),
            Some(b"abcdef".to_vec())
        );
        assert!(server.commands().iter().any(|line| line.starts_with("APPE ")));
    }

    #[tokio::test]
    async fn rejects_out_of_order_uploads() {
        let server = MockServer::start(true).await;
        let mut file = server
            .vfs()
            .open("/random.txt", OpenMode::Write { truncate: true })
            .await
            .unwrap();
        let error = file.write(8, b"x").await.unwrap_err();
        assert!(error.contains("sequential"), "{error}");
    }

    #[tokio::test]
    async fn restarts_a_download_at_the_requested_offset() {
        let server = MockServer::start(true).await;
        let mut file = server.vfs().open("/hello.txt", OpenMode::Read).await.unwrap();
        assert_eq!(&file.read(0, 2).await.unwrap(), b"he");
        // A non-sequential offset restarts the transfer with REST.
        assert_eq!(&file.read(6, 3).await.unwrap(), b"ftp");
        file.close().await.unwrap();
        assert!(server.commands().iter().any(|line| line == "REST 6"));
    }

    #[tokio::test]
    async fn reports_missing_files_with_the_server_message() {
        let server = MockServer::start(true).await;
        // `Box<dyn VfsFile>` is not Debug, so the error is inspected by hand.
        let error = match server.vfs().open("/missing.txt", OpenMode::Read).await {
            Ok(_) => panic!("a missing file must not open"),
            Err(error) => error,
        };
        assert!(error.contains("550"), "{error}");
    }

    #[tokio::test]
    async fn stats_files_and_directories() {
        let server = MockServer::start(true).await;
        let vfs = server.vfs();
        let file = vfs.stat("/hello.txt").await.unwrap();
        assert_eq!(file.name, "hello.txt");
        assert_eq!(file.size, 9);
        assert!(vfs.stat("/folder").await.unwrap().is_dir);
    }

    #[tokio::test]
    async fn creates_renames_and_removes_entries() {
        let server = MockServer::start(true).await;
        let vfs = server.vfs();
        vfs.mkdir("/newdir").await.unwrap();
        assert!(server
            .dirs
            .lock()
            .unwrap()
            .iter()
            .any(|dir| dir == "/newdir"));

        vfs.rename("/hello.txt", "/renamed.txt").await.unwrap();
        assert!(server.files.lock().unwrap().contains_key("/renamed.txt"));

        vfs.remove_file("/renamed.txt").await.unwrap();
        assert!(!server.files.lock().unwrap().contains_key("/renamed.txt"));

        // Removing a directory tree walks it depth first.
        server
            .files
            .lock()
            .unwrap()
            .insert("/folder/a.txt".to_string(), b"a".to_vec());
        server
            .files
            .lock()
            .unwrap()
            .insert("/folder/b.txt".to_string(), b"b".to_vec());
        vfs.remove_dir_all("/folder").await.unwrap();
        let remaining: Vec<String> = server.files.lock().unwrap().keys().cloned().collect();
        assert!(
            remaining.iter().all(|path| !path.starts_with("/folder/")),
            "{remaining:?}"
        );
        assert!(!server
            .dirs
            .lock()
            .unwrap()
            .iter()
            .any(|dir| dir == "/folder"));
    }

    #[tokio::test]
    async fn chmod_is_reported_as_unsupported() {
        let server = MockServer::start(true).await;
        let vfs = server.vfs();
        let error = vfs.chmod("/hello.txt", 0o600).await.unwrap_err();
        assert!(error.contains("POSIX"), "{error}");
        // set_times is best-effort and must not fail without MFMT.
        vfs.set_times("/hello.txt", 1_706_702_400).await.unwrap();
    }
}
