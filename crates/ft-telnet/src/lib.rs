//! Telnet transport. Sessions feed the same `TerminalSink` as SSH, PTY and
//! serial sessions; the IAC negotiation is handled here so the webview only
//! sees plain terminal bytes.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ft_core::TerminalSink;
use tokio::sync::Mutex;

const IAC: u8 = 255;
const SE: u8 = 240;
const SB: u8 = 250;
const WILL: u8 = 251;
const WONT: u8 = 252;
const DO: u8 = 253;
const DONT: u8 = 254;

const OPT_ECHO: u8 = 1;
const OPT_SGA: u8 = 3;
const OPT_NAWS: u8 = 31;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const READ_POLL: Duration = Duration::from_millis(200);

struct TelnetSession {
    writer: TcpStream,
    closed: Arc<AtomicBool>,
}

#[derive(Default, Clone)]
pub struct TelnetRegistry {
    sessions: Arc<Mutex<HashMap<String, TelnetSession>>>,
}

pub fn validate(host: &str, port: u16) -> Result<(), String> {
    if host.trim().is_empty() || host.len() > 255 || host.contains(char::is_whitespace) {
        return Err("telnet host is invalid".to_string());
    }
    if port == 0 {
        return Err("telnet port is invalid".to_string());
    }
    Ok(())
}

pub async fn open(
    sink: Arc<dyn TerminalSink>,
    registry: &TelnetRegistry,
    id: String,
    host: String,
    port: u16,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    validate(&host, port)?;
    if registry.sessions.lock().await.contains_key(&id) {
        return Err(format!("telnet session {id} already exists"));
    }
    let address = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|e| format!("failed to resolve {host}: {e}"))?
        .next()
        .ok_or_else(|| format!("no address found for {host}"))?;
    let stream = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT)
        .map_err(|e| format!("failed to connect to {address}: {e}"))?;
    stream.set_nodelay(true).ok();
    stream
        .set_read_timeout(Some(READ_POLL))
        .map_err(|e| e.to_string())?;
    let writer = stream.try_clone().map_err(|e| e.to_string())?;
    let mut writer = writer;
    let mut greeting = Vec::new();
    negotiation_reply_initial(&mut greeting, cols, rows);
    writer
        .write_all(&greeting)
        .map_err(|e| format!("failed to start telnet negotiation: {e}"))?;
    writer.flush().ok();

    let closed = Arc::new(AtomicBool::new(false));
    let event_id = id.clone();
    let reader_closed = closed.clone();
    std::thread::spawn(move || pump(stream, sink, event_id, reader_closed));

    registry
        .sessions
        .lock()
        .await
        .insert(id, TelnetSession { writer, closed });
    Ok(())
}

pub async fn write(registry: &TelnetRegistry, id: &str, data: String) -> Result<(), String> {
    write_raw(registry, id, data.as_bytes()).await
}

async fn write_raw(registry: &TelnetRegistry, id: &str, data: &[u8]) -> Result<(), String> {
    let mut sessions = registry.sessions.lock().await;
    let session = sessions
        .get_mut(id)
        .ok_or_else(|| "telnet session not found".to_string())?;
    let mut escaped = Vec::with_capacity(data.len() + 8);
    for byte in data {
        escaped.push(*byte);
        // A literal 0xFF is the IAC escape character; it must be doubled.
        if *byte == IAC {
            escaped.push(IAC);
        }
    }
    session
        .writer
        .write_all(&escaped)
        .map_err(|e| e.to_string())?;
    session.writer.flush().map_err(|e| e.to_string())
}

pub async fn resize(
    registry: &TelnetRegistry,
    id: &str,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let mut sessions = registry.sessions.lock().await;
    let session = sessions
        .get_mut(id)
        .ok_or_else(|| "telnet session not found".to_string())?;
    let mut frame = Vec::with_capacity(9);
    window_size(&mut frame, cols, rows);
    session
        .writer
        .write_all(&frame)
        .map_err(|e| e.to_string())?;
    session.writer.flush().map_err(|e| e.to_string())
}

pub async fn close(registry: &TelnetRegistry, id: &str) -> Result<(), String> {
    let session = registry.sessions.lock().await.remove(id);
    if let Some(session) = session {
        session.closed.store(true, Ordering::Relaxed);
        let _ = session.writer.shutdown(std::net::Shutdown::Both);
    }
    Ok(())
}

fn window_size(out: &mut Vec<u8>, cols: u16, rows: u16) {
    out.extend_from_slice(&[IAC, SB, OPT_NAWS]);
    for byte in [(cols >> 8) as u8, cols as u8, (rows >> 8) as u8, rows as u8] {
        out.push(byte);
        if byte == IAC {
            out.push(IAC);
        }
    }
    out.extend_from_slice(&[IAC, SE]);
}

fn negotiation_reply_initial(out: &mut Vec<u8>, cols: u16, rows: u16) {
    // Offer the options this client implements up front, then publish the
    // current window size so full-screen programs start with the right shape.
    out.extend_from_slice(&[IAC, WILL, OPT_SGA]);
    out.extend_from_slice(&[IAC, WILL, OPT_NAWS]);
    window_size(out, cols, rows);
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    Data,
    Iac,
    Verb(u8),
    Subneg,
    SubnegIac,
}

/// Strips IAC sequences from the server stream and answers the negotiations
/// this client supports. Unsupported options are refused, which keeps servers
/// from waiting on features we will not implement.
struct Parser {
    state: State,
    replies: Vec<u8>,
}

impl Parser {
    fn new() -> Self {
        Self {
            state: State::Data,
            replies: Vec::new(),
        }
    }

    fn feed(&mut self, input: &[u8], out: &mut Vec<u8>) {
        for byte in input {
            match self.state {
                State::Data => {
                    if *byte == IAC {
                        self.state = State::Iac;
                    } else {
                        out.push(*byte);
                    }
                }
                State::Iac => match *byte {
                    IAC => {
                        out.push(IAC);
                        self.state = State::Data;
                    }
                    WILL | WONT | DO | DONT => self.state = State::Verb(*byte),
                    SB => self.state = State::Subneg,
                    SE => self.state = State::Data,
                    _ => self.state = State::Data,
                },
                State::Verb(verb) => {
                    self.negotiate(verb, *byte);
                    self.state = State::Data;
                }
                State::Subneg => {
                    if *byte == IAC {
                        self.state = State::SubnegIac;
                    }
                }
                State::SubnegIac => {
                    self.state = if *byte == SE {
                        State::Data
                    } else {
                        State::Subneg
                    };
                }
            }
        }
    }

    fn negotiate(&mut self, verb: u8, option: u8) {
        let supported = matches!(option, OPT_ECHO | OPT_SGA | OPT_NAWS);
        match verb {
            WILL => {
                let reply = if supported { DO } else { DONT };
                self.replies.extend_from_slice(&[IAC, reply, option]);
            }
            DO => {
                let reply = if supported { WILL } else { WONT };
                self.replies.extend_from_slice(&[IAC, reply, option]);
            }
            _ => {}
        }
    }
}

fn pump(mut stream: TcpStream, sink: Arc<dyn TerminalSink>, id: String, closed: Arc<AtomicBool>) {
    let mut parser = Parser::new();
    let mut buf = [0u8; 4096];
    loop {
        if closed.load(Ordering::Relaxed) {
            break;
        }
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let mut out = Vec::with_capacity(n);
                parser.feed(&buf[..n], &mut out);
                if !out.is_empty() {
                    sink.output(&id, out);
                }
                if !parser.replies.is_empty() {
                    let replies = std::mem::take(&mut parser.replies);
                    if stream.write_all(&replies).is_err() {
                        break;
                    }
                }
            }
            Err(err)
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::Interrupted
                ) =>
            {
                continue;
            }
            Err(_) => break,
        }
    }
    sink.closed(&id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    struct CollectSink {
        data: std::sync::Mutex<Vec<u8>>,
        closed: AtomicBool,
    }

    impl Default for CollectSink {
        fn default() -> Self {
            Self {
                data: std::sync::Mutex::new(Vec::new()),
                closed: AtomicBool::new(false),
            }
        }
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
    fn strips_negotiation_and_answers_supported_options() {
        let mut parser = Parser::new();
        let mut out = Vec::new();
        parser.feed(b"hi\xff\xfb\x03bye", &mut out);
        assert_eq!(out, b"hibye");
        assert_eq!(parser.replies, vec![IAC, DO, OPT_SGA]);

        let mut out = Vec::new();
        parser.replies.clear();
        parser.feed(&[IAC, WILL, 99], &mut out);
        assert!(out.is_empty());
        assert_eq!(parser.replies, vec![IAC, DONT, 99]);
    }

    #[test]
    fn passes_escaped_iac_through_and_ignores_subnegotiation() {
        let mut parser = Parser::new();
        let mut out = Vec::new();
        parser.feed(
            &[b'a', IAC, IAC, b'b', IAC, SB, 24, 1, IAC, SE, b'c'],
            &mut out,
        );
        assert_eq!(out, vec![b'a', IAC, b'b', b'c']);
        assert!(parser.replies.is_empty());
    }

    #[tokio::test]
    async fn exchanges_bytes_with_a_listener() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(b"login: ").unwrap();
            let mut seen = Vec::new();
            let mut buf = [0u8; 64];
            let deadline = std::time::Instant::now() + Duration::from_secs(15);
            while std::time::Instant::now() < deadline {
                let n = stream.read(&mut buf).unwrap();
                if n == 0 {
                    break;
                }
                seen.extend_from_slice(&buf[..n]);
                if seen.windows(6).any(|window| window == b"root\xff\xff") {
                    break;
                }
            }
            // The client opens with WILL SGA, WILL NAWS and a NAWS frame; the
            // typed 0xFF arrives doubled because 255 is the IAC escape.
            assert!(seen.starts_with(&[IAC, WILL, OPT_SGA]));
            assert!(seen.windows(6).any(|window| window == b"root\xff\xff"));
            stream.write_all(b"ok\r\n").unwrap();
            stream.flush().unwrap();
        });

        let registry = TelnetRegistry::default();
        let sink = Arc::new(CollectSink::default());
        open(
            sink.clone(),
            &registry,
            "telnet-1".to_string(),
            "127.0.0.1".to_string(),
            port,
            80,
            24,
        )
        .await
        .unwrap();
        write_raw(&registry, "telnet-1", b"root\xff").await.unwrap();
        resize(&registry, "telnet-1", 100, 30).await.unwrap();

        // Shared CI runners can schedule the reader task well after the
        // handshake. Poll on tokio's timer, not a blocking sleep, or a
        // single-threaded runtime cannot run the reader while we wait.
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        while std::time::Instant::now() < deadline {
            if sink.data.lock().unwrap().ends_with(b"ok\r\n") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(sink.data.lock().unwrap().as_slice(), b"login: ok\r\n");
        close(&registry, "telnet-1").await.unwrap();
        server.join().unwrap();
        for _ in 0..200 {
            if sink.closed.load(Ordering::Relaxed) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(sink.closed.load(Ordering::Relaxed));
    }

    #[test]
    fn rejects_bad_targets() {
        assert!(validate("", 23).is_err());
        assert!(validate("host name", 23).is_err());
        assert!(validate("host", 0).is_err());
        assert!(validate("example.com", 23).is_ok());
    }
}
