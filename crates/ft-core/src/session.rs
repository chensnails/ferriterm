use std::collections::HashMap;
use std::future::Future;
use std::io::Write;
use std::pin::Pin;
use std::sync::Arc;

use portable_pty::{Child, MasterPty};
use russh::client::{self, Handle, Msg};
use russh::keys::ssh_key::PublicKey;
use russh::keys::PublicKeyOrCertificate;
use russh::ChannelWriteHalf;
use tokio::sync::Mutex;

pub trait ServerKeyVerifier: Send + Sync {
    fn verify<'a>(
        &'a self,
        host: &'a str,
        port: u16,
        key: &'a PublicKey,
    ) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>>;
}

pub trait RemoteForwardHandler: Send + Sync {
    fn forwarded_channel<'a>(
        &'a self,
        connected_address: &'a str,
        connected_port: u32,
        channel: russh::Channel<Msg>,
    ) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>>;
}

pub trait X11ChannelHandler: Send + Sync {
    fn x11_channel<'a>(
        &'a self,
        channel: russh::Channel<Msg>,
    ) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>>;
}

pub struct SshHandler {
    host: String,
    port: u16,
    verifier: Option<Arc<dyn ServerKeyVerifier>>,
    remote_forwards: Option<Arc<dyn RemoteForwardHandler>>,
    x11: Option<Arc<dyn X11ChannelHandler>>,
}

impl SshHandler {
    pub fn new(host: impl Into<String>, port: u16, verifier: Arc<dyn ServerKeyVerifier>) -> Self {
        Self {
            host: host.into(),
            port,
            verifier: Some(verifier),
            remote_forwards: None,
            x11: None,
        }
    }

    pub fn deny_all(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            verifier: None,
            remote_forwards: None,
            x11: None,
        }
    }

    pub fn with_remote_forwards(mut self, handler: Arc<dyn RemoteForwardHandler>) -> Self {
        self.remote_forwards = Some(handler);
        self
    }

    pub fn with_x11(mut self, handler: Arc<dyn X11ChannelHandler>) -> Self {
        self.x11 = Some(handler);
        self
    }
}

impl client::Handler for SshHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let owned;
        let key: &PublicKey = match server_public_key {
            PublicKeyOrCertificate::PublicKey { key, .. } => key,
            PublicKeyOrCertificate::Certificate(cert) => {
                owned = PublicKey::new(cert.public_key().clone(), "");
                &owned
            }
        };
        let Some(verifier) = self.verifier.clone() else {
            return Ok(false);
        };
        Ok(verifier.verify(&self.host, self.port, key).await)
    }

    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: russh::Channel<Msg>,
        connected_address: &str,
        connected_port: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: client::ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        let handled = match &self.remote_forwards {
            Some(handler) => {
                handler
                    .forwarded_channel(connected_address, connected_port, channel)
                    .await
            }
            None => false,
        };
        if handled {
            reply.accept().await;
        } else {
            reply
                .reject(russh::ChannelOpenFailure::AdministrativelyProhibited)
                .await;
        }
        Ok(())
    }

    async fn server_channel_open_x11(
        &mut self,
        channel: russh::Channel<Msg>,
        _originator_address: &str,
        _originator_port: u32,
        reply: client::ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        let handled = match &self.x11 {
            Some(handler) => handler.x11_channel(channel).await,
            None => false,
        };
        if handled {
            reply.accept().await;
        } else {
            reply
                .reject(russh::ChannelOpenFailure::AdministrativelyProhibited)
                .await;
        }
        Ok(())
    }
}

pub struct SshSession {
    pub handle: Arc<Handle<SshHandler>>,
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

#[derive(Default, Clone)]
pub struct SessionRegistry {
    pub sessions: Arc<Mutex<HashMap<String, Session>>>,
}
