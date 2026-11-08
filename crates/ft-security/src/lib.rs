//! Reserved security interfaces. Definitions land in P0 so later phases can
//! implement them without refactoring call sites: host-key TOFU in P3, audit
//! log and permission engine in M4. No concrete implementations yet.

#[derive(Debug, Clone)]
pub struct AuditEvent {
    pub actor: String,
    pub action: String,
    pub target: String,
    pub detail: String,
}

pub trait AuditLog: Send + Sync {
    fn record(&self, event: AuditEvent);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny,
    Ask,
}

#[derive(Debug, Clone)]
pub struct Action {
    pub kind: String,
    pub target: String,
}

pub trait PermissionEngine: Send + Sync {
    fn check(&self, action: &Action) -> Decision;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostKeyVerdict {
    Trusted,
    Unknown,
    Mismatch,
}

pub trait HostKeyVerifier: Send + Sync {
    fn verify(&self, host: &str, port: u16, fingerprint: &str) -> HostKeyVerdict;
}
