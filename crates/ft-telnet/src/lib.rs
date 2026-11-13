//! Telnet transport. Sessions feed the same `TerminalSink` as SSH, PTY and
//! serial sessions; the IAC negotiation is handled here so the webview only
//! sees plain terminal bytes.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use ft_core::{CloseReason, GuardMap, SessionGuard, TerminalSink};

const IAC: u8 = 255;
const SE: u8 = 240;
const NOP: u8 = 241;
const AYT: u8 = 246;
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
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
/// DNS resolution used to be a blocking call inside an async command with no
/// deadline at all. Bound the whole resolve + connect phase.
const OPEN_TIMEOUT: Duration = Duration::from_secs(20);
/// A malformed server can leave a subnegotiation open forever. Cap it so the
/// parser always returns to the data state.
const MAX_SUBNEG_BYTES: usize = 4096;

struct TelnetSession {
    writer: TcpStream,
    guard: SessionGuard,
}

/// A `std` mutex: the reader is a blocking OS thread and must be able to
/// release its own entry after an abnormal disconnect. Writers take a clone of
/// the socket under the lock and never hold it across a blocking `write_all`.
#[derive(Default, Clone)]
pub struct TelnetRegistry {
    sessions: Arc<StdMutex<HashMap<String, TelnetSession>>>,
    guards: GuardMap,
}

fn sessions_map<T>(lock: &Arc<StdMutex<T>>) -> Result<std::sync::MutexGuard<'_, T>, String> {
    lock.lock().map_err(|_| "telnet registry is unavailable".to_string())
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
    let Some(guard) = registry.guards.begin(&id, sink.clone()) else {
        return Err("a telnet connection for this session is already in progress".to_string());
    };
    let stream = match connect(&host, port).await {
        Ok(stream) => stream,
        Err(error) => {
            registry.guards.take_if(&id, &guard);
            return Err(error);
        }
    };
    if let Err(error) = stream
        .set_read_timeout(Some(READ_POLL))
        .and_then(|_| stream.set_write_timeout(Some(WRITE_TIMEOUT)))
    {
        registry.guards.take_if(&id, &guard);
        return Err(format!("failed to configure telnet socket: {error}"));
    }
    let writer = match stream.try_clone() {
        Ok(writer) => writer,
        Err(error) => {
            registry.guards.take_if(&id, &guard);
            return Err(format!("failed to set up telnet writer: {error}"));
        }
    };
    let mut writer = writer;
    let mut greeting = Vec::new();
    negotiation_reply_initial(&mut greeting, cols, rows);
    if let Err(error) = writer.write_all(&greeting).and_then(|_| writer.flush()) {
        registry.guards.take_if(&id, &guard);
        return Err(format!("failed to start telnet negotiation: {error}"));
    }
    if guard.is_stopped() || !registry.guards.is_current(&id, &guard) {
        registry.guards.take_if(&id, &guard);
        return Err("connection cancelled".to_string());
    }

    // Publish before starting the reader, so an immediate EOF cannot emit a
    // closed event that arrives before open() has returned Ok.
    sessions_map(&registry.sessions)?.insert(
        id.clone(),
        TelnetSession {
            writer,
            guard: guard.clone(),
        },
    );

    let event_id = id.clone();
    let reader_guard = guard.clone();
    let reader_registry = registry.clone();
    let reader_sink = sink.clone();
    std::thread::spawn(move || {
        let (reason, message) = pump(stream, reader_sink, event_id.clone(), reader_guard.clone());
        finish(&reader_registry, &event_id, &reader_guard, reason, message);
    });
    Ok(())
}

/// Resolve off the Tokio worker and try every address (IPv6 then IPv4) rather
/// than giving up on the first one. The whole phase has a hard deadline.
async fn connect(host: &str, port: u16) -> Result<TcpStream, String> {
    let target = (host.to_string(), port);
    let resolve = tokio::task::spawn_blocking(move || {
        (target.0.as_str(), target.1)
            .to_socket_addrs()
            .map(|addresses| addresses.collect::<Vec<_>>())
            .map_err(|e| format!("failed to resolve {}: {e}", target.0))
    });
    let addresses = tokio::time::timeout(OPEN_TIMEOUT, resolve)
        .await
        .map_err(|_| format!("resolving {host} timed out"))?
        .map_err(|join| format!("failed to resolve {host}: {join}"))??;
    let addresses = if addresses.is_empty() {
        return Err(format!("no address found for {host}"));
    } else {
        addresses
    };
    let deadline = tokio::time::Instant::now() + OPEN_TIMEOUT;
    let mut last_error = String::new();
    for address in addresses {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err(format!("connecting to {host} timed out"));
        }
        let per_attempt = remaining.min(CONNECT_TIMEOUT);
        let attempt = tokio::task::spawn_blocking(move || TcpStream::connect_timeout(&address, per_attempt));
        match tokio::time::timeout(remaining, attempt).await {
            Ok(Ok(Ok(stream))) => {
                let _ = stream.set_nodelay(true);
                return Ok(stream);
            }
            Ok(Ok(Err(error))) => last_error = format!("{address}: {error}"),
            Ok(Err(join)) => last_error = format!("{address}: {join}"),
            Err(_) => return Err(format!("connecting to {host} timed out")),
        }
    }
    Err(format!(
        "failed to connect to {host}: {last_error} (check the address, port and firewall)"
    ))
}

pub async fn write(registry: &TelnetRegistry, id: &str, data: String) -> Result<(), String> {
    write_raw(registry, id, data.as_bytes()).await
}

async fn write_raw(registry: &TelnetRegistry, id: &str, data: &[u8]) -> Result<(), String> {
    let mut writer = {
        let sessions = sessions_map(&registry.sessions)?;
        match sessions.get(id) {
            Some(session) if !session.guard.is_stopped() => session.writer.try_clone(),
            Some(_) => return Err("telnet session is closed; reconnect to continue".to_string()),
            None => return Err("telnet session not found; reconnect to continue".to_string()),
        }
    }
    .map_err(|e| e.to_string())?;
    let mut escaped = Vec::with_capacity(data.len() + 8);
    for byte in data {
        escaped.push(*byte);
        // A literal 0xFF is the IAC escape character; it must be doubled.
        if *byte == IAC {
            escaped.push(IAC);
        }
    }
    writer
        .write_all(&escaped)
        .and_then(|_| writer.flush())
        .map_err(|e| e.to_string())
}

pub async fn resize(
    registry: &TelnetRegistry,
    id: &str,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let mut writer = {
        let sessions = sessions_map(&registry.sessions)?;
        match sessions.get(id) {
            Some(session) if !session.guard.is_stopped() => session.writer.try_clone(),
            Some(_) => return Err("telnet session is closed; reconnect to continue".to_string()),
            None => return Err("telnet session not found; reconnect to continue".to_string()),
        }
    }
    .map_err(|e| e.to_string())?;
    let mut frame = Vec::with_capacity(9);
    window_size(&mut frame, cols, rows);
    writer
        .write_all(&frame)
        .and_then(|_| writer.flush())
        .map_err(|e| e.to_string())
}

pub async fn close(registry: &TelnetRegistry, id: &str) -> Result<(), String> {
    let taken = registry.guards.take_any(id);
    let removed = {
        let mut sessions = sessions_map(&registry.sessions)?;
        match &taken {
            Some((_, guard)) if matches!(sessions.get(id), Some(s) if s.guard.same(guard)) => {
                sessions.remove(id)
            }
            _ => sessions.remove(id),
        }
    };
    if let Some(session) = removed {
        let _ = session.writer.shutdown(std::net::Shutdown::Both);
    }
    if let Some((sink, guard)) = taken {
        guard.finish(sink.as_ref(), id, CloseReason::User, None);
    }
    Ok(())
}

/// The reader releases its own entry. A server that hangs up used to leave a
/// stale socket in the registry, so reconnecting the same tab was impossible.
fn finish(
    registry: &TelnetRegistry,
    id: &str,
    guard: &SessionGuard,
    reason: CloseReason,
    message: Option<String>,
) {
    let Some((sink, owned_guard)) = registry.guards.take_if(id, guard) else {
        return; // a newer connection owns this id
    };
    if let Ok(mut sessions) = sessions_map(&registry.sessions) {
        if matches!(sessions.get(id), Some(s) if s.guard.same(guard)) {
            sessions.remove(id);
        }
    }
    owned_guard.finish(sink.as_ref(), id, reason, message);
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

#[derive(Clone, Copy, PartialEq, Debug)]
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
    /// Options we have already answered. Some servers re-announce WILL/DO on
    /// every prompt; without this set the client answers forever and the two
    /// ends can build a reply storm.
    answered: HashSet<(u8, u8)>,
    subneg_bytes: usize,
}

impl Parser {
    fn new() -> Self {
        Self {
            state: State::Data,
            replies: Vec::new(),
            answered: HashSet::new(),
            subneg_bytes: 0,
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
                    SB => {
                        self.subneg_bytes = 0;
                        self.state = State::Subneg;
                    }
                    AYT => {
                        // "Are You There?" — acknowledge so a stuck-looking
                        // session can be distinguished from a dead one.
                        self.replies.extend_from_slice(&[IAC, NOP]);
                        self.state = State::Data;
                    }
                    _ => self.state = State::Data,
                },
                State::Verb(verb) => {
                    self.negotiate(verb, *byte);
                    self.state = State::Data;
                }
                State::Subneg => {
                    self.subneg_bytes += 1;
                    if self.subneg_bytes > MAX_SUBNEG_BYTES {
                        self.state = State::Data;
                    } else if *byte == IAC {
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
        // Only answer a state change. Replying WILL->WONT also clears the
        // memory, so a server that legitimately re-negotiates is still heard.
        if matches!(verb, WONT | DONT) {
            self.answered.remove(&(WILL, option));
            self.answered.remove(&(DO, option));
            return;
        }
        if !self.answered.insert((verb, option)) {
            return;
        }
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

/// A peer that closes with unread data in its receive buffer produces a TCP
/// RST (the common case on Windows). That is an ordinary remote hangup, not a
/// fault in this client, and must not be surfaced as a scary transport error.
fn is_remote_hangup(error: &std::io::Error) -> bool {
    use std::io::ErrorKind;
    if matches!(
        error.kind(),
        ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
            | ErrorKind::BrokenPipe
            | ErrorKind::UnexpectedEof
            | ErrorKind::NotConnected
    ) {
        return true;
    }
    #[cfg(windows)]
    if matches!(
        error.raw_os_error(),
        Some(10054 | 10053 | 10058 | 109 | 232)
    ) {
        return true;
    }
    false
}

fn io_error(error: std::io::Error) -> (CloseReason, Option<String>) {
    let detail = error.to_string();
    if is_remote_hangup(&error) {
        (
            CloseReason::RemoteEof,
            Some(format!("server closed the connection ({detail})")),
        )
    } else {
        (CloseReason::TransportError, Some(detail))
    }
}

fn pump(
    mut stream: TcpStream,
    sink: Arc<dyn TerminalSink>,
    id: String,
    guard: SessionGuard,
) -> (CloseReason, Option<String>) {
    let mut parser = Parser::new();
    let mut buf = [0u8; 4096];
    loop {
        if guard.is_stopped() {
            return (CloseReason::User, None);
        }
        match stream.read(&mut buf) {
            Ok(0) => return (CloseReason::RemoteEof, Some("server closed the connection".into())),
            Ok(n) => {
                let mut out = Vec::with_capacity(n);
                parser.feed(&buf[..n], &mut out);
                if !out.is_empty() {
                    sink.output(&id, out);
                }
                if !parser.replies.is_empty() {
                    let replies = std::mem::take(&mut parser.replies);
                    if let Err(error) = stream.write_all(&replies) {
                        return io_error(error);
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
            Err(err) => return io_error(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ft_core::ClosedPayload;
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

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
    fn repeated_negotiation_does_not_create_a_reply_storm() {
        let mut parser = Parser::new();
        let mut out = Vec::new();
        for _ in 0..5 {
            parser.feed(&[IAC, WILL, OPT_SGA], &mut out);
        }
        assert_eq!(parser.replies, vec![IAC, DO, OPT_SGA]);
        // A genuine state change is still answered.
        parser.feed(&[IAC, WONT, OPT_SGA, IAC, WILL, OPT_SGA], &mut out);
        assert_eq!(
            parser.replies,
            vec![IAC, DO, OPT_SGA, IAC, DO, OPT_SGA]
        );
    }

    #[test]
    fn an_open_subnegotiation_is_bounded() {
        let mut parser = Parser::new();
        let mut out = Vec::new();
        let mut payload = vec![IAC, SB, 24];
        payload.extend(std::iter::repeat(1u8).take(MAX_SUBNEG_BYTES + 10));
        payload.push(b'x');
        parser.feed(&payload, &mut out);
        // After the cap the parser returns to Data and keeps passing bytes.
        assert!(out.contains(&b'x'));
        assert_eq!(parser.state, State::Data);
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
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            stream.write_all(b"login: ").unwrap();
            let mut seen = Vec::new();
            let mut buf = [0u8; 64];
            while !seen.windows(6).any(|window| window == b"root\xff\xff") {
                match stream.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => seen.extend_from_slice(&buf[..n]),
                    Err(_) => break,
                }
            }
            assert!(
                seen.starts_with(&[IAC, WILL, OPT_SGA]),
                "initial negotiation missing: {seen:?}"
            );
            assert!(
                seen.windows(6).any(|window| window == b"root\xff\xff"),
                "typed input missing: {seen:?}"
            );
            stream.write_all(b"ok\r\n").unwrap();
            stream.flush().unwrap();
            // Keep the socket open and drain until the client closes. Dropping
            // it here resets the connection on Windows and discards the bytes
            // just written while the reader thread is still starting up.
            loop {
                match stream.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
            }
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

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            if sink.data.lock().unwrap().ends_with(b"ok\r\n") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(sink.data.lock().unwrap().as_slice(), b"login: ok\r\n");
        close(&registry, "telnet-1").await.unwrap();
        server.join().unwrap();
        // Either the explicit close or the reader noticing EOF wins the race;
        // what matters is one single announcement, never two.
        let closes = sink.closes.lock().unwrap();
        assert_eq!(closes.len(), 1);
        assert!(matches!(
            closes[0],
            CloseReason::User | CloseReason::RemoteEof
        ));
        drop(closes);
        assert!(!registry.guards.contains("telnet-1"));
        assert!(sessions_map(&registry.sessions).unwrap().is_empty());
    }

    #[tokio::test]
    async fn abnormal_eof_clears_the_registry() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut buf = [0u8; 64];
            let _ = stream.read(&mut buf); // drain the greeting so close sends a FIN
            stream.shutdown(std::net::Shutdown::Both).unwrap();
        });
        let registry = TelnetRegistry::default();
        let sink = Arc::new(CollectSink::default());
        open(
            sink.clone(),
            &registry,
            "telnet-2".to_string(),
            "127.0.0.1".to_string(),
            port,
            80,
            24,
        )
        .await
        .unwrap();
        server.join().unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            if !sink.closes.lock().unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(*sink.closes.lock().unwrap(), vec![CloseReason::RemoteEof]);
        assert!(sessions_map(&registry.sessions).unwrap().is_empty());
        assert!(!registry.guards.contains("telnet-2"));
        // The id is free again, so a reconnect can reuse the tab.
        assert!(registry
            .guards
            .begin("telnet-2", sink as Arc<dyn TerminalSink>)
            .is_some());
    }

    #[tokio::test]
    async fn a_reset_is_reported_as_a_remote_hangup() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            drop(stream); // unread client data -> RST on Windows
        });
        let registry = TelnetRegistry::default();
        let sink = Arc::new(CollectSink::default());
        open(
            sink.clone(),
            &registry,
            "telnet-3".to_string(),
            "127.0.0.1".to_string(),
            port,
            80,
            24,
        )
        .await
        .unwrap();
        server.join().unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            if !sink.closes.lock().unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(*sink.closes.lock().unwrap(), vec![CloseReason::RemoteEof]);
        assert!(sessions_map(&registry.sessions).unwrap().is_empty());
        assert!(!registry.guards.contains("telnet-3"));
    }

    #[test]
    fn a_connection_reset_is_a_remote_hangup_not_a_fault() {
        assert!(is_remote_hangup(&std::io::Error::from(
            std::io::ErrorKind::ConnectionReset
        )));
        assert!(is_remote_hangup(&std::io::Error::from(
            std::io::ErrorKind::BrokenPipe
        )));
        assert!(!is_remote_hangup(&std::io::Error::from(
            std::io::ErrorKind::PermissionDenied
        )));
    }

    #[test]
    fn rejects_bad_targets() {
        assert!(validate("", 23).is_err());
        assert!(validate("host name", 23).is_err());
        assert!(validate("host", 0).is_err());
        assert!(validate("example.com", 23).is_ok());
    }
}
