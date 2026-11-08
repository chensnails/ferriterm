use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ft_core::{Session, SessionRegistry};
use russh::client;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf};
use tokio::sync::Mutex;

use crate::{join_path, FileEntry, OpenMode, Vfs, VfsFile, VfsFuture};

const SSH_FXP_INIT: u8 = 1;
const SSH_FXP_VERSION: u8 = 2;
const SSH_FXP_OPEN: u8 = 3;
const SSH_FXP_CLOSE: u8 = 4;
const SSH_FXP_READ: u8 = 5;
const SSH_FXP_WRITE: u8 = 6;
const SSH_FXP_LSTAT: u8 = 7;
const SSH_FXP_SETSTAT: u8 = 9;
const SSH_FXP_OPENDIR: u8 = 11;
const SSH_FXP_READDIR: u8 = 12;
const SSH_FXP_REMOVE: u8 = 13;
const SSH_FXP_MKDIR: u8 = 14;
const SSH_FXP_RMDIR: u8 = 15;
const SSH_FXP_REALPATH: u8 = 16;
const SSH_FXP_STAT: u8 = 17;
const SSH_FXP_RENAME: u8 = 18;

const SSH_FXP_STATUS: u8 = 101;
const SSH_FXP_HANDLE: u8 = 102;
const SSH_FXP_DATA: u8 = 103;
const SSH_FXP_NAME: u8 = 104;
const SSH_FXP_ATTRS: u8 = 105;

const STATUS_EOF: u32 = 1;

const ATTR_SIZE: u32 = 0x1;
const ATTR_UIDGID: u32 = 0x2;
const ATTR_PERMISSIONS: u32 = 0x4;
const ATTR_ACMODTIME: u32 = 0x8;
const ATTR_EXTENDED: u32 = 0x8000_0000;

const OPEN_READ: u32 = 0x1;
const OPEN_WRITE: u32 = 0x2;
const OPEN_CREAT: u32 = 0x8;
const OPEN_TRUNC: u32 = 0x10;

const S_IFMT: u32 = 0o170000;
const S_IFDIR: u32 = 0o040000;
const S_IFLNK: u32 = 0o120000;

type SftpStream = russh::ChannelStream<client::Msg>;

struct SftpIo {
    reader: BufReader<ReadHalf<SftpStream>>,
    writer: WriteHalf<SftpStream>,
}

impl SftpIo {
    async fn write_packet(&mut self, packet_type: u8, body: &[u8]) -> Result<(), String> {
        let length = (body.len() + 1) as u32;
        let mut framed = Vec::with_capacity(body.len() + 5);
        framed.extend_from_slice(&length.to_be_bytes());
        framed.push(packet_type);
        framed.extend_from_slice(body);
        self.writer
            .write_all(&framed)
            .await
            .map_err(|e| format!("sftp write failed: {e}"))?;
        self.writer
            .flush()
            .await
            .map_err(|e| format!("sftp flush failed: {e}"))
    }

    async fn read_packet(&mut self) -> Result<(u8, Vec<u8>), String> {
        let mut length = [0u8; 4];
        self.reader
            .read_exact(&mut length)
            .await
            .map_err(|e| format!("sftp read failed: {e}"))?;
        let length = u32::from_be_bytes(length) as usize;
        if length == 0 || length > 16 * 1024 * 1024 {
            return Err(format!("invalid sftp packet length {length}"));
        }
        let mut packet = vec![0u8; length];
        self.reader
            .read_exact(&mut packet)
            .await
            .map_err(|e| format!("sftp read failed: {e}"))?;
        let packet_type = packet[0];
        Ok((packet_type, packet[1..].to_vec()))
    }
}

pub struct SftpConnection {
    io: Mutex<SftpIo>,
    next_id: AtomicU32,
}

impl SftpConnection {
    pub async fn open(sessions: &SessionRegistry, session_id: &str) -> Result<Arc<Self>, String> {
        let handle = {
            let guard = sessions.sessions.lock().await;
            match guard.get(session_id) {
                Some(Session::Ssh(session)) => session.handle.clone(),
                _ => return Err("ssh session not found".to_string()),
            }
        };
        let channel = handle
            .channel_open_session()
            .await
            .map_err(|e| format!("failed to open sftp channel: {e}"))?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|e| format!("failed to start sftp subsystem: {e}"))?;
        let (read, write) = tokio::io::split(channel.into_stream());
        let connection = Arc::new(Self {
            io: Mutex::new(SftpIo {
                reader: BufReader::new(read),
                writer: write,
            }),
            next_id: AtomicU32::new(1),
        });
        tokio::time::timeout(Duration::from_secs(15), connection.handshake())
            .await
            .map_err(|_| "sftp handshake timed out".to_string())??;
        Ok(connection)
    }

    async fn handshake(&self) -> Result<(), String> {
        let mut io = self.io.lock().await;
        let mut body = Vec::new();
        put_u32(&mut body, 3);
        io.write_packet(SSH_FXP_INIT, &body).await?;
        let (packet_type, body) = io.read_packet().await?;
        if packet_type != SSH_FXP_VERSION {
            return Err(format!("unexpected sftp packet {packet_type}"));
        }
        let mut reader = PacketReader::new(&body);
        let version = reader.u32()?;
        if version < 3 {
            return Err(format!("unsupported sftp version {version}"));
        }
        Ok(())
    }

    async fn request(&self, packet_type: u8, payload: &[u8]) -> Result<(u8, Vec<u8>), String> {
        let mut io = self.io.lock().await;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut body = Vec::with_capacity(payload.len() + 4);
        put_u32(&mut body, id);
        body.extend_from_slice(payload);
        io.write_packet(packet_type, &body).await?;
        loop {
            let (response_type, response) = io.read_packet().await?;
            let mut reader = PacketReader::new(&response);
            let response_id = reader.u32()?;
            if response_id != id {
                continue;
            }
            return Ok((response_type, reader.rest().to_vec()));
        }
    }

    async fn open_dir(&self, path: &str) -> Result<Vec<FileEntry>, String> {
        let handle = self.handle_request(SSH_FXP_OPENDIR, path).await?;
        let mut entries = Vec::new();
        loop {
            let (packet_type, body) = self
                .request(SSH_FXP_READDIR, &encode_bytes(&handle))
                .await?;
            match packet_type {
                SSH_FXP_NAME => entries.extend(parse_names(&body, path)?),
                SSH_FXP_STATUS => {
                    if status_code(&body)? == STATUS_EOF {
                        break;
                    }
                    return Err(status_message(&body)?);
                }
                other => return Err(format!("unexpected sftp packet {other}")),
            }
        }
        let _ = self.request(SSH_FXP_CLOSE, &encode_bytes(&handle)).await;
        Ok(entries)
    }

    async fn handle_request(&self, packet_type: u8, path: &str) -> Result<Vec<u8>, String> {
        let (response_type, body) = self.request(packet_type, &encode_string(path)).await?;
        match response_type {
            SSH_FXP_HANDLE => {
                let mut reader = PacketReader::new(&body);
                Ok(reader.bytes()?)
            }
            SSH_FXP_STATUS => Err(status_message(&body)?),
            other => Err(format!("unexpected sftp packet {other}")),
        }
    }

    async fn stat_raw(&self, path: &str, follow: bool) -> Result<FileEntry, String> {
        let packet = if follow { SSH_FXP_STAT } else { SSH_FXP_LSTAT };
        let (response_type, body) = self.request(packet, &encode_string(path)).await?;
        let attrs = match response_type {
            SSH_FXP_ATTRS => parse_attrs(&body)?,
            SSH_FXP_STATUS => return Err(status_message(&body)?),
            other => return Err(format!("unexpected sftp packet {other}")),
        };
        Ok(entry_from_attrs(name_of(path), path, attrs))
    }

    async fn realpath(&self, path: &str) -> Result<String, String> {
        let (response_type, body) = self.request(SSH_FXP_REALPATH, &encode_string(path)).await?;
        match response_type {
            SSH_FXP_NAME => {
                let mut reader = PacketReader::new(&body);
                let count = reader.u32()?;
                if count == 0 {
                    return Err("sftp realpath returned no entries".to_string());
                }
                let name = reader.string()?;
                Ok(name)
            }
            SSH_FXP_STATUS => Err(status_message(&body)?),
            other => Err(format!("unexpected sftp packet {other}")),
        }
    }

    async fn simple_path_request(&self, packet_type: u8, path: &str) -> Result<(), String> {
        let (response_type, body) = self.request(packet_type, &encode_string(path)).await?;
        expect_ok(response_type, &body)
    }

    async fn remove_dir_all(&self, path: &str) -> Result<(), String> {
        let entry = self.stat_raw(path, false).await?;
        if !entry.is_dir || entry.is_symlink {
            return self.simple_path_request(SSH_FXP_REMOVE, path).await;
        }
        for child in self.open_dir(path).await? {
            if child.name == "." || child.name == ".." {
                continue;
            }
            Box::pin(self.remove_dir_all(&child.path)).await?;
        }
        self.simple_path_request(SSH_FXP_RMDIR, path).await
    }

    async fn chmod(&self, path: &str, mode: u32) -> Result<(), String> {
        let mut body = encode_string(path);
        put_u32(&mut body, ATTR_PERMISSIONS);
        put_u32(&mut body, mode & 0o7777);
        let (response_type, response) = self.request(SSH_FXP_SETSTAT, &body).await?;
        expect_ok(response_type, &response)
    }

    async fn open_file(&self, path: &str, mode: OpenMode) -> Result<Vec<u8>, String> {
        let flags = match mode {
            OpenMode::Read => OPEN_READ,
            OpenMode::Write { truncate } => {
                let mut flags = OPEN_WRITE | OPEN_CREAT;
                if truncate {
                    flags |= OPEN_TRUNC;
                }
                flags
            }
        };
        // OPEN payload: filename + pflags + attrs (attrs flags = 0).
        let mut body = encode_string(path);
        put_u32(&mut body, flags);
        put_u32(&mut body, 0);
        let (response_type, response) = self.request(SSH_FXP_OPEN, &body).await?;
        match response_type {
            SSH_FXP_HANDLE => {
                let mut reader = PacketReader::new(&response);
                Ok(reader.bytes()?)
            }
            SSH_FXP_STATUS => Err(status_message(&response)?),
            other => Err(format!("unexpected sftp packet {other}")),
        }
    }

    async fn read_file(&self, handle: &[u8], offset: u64, len: u32) -> Result<Vec<u8>, String> {
        let mut body = encode_bytes(handle);
        put_u64(&mut body, offset);
        put_u32(&mut body, len);
        let (response_type, response) = self.request(SSH_FXP_READ, &body).await?;
        match response_type {
            SSH_FXP_DATA => {
                let mut reader = PacketReader::new(&response);
                reader.bytes()
            }
            SSH_FXP_STATUS => {
                if status_code(&response)? == STATUS_EOF {
                    Ok(Vec::new())
                } else {
                    Err(status_message(&response)?)
                }
            }
            other => Err(format!("unexpected sftp packet {other}")),
        }
    }

    async fn write_file(&self, handle: &[u8], offset: u64, data: &[u8]) -> Result<(), String> {
        let mut body = encode_bytes(handle);
        put_u64(&mut body, offset);
        body.extend_from_slice(&encode_bytes(data));
        let (response_type, response) = self.request(SSH_FXP_WRITE, &body).await?;
        expect_ok(response_type, &response)
    }

    async fn close_file(&self, handle: &[u8]) -> Result<(), String> {
        let (response_type, response) = self.request(SSH_FXP_CLOSE, &encode_bytes(handle)).await?;
        expect_ok(response_type, &response)
    }
}

pub struct SftpFile {
    connection: Arc<SftpConnection>,
    handle: Vec<u8>,
}

impl VfsFile for SftpFile {
    fn read<'a>(&'a mut self, offset: u64, len: u32) -> VfsFuture<'a, Vec<u8>> {
        Box::pin(async move { self.connection.read_file(&self.handle, offset, len).await })
    }

    fn write<'a>(&'a mut self, offset: u64, data: &'a [u8]) -> VfsFuture<'a, ()> {
        Box::pin(async move { self.connection.write_file(&self.handle, offset, data).await })
    }

    fn close<'a>(&'a mut self) -> VfsFuture<'a, ()> {
        Box::pin(async move { self.connection.close_file(&self.handle).await })
    }
}

#[derive(Clone)]
pub struct SftpVfs {
    connection: Arc<SftpConnection>,
}

impl SftpVfs {
    pub fn new(connection: Arc<SftpConnection>) -> Self {
        Self { connection }
    }
}

impl Vfs for SftpVfs {
    fn list<'a>(&'a self, path: &'a str, show_hidden: bool) -> VfsFuture<'a, Vec<FileEntry>> {
        Box::pin(async move {
            let mut entries = self.connection.open_dir(path).await?;
            entries.retain(|entry| entry.name != "." && entry.name != "..");
            if !show_hidden {
                entries.retain(|entry| !entry.name.starts_with('.'));
            }
            entries.sort_by(|a, b| {
                b.is_dir
                    .cmp(&a.is_dir)
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });
            Ok(entries)
        })
    }

    fn stat<'a>(&'a self, path: &'a str) -> VfsFuture<'a, FileEntry> {
        Box::pin(async move { self.connection.stat_raw(path, true).await })
    }

    fn home<'a>(&'a self) -> VfsFuture<'a, String> {
        Box::pin(async move { self.connection.realpath(".").await })
    }

    fn mkdir<'a>(&'a self, path: &'a str) -> VfsFuture<'a, ()> {
        Box::pin(async move {
            let mut body = encode_string(path);
            put_u32(&mut body, 0);
            let (response_type, response) = self.connection.request(SSH_FXP_MKDIR, &body).await?;
            expect_ok(response_type, &response)
        })
    }

    fn remove_file<'a>(&'a self, path: &'a str) -> VfsFuture<'a, ()> {
        Box::pin(async move {
            self.connection
                .simple_path_request(SSH_FXP_REMOVE, path)
                .await
        })
    }

    fn remove_dir_all<'a>(&'a self, path: &'a str) -> VfsFuture<'a, ()> {
        Box::pin(async move { self.connection.remove_dir_all(path).await })
    }

    fn rename<'a>(&'a self, from: &'a str, to: &'a str) -> VfsFuture<'a, ()> {
        Box::pin(async move {
            let mut body = encode_string(from);
            body.extend_from_slice(&encode_string(to));
            let (response_type, response) = self.connection.request(SSH_FXP_RENAME, &body).await?;
            expect_ok(response_type, &response)
        })
    }

    fn chmod<'a>(&'a self, path: &'a str, mode: u32) -> VfsFuture<'a, ()> {
        Box::pin(async move { self.connection.chmod(path, mode).await })
    }

    fn open<'a>(&'a self, path: &'a str, mode: OpenMode) -> VfsFuture<'a, Box<dyn VfsFile + Send>> {
        Box::pin(async move {
            let handle = self.connection.open_file(path, mode).await?;
            Ok(Box::new(SftpFile {
                connection: self.connection.clone(),
                handle,
            }) as Box<dyn VfsFile + Send>)
        })
    }
}

#[derive(Default)]
pub struct SftpRegistry {
    entries: Mutex<HashMap<String, Arc<SftpConnection>>>,
}

impl SftpRegistry {
    pub async fn get(
        &self,
        sessions: &SessionRegistry,
        session_id: &str,
    ) -> Result<Arc<SftpConnection>, String> {
        if let Some(connection) = self.entries.lock().await.get(session_id).cloned() {
            return Ok(connection);
        }
        let connection = SftpConnection::open(sessions, session_id).await?;
        self.entries
            .lock()
            .await
            .insert(session_id.to_string(), connection.clone());
        Ok(connection)
    }

    pub async fn invalidate(&self, session_id: &str) {
        self.entries.lock().await.remove(session_id);
    }
}

struct Attrs {
    size: u64,
    uid: u32,
    gid: u32,
    mode: u32,
    mtime: u64,
}

fn parse_attrs(body: &[u8]) -> Result<Attrs, String> {
    let mut reader = PacketReader::new(body);
    let flags = reader.u32()?;
    let mut attrs = Attrs {
        size: 0,
        uid: 0,
        gid: 0,
        mode: 0,
        mtime: 0,
    };
    if flags & ATTR_SIZE != 0 {
        attrs.size = reader.u64()?;
    }
    if flags & ATTR_UIDGID != 0 {
        attrs.uid = reader.u32()?;
        attrs.gid = reader.u32()?;
    }
    if flags & ATTR_PERMISSIONS != 0 {
        attrs.mode = reader.u32()?;
    }
    if flags & ATTR_ACMODTIME != 0 {
        let _atime = reader.u32()?;
        attrs.mtime = u64::from(reader.u32()?);
    }
    if flags & ATTR_EXTENDED != 0 {
        let count = reader.u32()?;
        for _ in 0..count {
            let _key = reader.string()?;
            let _value = reader.string()?;
        }
    }
    Ok(attrs)
}

fn parse_names(body: &[u8], base: &str) -> Result<Vec<FileEntry>, String> {
    let mut reader = PacketReader::new(body);
    let count = reader.u32()?;
    let mut entries = Vec::new();
    for _ in 0..count {
        let name = reader.string()?;
        let _long_name = reader.string()?;
        let attrs = parse_attrs_at(&mut reader)?;
        if name == "." || name == ".." {
            continue;
        }
        let path = join_path(base, &name);
        entries.push(entry_from_attrs(name, &path, attrs));
    }
    Ok(entries)
}

fn parse_attrs_at(reader: &mut PacketReader<'_>) -> Result<Attrs, String> {
    let flags = reader.u32()?;
    let mut attrs = Attrs {
        size: 0,
        uid: 0,
        gid: 0,
        mode: 0,
        mtime: 0,
    };
    if flags & ATTR_SIZE != 0 {
        attrs.size = reader.u64()?;
    }
    if flags & ATTR_UIDGID != 0 {
        attrs.uid = reader.u32()?;
        attrs.gid = reader.u32()?;
    }
    if flags & ATTR_PERMISSIONS != 0 {
        attrs.mode = reader.u32()?;
    }
    if flags & ATTR_ACMODTIME != 0 {
        let _atime = reader.u32()?;
        attrs.mtime = u64::from(reader.u32()?);
    }
    if flags & ATTR_EXTENDED != 0 {
        let count = reader.u32()?;
        for _ in 0..count {
            let _key = reader.string()?;
            let _value = reader.string()?;
        }
    }
    Ok(attrs)
}

fn entry_from_attrs(name: String, path: &str, attrs: Attrs) -> FileEntry {
    FileEntry {
        name,
        path: path.to_string(),
        size: attrs.size,
        mode: attrs.mode,
        uid: attrs.uid,
        gid: attrs.gid,
        mtime: attrs.mtime,
        is_dir: attrs.mode & S_IFMT == S_IFDIR,
        is_symlink: attrs.mode & S_IFMT == S_IFLNK,
    }
}

fn name_of(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return "/".to_string();
    }
    trimmed
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(trimmed)
        .to_string()
}

fn encode_string(value: &str) -> Vec<u8> {
    encode_bytes(value.as_bytes())
}

fn encode_bytes(value: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(value.len() + 4);
    put_u32(&mut out, value.len() as u32);
    out.extend_from_slice(value);
    out
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn status_code(body: &[u8]) -> Result<u32, String> {
    PacketReader::new(body).u32()
}

fn status_message(body: &[u8]) -> Result<String, String> {
    let mut reader = PacketReader::new(body);
    let code = reader.u32()?;
    let message = reader.string().unwrap_or_default();
    let message = message.trim();
    if message.is_empty() {
        Err(format!("sftp status error {code}"))
    } else {
        Err(format!("sftp error {code}: {message}"))
    }
}

fn expect_ok(packet_type: u8, body: &[u8]) -> Result<(), String> {
    match packet_type {
        SSH_FXP_STATUS => {
            if status_code(body)? == 0 {
                Ok(())
            } else {
                Err(status_message(body).unwrap_err())
            }
        }
        other => Err(format!("unexpected sftp packet {other}")),
    }
}

struct PacketReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> PacketReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn rest(&self) -> &'a [u8] {
        &self.data[self.pos..]
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], String> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or_else(|| "sftp packet overflow".to_string())?;
        if end > self.data.len() {
            return Err("truncated sftp packet".to_string());
        }
        let slice = &self.data[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn u32(&mut self) -> Result<u32, String> {
        let bytes = self.take(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn u64(&mut self) -> Result<u64, String> {
        let bytes = self.take(8)?;
        Ok(u64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    fn string(&mut self) -> Result<String, String> {
        let bytes = self.bytes()?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    fn bytes(&mut self) -> Result<Vec<u8>, String> {
        let len = self.u32()? as usize;
        Ok(self.take(len)?.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parent_path;

    fn attrs_bytes(size: u64, mode: u32, mtime: u32) -> Vec<u8> {
        let mut body = Vec::new();
        put_u32(
            &mut body,
            ATTR_SIZE | ATTR_UIDGID | ATTR_PERMISSIONS | ATTR_ACMODTIME,
        );
        put_u64(&mut body, size);
        put_u32(&mut body, 1000);
        put_u32(&mut body, 1000);
        put_u32(&mut body, mode);
        put_u32(&mut body, mtime);
        put_u32(&mut body, mtime);
        body
    }

    #[test]
    fn parses_attrs() {
        let attrs = parse_attrs(&attrs_bytes(42, S_IFDIR | 0o755, 99)).unwrap();
        assert_eq!(attrs.size, 42);
        assert_eq!(attrs.mode, S_IFDIR | 0o755);
        assert_eq!(attrs.mtime, 99);
    }

    #[test]
    fn parses_multiple_names() {
        let mut body = Vec::new();
        put_u32(&mut body, 2);
        body.extend_from_slice(&encode_string("a.txt"));
        body.extend_from_slice(&encode_string("-rw-r--r-- a.txt"));
        body.extend_from_slice(&attrs_bytes(10, 0o100644, 7));
        body.extend_from_slice(&encode_string("dir"));
        body.extend_from_slice(&encode_string("drwxr-xr-x dir"));
        body.extend_from_slice(&attrs_bytes(0, S_IFDIR | 0o755, 8));
        let entries = parse_names(&body, "/home/user").unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, "/home/user/a.txt");
        assert!(entries[1].is_dir);
    }

    #[test]
    fn derives_names_and_parents() {
        assert_eq!(name_of("/var/log/syslog"), "syslog");
        assert_eq!(name_of("/"), "/");
        assert_eq!(parent_path("/a/b").as_deref(), Some("/a"));
    }
}
