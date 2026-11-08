use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;

use portable_pty::{Child, MasterPty};
use russh::client::{Handle, Msg};
use russh::ChannelWriteHalf;
use tokio::sync::Mutex;

use crate::ssh::SshHandler;

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
pub struct AppState {
    pub sessions: Arc<Mutex<HashMap<String, Session>>>,
}