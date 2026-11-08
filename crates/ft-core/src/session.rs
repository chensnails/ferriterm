use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;

use portable_pty::{Child, MasterPty};
use russh::client::{self, Handle, Msg};
use russh::keys::PublicKeyOrCertificate;
use russh::ChannelWriteHalf;
use tokio::sync::Mutex;

pub struct SshHandler;

impl client::Handler for SshHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        // TODO(P3): known_hosts / TOFU verification before production. Accepting
        // any host key here exposes the connection to man-in-the-middle attacks.
        Ok(true)
    }
}

pub struct SshSession {
    pub handle: Handle<SshHandler>,
    pub writer: ChannelWriteHalf<Msg>,
}

pub struct PtySession {
    pub master: Box<dyn MasterPty + Send>,
    pub writer: Box<dyn Write + Send>,
    pub child: Box<dyn Child + Send + Sync>,
}

pub enum Session {
    Ssh(SshSession),
    Pty(PtySession),
}

#[derive(Default)]
pub struct SessionRegistry {
    pub sessions: Arc<Mutex<HashMap<String, Session>>>,
}
