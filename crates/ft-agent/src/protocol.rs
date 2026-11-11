//! Wire types shared by the service and its clients.
//!
//! Everything crosses the boundary as JSON with camelCase keys, which is what
//! the Rust side and any hand-written script both expect to read.

use serde::{Deserialize, Serialize};

/// What the service reports about itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    pub version: String,
    pub pid: u32,
    pub sessions: usize,
    pub uptime_secs: u64,
    /// False while the app is still starting; a client can wait on it.
    pub ready: bool,
}

/// One saved host, with the session serving it when one is open.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostSummary {
    pub id: String,
    pub label: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub protocol: String,
    pub production: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
}

/// One live connection. `id` is the handle every other call takes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub id: String,
    pub label: String,
    pub host: String,
    pub username: String,
    /// `ssh` or `local`; only `ssh` can answer [`ExecRequest`].
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecRequest {
    pub session: String,
    pub command: String,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// Where the call came from, so the audit line can say. Absent is treated as
    /// the window: the only caller that never has to introduce itself.
    #[serde(default)]
    pub origin: Option<String>,
}

/// A graded command and, when it ran, what it printed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecReport {
    /// `allow`, `ask` or `deny`, straight from the policy engine.
    pub verdict: String,
    /// `r0` to `r4`.
    pub risk: String,
    pub reasons: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
    /// `auto`, `ask` or `plan`.
    pub mode: String,
    /// Whether the command actually ran. An `ask` verdict never runs here:
    /// there is nobody to ask, so it stays unrun and says so.
    pub ran: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// True when the output hit the cap and was cut.
    pub truncated: bool,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckRequest {
    pub command: String,
    /// Grades under a session's host. Falls back to `host` when absent.
    #[serde(default)]
    pub session: Option<String>,
    #[serde(default)]
    pub host: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckReport {
    pub command: String,
    pub host: String,
    pub verdict: String,
    pub risk: String,
    pub reasons: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathRequest {
    pub session: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileBlob {
    pub path: String,
    pub size: u64,
    pub content_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadRequest {
    pub session: String,
    pub path: String,
    pub content_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadReport {
    pub path: String,
    pub written: u64,
}

/// Body of every non-2xx answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorBody {
    pub error: String,
    /// Stable machine-readable category: `auth`, `request`, `missing`,
    /// `backend`.
    pub kind: String,
}
