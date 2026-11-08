//! Loopback SSH fixture for automated and manual client verification.
//! Binds 127.0.0.1:2222 only; disposable account test / ferriterm-test.

use std::collections::HashMap;
use std::sync::Arc;

use russh::keys::PrivateKey;
use russh::server::{Auth, ChannelOpenHandle, Config, Handler, Msg, Server, Session};
use russh::{Channel, ChannelId, MethodKind, MethodSet, Pty, SshId};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{ChildStdin, Command};

const USER: &str = "test";
const PASSWORD: &str = "ferriterm-test";
const PROMPT: &str = "test@loopback:~$ ";
const BANNER: &str =
    "\r\nFerriterm loopback SSH test server\r\nType 'help' for the command whitelist.\r\n\r\n";

struct TestServer;

impl Server for TestServer {
    type Handler = TestHandler;

    fn new_client(&mut self, peer: Option<std::net::SocketAddr>) -> Self::Handler {
        println!("loopback: client {peer:?}");
        TestHandler {
            line: Vec::new(),
            sftp: HashMap::new(),
        }
    }
}

struct TestHandler {
    line: Vec<u8>,
    sftp: HashMap<ChannelId, ChildStdin>,
}

impl TestHandler {
    fn command(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
        line: &str,
    ) -> Result<bool, russh::Error> {
        let out = match line {
            "" => String::new(),
            "help" => "commands: help, whoami, pwd, uname, echo <text>, exit\r\n".to_string(),
            "whoami" => format!("{USER}\r\n"),
            "pwd" => "/home/test\r\n".to_string(),
            "uname" => "Ferriterm-TestShell loopback 0.1\r\n".to_string(),
            "exit" => {
                session.data(channel, "logout\r\n")?;
                session.close(channel)?;
                return Ok(false);
            }
            _ if line.starts_with("echo ") => format!("{}\r\n", &line[5..]),
            _ => format!("-testsh: {line}: command not found (try 'help')\r\n"),
        };
        session.data(channel, out)?;
        Ok(true)
    }
}

impl Handler for TestHandler {
    type Error = russh::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        Ok(if user == USER && password == PASSWORD {
            Auth::Accept
        } else {
            Auth::reject()
        })
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        _cols: u32,
        _rows: u32,
        _pix_w: u32,
        _pix_h: u32,
        _modes: &[(Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        session.data(channel, BANNER)?;
        session.data(channel, PROMPT)?;
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        channel: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if name != "sftp" {
            session.channel_failure(channel)?;
            return Ok(());
        }
        let root = sftp_root();
        std::fs::create_dir_all(&root)?;
        let readme = root.join("README.md");
        if !readme.exists() {
            std::fs::write(&readme, "# Ferriterm loopback SFTP\n")?;
        }
        let mut child = Command::new(sftp_server_path()?)
            .current_dir(&root)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        let stdin = child.stdin.take().expect("piped stdin");
        let mut stdout = child.stdout.take().expect("piped stdout");
        self.sftp.insert(channel, stdin);
        let handle = session.handle();
        tokio::spawn(async move {
            let mut buffer = vec![0u8; 32 * 1024];
            loop {
                match stdout.read(&mut buffer).await {
                    Ok(0) | Err(_) => break,
                    Ok(read) => {
                        if handle.data(channel, buffer[..read].to_vec()).await.is_err() {
                            break;
                        }
                    }
                }
            }
            let _ = handle.eof(channel).await;
        });
        session.channel_success(channel)?;
        Ok(())
    }

    async fn channel_close(
        &mut self,
        channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.sftp.remove(&channel);
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some(stdin) = self.sftp.get_mut(&channel) {
            if stdin.write_all(data).await.is_err() {
                self.sftp.remove(&channel);
            } else {
                let _ = stdin.flush().await;
            }
            return Ok(());
        }
        for &byte in data {
            match byte {
                b'\r' | b'\n' => {
                    session.data(channel, "\r\n")?;
                    let line = String::from_utf8_lossy(&self.line).trim().to_string();
                    self.line.clear();
                    if !self.command(channel, session, &line)? {
                        return Ok(());
                    }
                    session.data(channel, PROMPT)?;
                }
                0x7f | 0x08 => {
                    if self.line.pop().is_some() {
                        session.data(channel, "\x08 \x08")?;
                    }
                }
                0x03 => {
                    self.line.clear();
                    session.data(channel, "^C\r\n")?;
                    session.data(channel, PROMPT)?;
                }
                0x20..=0x7e => {
                    self.line.push(byte);
                    session.data(channel, vec![byte])?;
                }
                _ => {}
            }
        }
        Ok(())
    }
}

fn sftp_server_path() -> Result<std::path::PathBuf, std::io::Error> {
    let candidates = [
        "/usr/libexec/sftp-server",
        "/usr/lib/openssh/sftp-server",
        "/usr/lib/ssh/sftp-server",
    ];
    candidates
        .iter()
        .map(std::path::PathBuf::from)
        .find(|path| path.exists())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "sftp-server not found"))
}

fn sftp_root() -> std::path::PathBuf {
    std::env::temp_dir().join("ferriterm-sftp-fixture")
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let key_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| concat!(env!("CARGO_MANIFEST_DIR"), "/../testenv/host_key").to_string());
    let key = PrivateKey::from_openssh(std::fs::read(&key_path)?)?;
    let config = Arc::new(Config {
        server_id: SshId::Standard("SSH-2.0-FerritermTestEnv_0.1".into()),
        methods: MethodSet::from(&[MethodKind::Password][..]),
        keys: vec![key],
        ..Default::default()
    });
    println!("loopback sshd on 127.0.0.1:2222, user {USER}, password {PASSWORD}");
    TestServer
        .run_on_address(config, ("127.0.0.1", 2222))
        .await?;
    Ok(())
}
