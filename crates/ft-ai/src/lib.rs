//! Bring-your-own-key AI client for Ferriterm.
//!
//! Two wire formats are supported, both from their public API documentation:
//! OpenAI-compatible `/chat/completions` (covers DashScope compatibility mode,
//! DeepSeek, GLM, Kimi, Ollama, …) and Anthropic `/messages`.
//!
//! The API key is passed per call and only ever placed in a request header; it
//! is scrubbed from every error string that could reach the UI. Context that
//! leaves the machine goes through [`redact`] unless the caller opts out.

use std::time::Duration;

use futures_util::StreamExt;
use regex::Regex;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------- providers

/// Wire protocol family. Every vendor we support collapses into one of these
/// three shapes, so adding a vendor is a preset change, not a new client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// `POST {base}/chat/completions` with `Authorization: Bearer`.
    #[default]
    OpenAi,
    /// `POST {base}/messages` with `x-api-key` + `anthropic-version`.
    Anthropic,
    /// Azure: `api-key` header, optional `api-version` query, deployment path.
    Azure,
}

impl Kind {
    pub fn id(self) -> &'static str {
        match self {
            Kind::OpenAi => "openai",
            Kind::Anthropic => "anthropic",
            Kind::Azure => "azure",
        }
    }

    pub fn from_id(value: &str) -> Kind {
        let value = value.trim();
        if value.eq_ignore_ascii_case("anthropic") {
            Kind::Anthropic
        } else if value.eq_ignore_ascii_case("azure") {
            Kind::Azure
        } else {
            Kind::OpenAi
        }
    }

    /// Azure speaks the OpenAI body shape, so most decisions only care about
    /// "is this the Anthropic dialect".
    fn is_anthropic(self) -> bool {
        matches!(self, Kind::Anthropic)
    }
}

/// Unknown protocol ids degrade to OpenAI instead of failing the whole provider
/// list — a stored config must never become unreadable after an upgrade.
impl<'de> Deserialize<'de> for Kind {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(Kind::from_id(&raw))
    }
}

/// Which request field carries the output cap. New OpenAI reasoning models
/// reject `max_tokens` and demand `max_completion_tokens`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum MaxTokensField {
    /// Send `max_tokens`, then switch automatically if the vendor rejects it.
    #[default]
    Auto,
    MaxTokens,
    MaxCompletionTokens,
}

impl MaxTokensField {
    pub fn from_id(value: &str) -> MaxTokensField {
        match value {
            "max_tokens" => MaxTokensField::MaxTokens,
            "maxCompletionTokens" | "max_completion_tokens" => MaxTokensField::MaxCompletionTokens,
            _ => MaxTokensField::Auto,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            MaxTokensField::Auto => "auto",
            MaxTokensField::MaxTokens => "max_tokens",
            MaxTokensField::MaxCompletionTokens => "maxCompletionTokens",
        }
    }
}

impl<'de> Deserialize<'de> for MaxTokensField {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(MaxTokensField::from_id(&raw))
    }
}

/// Optional reasoning-depth hint. `Off` means "do not send the field at all",
/// which is what every non-reasoning vendor needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Reasoning {
    #[default]
    Off,
    Low,
    Medium,
    High,
}

impl Reasoning {
    pub fn from_id(value: &str) -> Reasoning {
        match value {
            "low" => Reasoning::Low,
            "medium" => Reasoning::Medium,
            "high" => Reasoning::High,
            _ => Reasoning::Off,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Reasoning::Off => "off",
            Reasoning::Low => "low",
            Reasoning::Medium => "medium",
            Reasoning::High => "high",
        }
    }
}

impl<'de> Deserialize<'de> for Reasoning {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(Reasoning::from_id(&raw))
    }
}

/// One extra request header, e.g. OpenRouter's `HTTP-Referer` or a corporate
/// gateway token. Kept as a list so the order survives a round trip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HeaderPair {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub value: String,
}

/// `temperature` is the only sampling knob every protocol accepts; `top_p`,
/// `stop` and `reasoning_effort` are opt-in so a strict vendor never sees an
/// unknown field.
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    pub base_url: String,
    pub model: String,
    pub timeout_secs: u64,
    pub temperature: f32,
    pub max_tokens: u32,
    /// Optional cheaper model for one-shot command generation; empty = `model`.
    pub fast_model: String,
    /// Preset id this provider was created from (`openai`, `anthropic`, …).
    pub preset: String,

    // --- advanced: parameter differences between vendors -------------------
    /// Field name for the output cap (`auto` retries with the other one).
    pub max_tokens_field: MaxTokensField,
    /// Send `temperature`/`top_p`/penalties at all. Reasoning models reject them.
    #[serde(default = "default_true")]
    pub send_sampling: bool,
    pub top_p: Option<f32>,
    /// Stop sequences; always serialised as an array, omitted when empty.
    pub stop: Vec<String>,
    /// Merge the system prompt into the first user message (vendors without a
    /// `system` role).
    pub fold_system: bool,
    /// Ask for a usage block on the stream (`stream_options.include_usage`).
    pub stream_usage: bool,
    /// `reasoning_effort` for vendors that accept it; `Off` sends nothing.
    pub reasoning: Reasoning,
    /// Azure `api-version`; ignored by the other protocols.
    pub api_version: String,
    /// Extra request headers, applied after the protocol's own headers.
    pub extra_headers: Vec<HeaderPair>,
    /// Optional explicit proxy URL (`http://host:port`). Empty = system proxy.
    pub proxy: String,
}

impl Default for Provider {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            kind: Kind::OpenAi,
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4.1-mini".into(),
            timeout_secs: 60,
            temperature: 0.2,
            max_tokens: 1024,
            fast_model: String::new(),
            preset: "custom".into(),
            max_tokens_field: MaxTokensField::Auto,
            send_sampling: true,
            top_p: None,
            stop: Vec::new(),
            fold_system: false,
            stream_usage: false,
            reasoning: Reasoning::Off,
            api_version: String::new(),
            extra_headers: Vec::new(),
            proxy: String::new(),
        }
    }
}

impl Provider {
    /// Vault reference for this provider's key (`ai:<id>`).
    pub fn secret_ref(&self) -> String {
        format!("ai:{}", self.id)
    }

    pub fn command_model(&self) -> &str {
        let fast = self.fast_model.trim();
        if fast.is_empty() {
            &self.model
        } else {
            fast
        }
    }

    /// Clamp everything a vendor would reject, so a bad number in the UI can
    /// never turn into an opaque HTTP 400.
    pub fn sanitize(mut self) -> Self {
        self.base_url = self.base_url.trim().trim_end_matches('/').to_string();
        self.model = self.model.trim().to_string();
        self.api_version = self.api_version.trim().to_string();
        self.proxy = self.proxy.trim().to_string();
        self.timeout_secs = self.timeout_secs.clamp(5, 600);
        self.max_tokens = self.max_tokens.clamp(16, 200_000);
        let ceiling = if self.kind.is_anthropic() { 1.0 } else { 2.0 };
        self.temperature = if self.temperature.is_finite() {
            self.temperature.clamp(0.0, ceiling)
        } else {
            0.2
        };
        self.top_p = self
            .top_p
            .filter(|value| value.is_finite())
            .map(|value| value.clamp(0.0, 1.0));
        self.stop = self
            .stop
            .into_iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .take(8)
            .collect();
        self.extra_headers = self
            .extra_headers
            .into_iter()
            .map(|pair| HeaderPair {
                name: pair.name.trim().to_string(),
                value: pair.value.trim().to_string(),
            })
            .filter(|pair| !pair.name.is_empty())
            .collect();
        self
    }

    fn token_field(&self) -> &'static str {
        if self.kind.is_anthropic() {
            return "max_tokens";
        }
        match self.max_tokens_field {
            MaxTokensField::MaxCompletionTokens => "max_completion_tokens",
            _ => "max_tokens",
        }
    }
}

// ------------------------------------------------------------------- errors

#[derive(Debug, Clone, PartialEq)]
pub enum AiError {
    /// No API key stored for the provider.
    MissingKey,
    /// The provider answered with a non-success status.
    Http(u16, String),
    /// The request could not be sent (DNS, TLS, timeout, …).
    Request(String),
    /// The response was not the shape we expect.
    Response(String),
}

impl std::fmt::Display for AiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AiError::MissingKey => write!(f, "no API key saved for this provider"),
            AiError::Http(code, body) => write!(f, "HTTP {code}: {body}"),
            AiError::Request(msg) => write!(f, "{msg}"),
            AiError::Response(msg) => write!(f, "unexpected response: {msg}"),
        }
    }
}

impl std::error::Error for AiError {}

pub type Result<T> = std::result::Result<T, AiError>;

// ---------------------------------------------------------------- redaction

fn redactors() -> &'static [(Regex, &'static str)] {
    static ONCE: std::sync::OnceLock<Vec<(Regex, &'static str)>> = std::sync::OnceLock::new();
    ONCE.get_or_init(|| {
        let r = |pattern: &str| Regex::new(pattern).expect("valid redaction pattern");
        vec![
            (
                r(r"(?s)-----BEGIN [A-Z ]*PRIVATE KEY-----.*?(?:-----END [A-Z ]*PRIVATE KEY-----|$)"),
                "[REDACTED_PRIVATE_KEY]",
            ),
            (
                r(r"(?i)\b(authorization\s*:\s*(?:bearer|basic|token)\s+)[A-Za-z0-9._~+/=-]+"),
                "${1}[REDACTED]",
            ),
            (r(r"\b(?:sk|pk|rk)-[A-Za-z0-9_-]{8,}"), "[REDACTED_KEY]"),
            (
                r(r"\b(?:ghp|gho|ghu|ghs|ghr|github_pat)_[A-Za-z0-9_]{10,}"),
                "[REDACTED_TOKEN]",
            ),
            (r(r"\bxox[abprs]-[A-Za-z0-9-]{10,}"), "[REDACTED_TOKEN]"),
            (r(r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b"), "[REDACTED_AWS_KEY]"),
            (
                r(r"\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}"),
                "[REDACTED_JWT]",
            ),
            (
                r(r#"(?i)\b([A-Z0-9_]*(?:password|passwd|pwd|secret|token|api_?key|access_?key)[A-Z0-9_]*\s*[=:]\s*)('[^']*'|"[^"]*"|\S+)"#),
                "${1}[REDACTED]",
            ),
            (
                r(r"(?i)(--?(?:password|passwd|token|api-key|secret)[= ])\S+"),
                "${1}[REDACTED]",
            ),
            (
                r(r"(\b[a-z][a-z0-9+.-]*://[^:/\s@]+:)[^@\s/]+@"),
                "${1}[REDACTED]@",
            ),
        ]
    })
}

/// Remove secrets before the text leaves the machine. Returns the text and the
/// number of replacements.
pub fn redact(text: &str) -> (String, usize) {
    let mut out = text.to_string();
    let mut count = 0;
    for (re, replacement) in redactors() {
        count += re.find_iter(&out).count();
        out = re.replace_all(&out, *replacement).into_owned();
    }
    (out, count)
}

// ------------------------------------------------------------------ context

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scope {
    #[default]
    None,
    Command,
    Selection,
    Screen,
}

impl Scope {
    pub fn from_id(value: &str) -> Scope {
        match value {
            "command" => Scope::Command,
            "selection" => Scope::Selection,
            "screen" => Scope::Screen,
            _ => Scope::None,
        }
    }
}

/// Raw material offered to the context builder.
pub struct ContextInput<'a> {
    pub selection: Option<&'a str>,
    pub last_command: Option<&'a str>,
    pub screen: &'a [String],
}

#[derive(Debug, Clone, Copy)]
pub struct ContextOpts {
    pub scope: Scope,
    pub lines: usize,
    pub send_output: bool,
    pub redact: bool,
}

/// Serialisable form exchanged with the frontend.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ContextRequest {
    pub selection: Option<String>,
    pub last_command: Option<String>,
    pub screen: Vec<String>,
    /// `none` | `command` | `selection` | `screen`
    pub scope: String,
    pub lines: usize,
    pub send_output: bool,
    pub redact: bool,
}

impl ContextRequest {
    pub fn build(&self) -> (String, usize) {
        let input = ContextInput {
            selection: self.selection.as_deref(),
            last_command: self.last_command.as_deref(),
            screen: &self.screen,
        };
        build_context(
            &input,
            &ContextOpts {
                scope: Scope::from_id(&self.scope),
                lines: self.lines.clamp(1, 400),
                send_output: self.send_output,
                redact: self.redact,
            },
        )
    }
}

/// Build the context block that will be attached to a prompt.
///
/// With `send_output = false` only the last command line may be sent: selection
/// and screen content are treated as command output and withheld.
pub fn build_context(input: &ContextInput<'_>, opts: &ContextOpts) -> (String, usize) {
    let raw = match opts.scope {
        Scope::None => String::new(),
        Scope::Command => input.last_command.unwrap_or("").to_string(),
        Scope::Selection if opts.send_output => input.selection.unwrap_or("").to_string(),
        Scope::Screen if opts.send_output => {
            let lines: Vec<&String> = input
                .screen
                .iter()
                .filter(|line| !line.trim().is_empty())
                .collect();
            let start = lines.len().saturating_sub(opts.lines.max(1));
            lines[start..]
                .iter()
                .map(|line| line.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        }
        _ => input.last_command.unwrap_or("").to_string(),
    };
    if opts.redact {
        redact(&raw)
    } else {
        (raw, 0)
    }
}

// ------------------------------------------------------------------ prompts

pub const COMMAND_SYSTEM_PROMPT: &str = "You translate a natural-language request into ONE shell command for the user's terminal. \
Reply ONLY with JSON: {\"command\": \"...\", \"explanation\": \"one short sentence\"}. Never chain unrelated commands. \
Prefer safe, read-only commands when the request is ambiguous.";

pub const CHAT_SYSTEM_PROMPT: &str =
    "You are the Ferriterm terminal assistant. Reply in plain sentences without bold or headings. \
Never claim you ran anything. Any terminal context given to you is already redacted. \
Shell commands must go in fenced code blocks: one command per block, no leading $ and no comments. \
If the user only wants an explanation, include no code block.";

pub const EXPLAIN_SYSTEM_PROMPT: &str = "You explain why a shell command failed. Given the command, its exit status and the redacted tail of the \
terminal output, explain the most likely cause in two to five plain sentences and suggest exactly one fix command \
in its own fenced code block. Do not run anything.";

pub fn build_user_prompt(request: &str, shell: &str, host: &str, context: &str) -> String {
    let mut prompt = format!("Shell: {shell}\nHost: {host}\nRequest: {request}\n");
    if !context.is_empty() {
        prompt.push_str("Terminal context:\n```\n");
        prompt.push_str(context);
        prompt.push_str("\n```\n");
    }
    prompt
}

pub fn build_explain_prompt(command: &str, code: i32, host: &str, output_tail: &str) -> String {
    format!(
        "Host: {host}\nCommand: {command}\nExit status: {code}\nTerminal output (tail, redacted):\n```\n{output_tail}\n```"
    )
}

/// Drop whitespace and a dangling line-continuation backslash left behind by a
/// truncated reply, so the draft lands as one executable line.
fn trim_command(command: &str) -> String {
    let mut out = command.trim().to_string();
    while out.ends_with('\\') {
        out.pop();
        out = out.trim_end().to_string();
    }
    out
}

/// Read a JSON string value for `key` straight out of `source`, tolerating a
/// truncated or otherwise unparsable document. Handles the standard escapes and
/// returns whatever was read when the closing quote never arrives — which is the
/// normal shape for reasoning models whose JSON gets cut off by `max_tokens`.
fn salvage_json_string(source: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let mut from = 0;
    while let Some(offset) = source[from..].find(&needle) {
        let after = from + offset + needle.len();
        from = after;
        let rest = source[after..].trim_start();
        let Some(rest) = rest.strip_prefix(':') else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix('"') else {
            continue;
        };
        let mut out = String::new();
        let mut chars = rest.chars();
        while let Some(ch) = chars.next() {
            match ch {
                '\\' => match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('r') => out.push('\r'),
                    Some('u') => {
                        let hex: String = chars.by_ref().take(4).collect();
                        if let Some(decoded) =
                            u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32)
                        {
                            out.push(decoded);
                        }
                    }
                    Some(other) => out.push(other),
                    // Trailing backslash: the reply was cut mid-escape.
                    None => {}
                },
                '"' => break,
                _ => out.push(ch),
            }
        }
        let value = out.trim().to_string();
        if !value.is_empty() {
            return Some(value);
        }
    }
    None
}

/// Pull `{command, explanation}` out of a model reply (JSON, fenced code or the
/// first non-empty line).
pub fn extract_command(reply: &str) -> (String, String) {
    let trimmed = reply.trim();
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if start < end {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&trimmed[start..=end]) {
                if let Some(command) = value.get("command").and_then(|v| v.as_str()) {
                    return (
                        command.trim().to_string(),
                        value
                            .get("explanation")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .trim()
                            .to_string(),
                    );
                }
            }
        }
    }
    if let Some(open) = trimmed.find("```") {
        let rest = &trimmed[open + 3..];
        let body = rest.split_once('\n').map(|(_, tail)| tail).unwrap_or(rest);
        if let Some(close) = body.find("```") {
            return (
                trim_command(body[..close].trim().lines().next().unwrap_or("")),
                String::new(),
            );
        }
    }
    // Truncated JSON: the reply starts a `{...}` object but never closes it.
    // Reasoning models hit this routinely once the token budget is spent.
    if trimmed.starts_with('{') {
        if let Some(command) = salvage_json_string(trimmed, "command") {
            let explanation = salvage_json_string(trimmed, "explanation").unwrap_or_default();
            return (trim_command(&command), explanation);
        }
    }
    (
        trim_command(
            trimmed
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .trim_matches('`'),
        ),
        String::new(),
    )
}

// --------------------------------------------------------------------- HTTP

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reply {
    pub text: String,
    pub latency_ms: u64,
    pub model: String,
    /// i18n keys describing parameters that were dropped automatically because
    /// the vendor rejected them. Empty when the request went through as-is.
    #[serde(default)]
    pub notes: Vec<String>,
}

fn client(provider: &Provider) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(provider.timeout_secs.clamp(5, 600)));
    let proxy = provider.proxy.trim();
    if !proxy.is_empty() {
        builder = builder.proxy(
            reqwest::Proxy::all(proxy)
                .map_err(|err| AiError::Request(format!("invalid proxy: {err}")))?,
        );
    }
    builder
        .build()
        .map_err(|err| AiError::Request(err.to_string()))
}

/// Keep provider error bodies short and never echo the key back.
fn scrub(body: &str, key: &str) -> String {
    let mut text: String = body.chars().take(400).collect();
    if key.len() >= 4 {
        text = text.replace(key, "[key]");
    }
    text
}

fn base_of(provider: &Provider) -> Result<&str> {
    let base = provider.base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err(AiError::Request("base URL is empty".into()));
    }
    if !(base.starts_with("http://") || base.starts_with("https://")) {
        return Err(AiError::Request(
            "base URL must start with http:// or https://".into(),
        ));
    }
    Ok(base)
}

/// `2024-10-21` is the Azure GA version that accepts the classic deployment
/// path; the newer `/openai/v1` style does not care.
const AZURE_DEFAULT_VERSION: &str = "2024-10-21";

fn azure_suffix(provider: &Provider) -> String {
    let version = if provider.api_version.trim().is_empty() {
        AZURE_DEFAULT_VERSION
    } else {
        provider.api_version.trim()
    };
    format!("api-version={version}")
}

/// Normalise whatever the user typed into the exact chat endpoint.
///
/// Accepts a bare host, a versioned base (`/v1`, `/v2`, `/api/v3`), a URL that
/// already ends in `/chat/completions` or `/messages`, and both Azure styles
/// (resource root → deployment path, or `/openai/v1` → plain suffix).
pub fn chat_endpoint(provider: &Provider) -> Result<String> {
    let base = base_of(provider)?;
    let mut url = if base.ends_with("/chat/completions") || base.ends_with("/messages") {
        base.to_string()
    } else if provider.kind == Kind::Azure {
        if base.ends_with("/v1") || base.contains("/deployments/") {
            format!("{base}/chat/completions")
        } else {
            format!(
                "{base}/openai/deployments/{}/chat/completions",
                provider.model
            )
        }
    } else if provider.kind.is_anthropic() {
        format!("{base}/messages")
    } else {
        format!("{base}/chat/completions")
    };
    if provider.kind == Kind::Azure {
        let separator = if url.contains('?') { '&' } else { '?' };
        url.push(separator);
        url.push_str(&azure_suffix(provider));
    }
    Ok(url)
}

/// Model catalogue endpoint. Azure keeps models under `/openai/models` unless
/// the newer `/openai/v1` base is in use.
pub fn models_endpoint(provider: &Provider) -> Result<String> {
    let base = base_of(provider)?;
    if base.ends_with("/models") {
        return Ok(base.to_string());
    }
    let url = if provider.kind == Kind::Azure {
        if base.ends_with("/v1") {
            format!("{base}/models")
        } else {
            format!("{base}/openai/models?{}", azure_suffix(provider))
        }
    } else {
        format!("{base}/models")
    };
    Ok(url)
}

fn apply_auth(
    builder: reqwest::RequestBuilder,
    provider: &Provider,
    key: &str,
) -> reqwest::RequestBuilder {
    match provider.kind {
        Kind::OpenAi => {
            if key.is_empty() {
                builder
            } else {
                builder.header("authorization", format!("Bearer {key}"))
            }
        }
        Kind::Anthropic => builder
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01"),
        Kind::Azure => builder.header("api-key", key),
    }
}

fn apply_extra(builder: reqwest::RequestBuilder, provider: &Provider) -> reqwest::RequestBuilder {
    provider.extra_headers.iter().fold(builder, |acc, pair| {
        acc.header(pair.name.as_str(), pair.value.as_str())
    })
}

fn send_call(
    provider: &Provider,
    key: &str,
    body: &serde_json::Value,
) -> Result<reqwest::RequestBuilder> {
    let url = chat_endpoint(provider)?;
    let builder = client(provider)?
        .post(&url)
        .header("content-type", "application/json")
        .json(body);
    Ok(apply_extra(apply_auth(builder, provider, key), provider))
}

fn merge_system(wire: &mut Vec<serde_json::Value>, system: &str) {
    let system = system.trim();
    if system.is_empty() {
        return;
    }
    match wire.iter().position(|message| message["role"] == "user") {
        Some(index) => {
            let current = wire[index]["content"].as_str().unwrap_or("").to_string();
            wire[index]["content"] = serde_json::json!(format!("{system}\n\n{current}"));
        }
        None => wire.insert(0, serde_json::json!({ "role": "user", "content": system })),
    }
}

fn request_body(
    provider: &Provider,
    system: &str,
    messages: &[(String, String)],
) -> serde_json::Value {
    let mut wire: Vec<serde_json::Value> = messages
        .iter()
        .map(|(role, content)| serde_json::json!({ "role": role, "content": content }))
        .collect();
    let mut body = serde_json::Map::new();
    body.insert("model".into(), serde_json::json!(provider.model));
    body.insert(
        provider.token_field().into(),
        serde_json::json!(provider.max_tokens),
    );
    if provider.send_sampling {
        body.insert(
            "temperature".into(),
            serde_json::json!(provider.temperature),
        );
        if let Some(top_p) = provider.top_p {
            body.insert("top_p".into(), serde_json::json!(top_p));
        }
    }
    if !provider.stop.is_empty() {
        body.insert("stop".into(), serde_json::json!(provider.stop));
    }
    if provider.reasoning != Reasoning::Off && !provider.kind.is_anthropic() {
        body.insert(
            "reasoning_effort".into(),
            serde_json::json!(provider.reasoning.id()),
        );
    }
    if provider.kind.is_anthropic() && !provider.fold_system {
        if !system.trim().is_empty() {
            body.insert("system".into(), serde_json::json!(system));
        }
    } else if provider.fold_system {
        merge_system(&mut wire, system);
    } else if !system.trim().is_empty() {
        wire.insert(
            0,
            serde_json::json!({ "role": "system", "content": system }),
        );
    }
    body.insert("messages".into(), serde_json::json!(wire));
    serde_json::Value::Object(body)
}

fn reply_text(provider: &Provider, value: &serde_json::Value) -> Option<String> {
    if provider.kind.is_anthropic() {
        let blocks = value.get("content")?.as_array()?;
        return blocks
            .iter()
            .find(|block| {
                block
                    .get("type")
                    .and_then(|node| node.as_str())
                    .unwrap_or("text")
                    == "text"
            })
            .and_then(|block| block.get("text").and_then(|node| node.as_str()))
            .map(str::to_string);
    }
    value
        .pointer("/choices/0/message/content")
        .or_else(|| value.pointer("/choices/0/text"))
        .and_then(|node| node.as_str())
        .map(str::to_string)
}

fn response_model(provider: &Provider, value: &serde_json::Value) -> String {
    value
        .get("model")
        .and_then(|node| node.as_str())
        .unwrap_or(&provider.model)
        .to_string()
}

/// Decide how to dumb the request down after a rejection.
///
/// Vendors differ on the output-cap field name, on whether reasoning models
/// accept sampling parameters, and on whether a `system` role exists at all.
/// Each rule is one-shot: the retry repeats the same call with that single
/// difference removed, so we never wander into an unrelated configuration.
fn adjustment(provider: &Provider, status: u16, body: &str) -> Option<(Provider, &'static str)> {
    if status != 400 && status != 422 {
        return None;
    }
    let text = body.to_ascii_lowercase();
    if !provider.kind.is_anthropic()
        && provider.max_tokens_field != MaxTokensField::MaxCompletionTokens
        && text.contains("max_completion_tokens")
    {
        let mut next = provider.clone();
        next.max_tokens_field = MaxTokensField::MaxCompletionTokens;
        return Some((next, "ai.noteMaxTokens"));
    }
    let mentions_sampling = text.contains("temperature")
        || text.contains("top_p")
        || text.contains("sampling")
        || text.contains("penalty");
    let rejects = text.contains("unsupported")
        || text.contains("not support")
        || text.contains("does not support")
        || text.contains("unknown")
        || text.contains("invalid")
        || text.contains("unrecognized");
    if provider.send_sampling && mentions_sampling && rejects {
        let mut next = provider.clone();
        next.send_sampling = false;
        return Some((next, "ai.noteSampling"));
    }
    if !provider.fold_system
        && text.contains("system")
        && (text.contains("role") || text.contains("unsupported"))
    {
        let mut next = provider.clone();
        next.fold_system = true;
        return Some((next, "ai.noteSystem"));
    }
    None
}

fn remember(notes: &mut Vec<String>, note: &str) {
    if !notes.iter().any(|item| item == note) {
        notes.push(note.to_string());
    }
}

/// One non-streaming round trip.
pub async fn chat(
    provider: &Provider,
    key: &str,
    system: &str,
    user: &str,
    max_tokens: Option<u32>,
) -> Result<Reply> {
    let provider = provider.clone().sanitize();
    let messages = vec![("user".to_string(), user.to_string())];
    let mut current = provider.clone();
    let mut notes: Vec<String> = Vec::new();
    for attempt in 0..3 {
        let mut body = request_body(&current, system, &messages);
        if let Some(limit) = max_tokens {
            body[current.token_field()] = serde_json::json!(limit);
        }
        let started = std::time::Instant::now();
        let response = send_call(&current, key, &body)?
            .send()
            .await
            .map_err(|err| AiError::Request(scrub(&err.to_string(), key)))?;
        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            if attempt < 2 {
                if let Some((next, note)) = adjustment(&current, status.as_u16(), &text) {
                    remember(&mut notes, note);
                    current = next;
                    continue;
                }
            }
            return Err(AiError::Http(status.as_u16(), scrub(&text, key)));
        }
        let value: serde_json::Value = response
            .json()
            .await
            .map_err(|err| AiError::Response(scrub(&err.to_string(), key)))?;
        let text = reply_text(&current, &value)
            .ok_or_else(|| AiError::Response(scrub(&value.to_string(), key)))?;
        return Ok(Reply {
            text,
            latency_ms: started.elapsed().as_millis() as u64,
            model: response_model(&current, &value),
            notes,
        });
    }
    Err(AiError::Response("provider rejected every attempt".into()))
}

/// A minimal real request used by "test connection".
pub async fn test_connection(provider: &Provider, key: &str) -> Result<Reply> {
    chat(provider, key, "ping", "ping", Some(16)).await
}

/// List the models the vendor exposes. Used by the settings picker; a failure
/// here is never fatal because the model field stays hand-editable.
pub async fn models(provider: &Provider, key: &str) -> Result<Vec<String>> {
    let provider = provider.clone().sanitize();
    let url = models_endpoint(&provider)?;
    let builder = apply_extra(
        apply_auth(client(&provider)?.get(&url), &provider, key),
        &provider,
    );
    let response = builder
        .send()
        .await
        .map_err(|err| AiError::Request(scrub(&err.to_string(), key)))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(AiError::Http(status.as_u16(), scrub(&body, key)));
    }
    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|err| AiError::Response(scrub(&err.to_string(), key)))?;
    let mut out: Vec<String> = value
        .get("data")
        .and_then(|node| node.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("id").and_then(|node| node.as_str()))
                .map(str::to_string)
                .collect()
        })
        .or_else(|| {
            // Ollama's native shape: `{ "models": [{ "name": … }] }`.
            value
                .get("models")
                .and_then(|node| node.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.get("name").and_then(|node| node.as_str()))
                        .map(str::to_string)
                        .collect()
                })
        })
        .unwrap_or_default();
    out.sort();
    out.dedup();
    out.truncate(500);
    if out.is_empty() {
        return Err(AiError::Response("the vendor returned no models".into()));
    }
    Ok(out)
}

fn delta_text<'a>(provider: &Provider, value: &'a serde_json::Value) -> Option<&'a str> {
    if provider.kind.is_anthropic() {
        return value
            .pointer("/delta/text")
            .or_else(|| value.pointer("/content_block/delta/text"))
            .and_then(|node| node.as_str());
    }
    value
        .pointer("/choices/0/delta/content")
        .and_then(|node| node.as_str())
}

/// Streaming multi-turn chat. `on_delta` receives each text fragment and
/// returns whether to keep going.
pub async fn chat_stream<F>(
    provider: &Provider,
    key: &str,
    system: &str,
    messages: &[(String, String)],
    mut on_delta: F,
) -> Result<Reply>
where
    F: FnMut(&str) -> bool,
{
    let provider = provider.clone().sanitize();
    let mut current = provider.clone();
    let mut notes: Vec<String> = Vec::new();
    for attempt in 0..3 {
        let mut body = request_body(&current, system, messages);
        body["stream"] = serde_json::json!(true);
        if current.stream_usage && !current.kind.is_anthropic() {
            body["stream_options"] = serde_json::json!({ "include_usage": true });
        }
        let started = std::time::Instant::now();
        let response = send_call(&current, key, &body)?
            .header("accept", "text/event-stream")
            .send()
            .await
            .map_err(|err| AiError::Request(scrub(&err.to_string(), key)))?;
        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            if attempt < 2 {
                if let Some((next, note)) = adjustment(&current, status.as_u16(), &text) {
                    remember(&mut notes, note);
                    current = next;
                    continue;
                }
            }
            return Err(AiError::Http(status.as_u16(), scrub(&text, key)));
        }
        let is_sse = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .map(|value| value.contains("event-stream"))
            .unwrap_or(false);
        if !is_sse {
            let value: serde_json::Value = response
                .json()
                .await
                .map_err(|err| AiError::Response(scrub(&err.to_string(), key)))?;
            let text = reply_text(&current, &value)
                .ok_or_else(|| AiError::Response(scrub(&value.to_string(), key)))?;
            on_delta(&text);
            return Ok(Reply {
                text,
                latency_ms: started.elapsed().as_millis() as u64,
                model: response_model(&current, &value),
                notes,
            });
        }

        let mut stream = response.bytes_stream();
        let mut buffer: Vec<u8> = Vec::new();
        let mut text = String::new();
        let mut model = current.model.clone();
        'outer: while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|err| AiError::Request(scrub(&err.to_string(), key)))?;
            buffer.extend_from_slice(&chunk);
            while let Some(index) = buffer.iter().position(|byte| *byte == b'\n') {
                let line: Vec<u8> = buffer.drain(..=index).collect();
                let line = String::from_utf8_lossy(&line);
                let line = line.trim_end_matches(['\r', '\n']);
                let Some(data) = line.strip_prefix("data:") else {
                    continue;
                };
                let data = data.trim();
                if data.is_empty() {
                    continue;
                }
                if data == "[DONE]" {
                    break 'outer;
                }
                let Ok(value) = serde_json::from_str::<serde_json::Value>(data) else {
                    continue;
                };
                if let Some(next) = value.get("model").and_then(|node| node.as_str()) {
                    model = next.to_string();
                }
                if let Some(delta) = delta_text(&current, &value) {
                    text.push_str(delta);
                    if !on_delta(delta) {
                        break 'outer;
                    }
                }
                if value.get("type").and_then(|node| node.as_str()) == Some("message_stop") {
                    break 'outer;
                }
                if let Some(error) = value
                    .pointer("/error/message")
                    .and_then(|node| node.as_str())
                {
                    return Err(AiError::Request(scrub(error, key)));
                }
            }
        }
        return Ok(Reply {
            text,
            latency_ms: started.elapsed().as_millis() as u64,
            model,
            notes,
        });
    }
    Err(AiError::Response("provider rejected every attempt".into()))
}

// -------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_common_secrets_and_leaves_plain_text_alone() {
        let sample = "export OPENAI_API_KEY=sk-abcdefghijklmnop\n\
curl -H 'Authorization: Bearer abc.def.ghi' https://user:hunter2@host/x\n\
mysql --password=topsecret\n\
AKIAABCDEFGHIJKLMNOP\n\
-----BEGIN OPENSSH PRIVATE KEY-----\nAAAA\n-----END OPENSSH PRIVATE KEY-----";
        let (clean, count) = redact(sample);
        for leak in [
            "sk-abcdefghijklmnop",
            "abc.def.ghi",
            "hunter2",
            "topsecret",
            "AKIAABCDEFGHIJKLMNOP",
            "AAAA",
        ] {
            assert!(!clean.contains(leak), "{leak} leaked: {clean}");
        }
        assert!(count >= 6, "expected several redactions, got {count}");
        assert_eq!(redact("ls -la /var/log").0, "ls -la /var/log");
    }

    #[test]
    fn context_respects_scope_and_the_output_gate() {
        let screen = vec![
            "$ cat .env".to_string(),
            "TOKEN=abc123secret".to_string(),
            "$ ls".to_string(),
            "a b".to_string(),
        ];
        let input = ContextInput {
            selection: Some("sel"),
            last_command: Some("ls"),
            screen: &screen,
        };
        let opts = |scope, send_output| ContextOpts {
            scope,
            lines: 3,
            send_output,
            redact: true,
        };
        assert_eq!(build_context(&input, &opts(Scope::None, true)).0, "");
        assert_eq!(build_context(&input, &opts(Scope::Command, true)).0, "ls");
        assert_eq!(
            build_context(&input, &opts(Scope::Screen, false)).0,
            "ls",
            "screen output must be withheld when send_output is off"
        );
        let (context, count) = build_context(&input, &opts(Scope::Screen, true));
        assert_eq!(context.lines().count(), 3);
        assert!(!context.contains("abc123secret") && count == 1, "{context}");
        assert_eq!(
            build_context(&input, &opts(Scope::Selection, true)).0,
            "sel"
        );
        // A request built from the same data agrees with the borrowed form.
        let request = ContextRequest {
            selection: Some("sel".into()),
            last_command: Some("ls".into()),
            screen: screen.clone(),
            scope: "screen".into(),
            lines: 3,
            send_output: true,
            redact: true,
        };
        assert_eq!(request.build(), (context, count));
    }

    #[test]
    fn extracts_a_command_from_json_fence_or_plain_line() {
        assert_eq!(
            extract_command("{\"command\":\"df -h\",\"explanation\":\"disk\"}"),
            ("df -h".into(), "disk".into())
        );
        assert_eq!(extract_command("Sure:\n```bash\nls -la\n```").0, "ls -la");
        assert_eq!(extract_command("`uptime`").0, "uptime");
    }

    #[test]
    fn salvages_a_command_from_truncated_json() {
        // The exact shape a reasoning model produced: budget ran out mid-string.
        let reply =
            "\n\n{\"command\": \"find . -maxdepth 1 -type f -printf '%s %p\\\\n' | sort -rn";
        assert_eq!(
            extract_command(reply).0,
            "find . -maxdepth 1 -type f -printf '%s %p\\n' | sort -rn"
        );

        // Escape sequences still decode when the object is cut short.
        assert_eq!(
            extract_command("{\"command\": \"printf \\\"a\\\\tb\\\"").0,
            "printf \"a\\tb\""
        );

        // A trailing continuation backslash never survives into the draft.
        assert_eq!(extract_command("{\"command\": \"make \\").0, "make");

        // An explanation that was emitted first is still picked up.
        let with_explanation = "{\"explanation\": \"list files\", \"command\": \"ls\"";
        assert_eq!(
            extract_command(with_explanation),
            ("ls".into(), "list files".into())
        );

        // Not JSON: the fenced and plain-line paths stay in charge.
        assert_eq!(
            extract_command("run `df -h` please").0,
            "run `df -h` please"
        );
    }

    #[test]
    fn provider_defaults_and_refs() {
        let provider = Provider {
            id: "p1".into(),
            ..Default::default()
        };
        assert_eq!(provider.kind, Kind::OpenAi);
        assert_eq!(provider.secret_ref(), "ai:p1");
        assert_eq!(provider.command_model(), provider.model.as_str());
        let fast = Provider {
            fast_model: "  mini ".into(),
            ..provider
        };
        assert_eq!(fast.command_model(), "mini");
    }

    fn provider(kind: Kind, base: &str, model: &str) -> Provider {
        Provider {
            id: "t".into(),
            kind,
            base_url: base.into(),
            model: model.into(),
            ..Default::default()
        }
    }

    #[test]
    fn normalises_every_base_url_shape() {
        let openai = |base: &str| chat_endpoint(&provider(Kind::OpenAi, base, "m")).unwrap();
        // Bare versioned bases get exactly one suffix.
        assert_eq!(
            openai("https://api.deepseek.com/v1"),
            "https://api.deepseek.com/v1/chat/completions"
        );
        assert_eq!(
            openai("https://ark.cn-beijing.volces.com/api/v3"),
            "https://ark.cn-beijing.volces.com/api/v3/chat/completions"
        );
        assert_eq!(
            openai("https://qianfan.baidubce.com/v2/"),
            "https://qianfan.baidubce.com/v2/chat/completions"
        );
        // A URL that is already complete must not be suffixed twice.
        assert_eq!(
            openai("https://api.moonshot.cn/v1/chat/completions"),
            "https://api.moonshot.cn/v1/chat/completions"
        );
        // Anthropic keeps its own path.
        assert_eq!(
            chat_endpoint(&provider(
                Kind::Anthropic,
                "https://api.anthropic.com/v1",
                "claude"
            ))
            .unwrap(),
            "https://api.anthropic.com/v1/messages"
        );
        // Azure: `/openai/v1` style and the classic deployment path.
        assert_eq!(
            chat_endpoint(&provider(
                Kind::Azure,
                "https://res.openai.azure.com/openai/v1",
                "gpt4o"
            ))
            .unwrap(),
            "https://res.openai.azure.com/openai/v1/chat/completions?api-version=2024-10-21"
        );
        assert_eq!(
            chat_endpoint(&provider(
                Kind::Azure,
                "https://res.openai.azure.com",
                "gpt4o"
            ))
            .unwrap(),
            "https://res.openai.azure.com/openai/deployments/gpt4o/chat/completions?api-version=2024-10-21"
        );
        let mut versioned = provider(Kind::Azure, "https://res.openai.azure.com", "d");
        versioned.api_version = "2025-01-01-preview".into();
        assert!(chat_endpoint(&versioned)
            .unwrap()
            .ends_with("api-version=2025-01-01-preview"));
        // Bad bases fail loudly instead of producing a relative URL.
        assert!(chat_endpoint(&provider(Kind::OpenAi, "  ", "m")).is_err());
        assert!(chat_endpoint(&provider(Kind::OpenAi, "api.openai.com/v1", "m")).is_err());
        // Models endpoints follow the same normalisation.
        assert_eq!(
            models_endpoint(&provider(Kind::OpenAi, "https://api.deepseek.com/v1", "m")).unwrap(),
            "https://api.deepseek.com/v1/models"
        );
        assert!(
            models_endpoint(&provider(Kind::Azure, "https://res.openai.azure.com", "d"))
                .unwrap()
                .contains("/openai/models?api-version=")
        );
    }

    #[test]
    fn builds_each_protocol_body_differently() {
        let messages = vec![("user".to_string(), "hi".to_string())];
        let openai = request_body(
            &provider(Kind::OpenAi, "https://x/v1", "m"),
            "sys",
            &messages,
        );
        assert_eq!(openai["messages"][0]["role"], "system");
        assert_eq!(openai["max_tokens"], serde_json::json!(1024));
        assert!(openai["temperature"].is_number());

        let anthropic = request_body(
            &provider(Kind::Anthropic, "https://x/v1", "claude"),
            "sys",
            &messages,
        );
        assert_eq!(anthropic["system"], "sys");
        assert_eq!(anthropic["messages"][0]["role"], "user");
        assert!(anthropic["temperature"].is_number());
        // A reasoning hint is never sent to Anthropic.
        assert!(anthropic.get("reasoning_effort").is_none());

        // Azure reuses the OpenAI body shape.
        let azure = request_body(&provider(Kind::Azure, "https://x", "d"), "sys", &messages);
        assert_eq!(azure["messages"][0]["role"], "system");

        // Reasoning models: no sampling block, different cap field name.
        let mut strict = provider(Kind::OpenAi, "https://x/v1", "o3");
        strict.send_sampling = false;
        strict.max_tokens_field = MaxTokensField::MaxCompletionTokens;
        strict.top_p = Some(0.5);
        let body = request_body(&strict, "sys", &messages);
        assert!(body.get("temperature").is_none());
        assert!(body.get("top_p").is_none());
        assert_eq!(body["max_completion_tokens"], serde_json::json!(1024));
        assert!(body.get("max_tokens").is_none());

        // Vendors without a `system` role: the prompt folds into the first turn.
        let mut folded = provider(Kind::OpenAi, "https://x/v1", "m");
        folded.fold_system = true;
        let body = request_body(&folded, "sys", &messages);
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
        assert_eq!(body["messages"][0]["content"], "sys\n\nhi");

        // Opt-in extras only appear when configured.
        let mut rich = provider(Kind::OpenAi, "https://x/v1", "m");
        rich.stop = vec!["###".into()];
        rich.reasoning = Reasoning::Medium;
        let body = request_body(&rich, "sys", &messages);
        assert_eq!(body["stop"], serde_json::json!(["###"]));
        assert_eq!(body["reasoning_effort"], "medium");
    }

    #[test]
    fn clamps_parameters_to_vendor_ranges() {
        let mut raw = provider(Kind::Anthropic, "https://x/v1/", " m ");
        raw.temperature = 1.8;
        raw.max_tokens = 4;
        raw.timeout_secs = 9_999;
        raw.top_p = Some(3.0);
        raw.stop = vec![" ".into(), "END".into()];
        raw.extra_headers = vec![HeaderPair {
            name: "  ".into(),
            value: "x".into(),
        }];
        let clean = raw.sanitize();
        assert_eq!(clean.base_url, "https://x/v1");
        assert_eq!(clean.model, "m");
        assert_eq!(clean.temperature, 1.0);
        assert_eq!(clean.max_tokens, 16);
        assert_eq!(clean.timeout_secs, 600);
        assert_eq!(clean.top_p, Some(1.0));
        assert_eq!(clean.stop, vec!["END".to_string()]);
        assert!(clean.extra_headers.is_empty());

        // OpenAI allows a wider temperature range.
        let mut wide = Provider {
            temperature: 1.8,
            ..Default::default()
        };
        wide = wide.sanitize();
        assert_eq!(wide.temperature, 1.8);
    }

    #[test]
    fn downgrades_a_rejected_parameter_once() {
        let base = provider(Kind::OpenAi, "https://x/v1", "o3");
        // Wrong cap field name.
        let (next, note) = adjustment(
            &base,
            400,
            r#"{"error":"Unsupported parameter: 'max_tokens' is not supported with this model. Use 'max_completion_tokens' instead."}"#,
        )
        .unwrap();
        assert_eq!(note, "ai.noteMaxTokens");
        assert_eq!(next.max_tokens_field, MaxTokensField::MaxCompletionTokens);
        // Already corrected → nothing left to try.
        assert!(adjustment(&next, 400, "max_completion_tokens").is_none());

        // Sampling parameters rejected by a reasoning model.
        let (next, note) = adjustment(
            &base,
            400,
            r#"{"error":{"message":"Unsupported value: 'temperature' does not support 0.2 with this model"}}"#,
        )
        .unwrap();
        assert_eq!(note, "ai.noteSampling");
        assert!(!next.send_sampling);

        // `system` role not understood.
        let (next, note) = adjustment(&base, 422, "invalid role: system is not supported").unwrap();
        assert_eq!(note, "ai.noteSystem");
        assert!(next.fold_system);

        // Unrelated failures are surfaced as-is.
        assert!(adjustment(&base, 400, "invalid api key").is_none());
        assert!(adjustment(&base, 401, "max_completion_tokens").is_none());
        assert!(adjustment(&base, 500, "temperature").is_none());
    }

    /// One-shot HTTP server: returns (base_url, join handle yielding the raw request).
    fn mock(
        status: u16,
        content_type: &'static str,
        body: &'static str,
    ) -> (String, std::thread::JoinHandle<String>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut raw = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let read = socket.read(&mut chunk).unwrap();
                if read == 0 {
                    break;
                }
                raw.extend_from_slice(&chunk[..read]);
                let text = String::from_utf8_lossy(&raw);
                if let Some(head_end) = text.find("\r\n\r\n") {
                    let length = text
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|value| value.trim().parse::<usize>().unwrap_or(0))
                        })
                        .unwrap_or(0);
                    if raw.len() >= head_end + 4 + length {
                        break;
                    }
                }
            }
            let response = format!(
                "HTTP/1.1 {status} X\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(response.as_bytes());
            String::from_utf8_lossy(&raw).to_string()
        });
        (format!("http://{addr}/v1"), handle)
    }

    /// Multi-shot mock: one canned reply per accepted connection, returning
    /// every raw request so a retry can be inspected.
    fn mock_many(
        replies: Vec<(u16, &'static str, String)>,
    ) -> (String, std::thread::JoinHandle<Vec<String>>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for (status, content_type, body) in replies {
                let (mut socket, _) = listener.accept().unwrap();
                let mut raw = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    let read = socket.read(&mut chunk).unwrap();
                    if read == 0 {
                        break;
                    }
                    raw.extend_from_slice(&chunk[..read]);
                    let text = String::from_utf8_lossy(&raw);
                    if let Some(head_end) = text.find("\r\n\r\n") {
                        let length = text
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|value| value.trim().parse::<usize>().unwrap_or(0))
                            })
                            .unwrap_or(0);
                        if raw.len() >= head_end + 4 + length {
                            break;
                        }
                    }
                }
                seen.push(String::from_utf8_lossy(&raw).to_string());
                let response = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes());
                let _ = socket.flush();
            }
            seen
        });
        (format!("http://{addr}/v1"), handle)
    }

    #[tokio::test]
    async fn retries_once_after_a_field_rejection() {
        let (base, server) = mock_many(vec![
            (
                400,
                "application/json",
                r#"{"error":{"message":"Unsupported parameter: 'max_tokens' is not supported with this model. Use 'max_completion_tokens' instead."}}"#
                    .to_string(),
            ),
            (
                200,
                "application/json",
                r#"{"model":"mock-1","choices":[{"message":{"content":"ok"}}]}"#.to_string(),
            ),
        ]);
        let provider = Provider {
            id: "t".into(),
            base_url: base,
            model: "o3".into(),
            ..Default::default()
        };
        let reply = chat(&provider, "sk-TESTKEY-9999", "sys", "hi", None)
            .await
            .unwrap();
        assert_eq!(reply.text, "ok");
        assert_eq!(reply.notes, vec!["ai.noteMaxTokens".to_string()]);

        let seen = server.join().unwrap();
        assert_eq!(seen.len(), 2);
        assert!(seen[0].contains("\"max_tokens\""));
        // The retry swaps the field and keeps everything else.
        assert!(seen[1].contains("\"max_completion_tokens\""));
        assert!(!seen[1].contains("\"max_tokens\""));
        assert!(seen[1].contains("\"temperature\""));
    }

    #[tokio::test]
    async fn lists_models_and_survives_an_empty_catalogue() {
        let (base, server) = mock_many(vec![(
            200,
            "application/json",
            r#"{"data":[{"id":"b-model"},{"id":"a-model"},{"id":"a-model"}]}"#.to_string(),
        )]);
        let provider = Provider {
            id: "t".into(),
            base_url: base,
            ..Default::default()
        };
        let list = models(&provider, "sk-TESTKEY-9999").await.unwrap();
        assert_eq!(list, vec!["a-model".to_string(), "b-model".to_string()]);
        let seen = server.join().unwrap();
        assert!(seen[0].contains("GET /v1/models"));

        let (base, server) = mock_many(vec![(
            200,
            "application/json",
            r#"{"data":[]}"#.to_string(),
        )]);
        let provider = Provider {
            id: "t".into(),
            base_url: base,
            ..Default::default()
        };
        assert!(models(&provider, "sk-K").await.is_err());
        let _ = server.join();
    }

    #[tokio::test]
    async fn openai_key_stays_in_the_header_and_context_is_redacted() {
        let (base, server) = mock(
            200,
            "application/json",
            r#"{"model":"mock-1","choices":[{"message":{"content":"{\"command\":\"df -h\",\"explanation\":\"disk\"}"}}]}"#,
        );
        let provider = Provider {
            id: "t".into(),
            base_url: base,
            model: "m".into(),
            ..Default::default()
        };
        let (context, _) = redact("export API_KEY=sk-livesecretvalue123");
        let reply = chat(
            &provider,
            "sk-TESTKEY-9999",
            COMMAND_SYSTEM_PROMPT,
            &build_user_prompt("disk usage", "bash", "local", &context),
            None,
        )
        .await
        .unwrap();
        assert_eq!(extract_command(&reply.text).0, "df -h");
        let request = server.join().unwrap();
        let (head, body) = request.split_once("\r\n\r\n").unwrap();
        assert!(head
            .to_ascii_lowercase()
            .contains("authorization: bearer sk-testkey-9999"));
        assert!(!body.contains("sk-TESTKEY-9999"), "key leaked into body");
        assert!(
            !body.contains("sk-livesecretvalue123"),
            "unredacted context was sent"
        );
    }

    #[tokio::test]
    async fn errors_never_echo_the_key() {
        let (base, server) = mock(
            401,
            "application/json",
            r#"{"error":"invalid key sk-TESTKEY-9999"}"#,
        );
        let provider = Provider {
            id: "t".into(),
            base_url: base,
            ..Default::default()
        };
        let error = test_connection(&provider, "sk-TESTKEY-9999")
            .await
            .unwrap_err()
            .to_string();
        server.join().unwrap();
        assert!(
            error.contains("401") && !error.contains("sk-TESTKEY-9999"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn anthropic_uses_the_expected_headers() {
        let (base, server) = mock(
            200,
            "application/json",
            r#"{"model":"claude-x","content":[{"type":"text","text":"ok"}]}"#,
        );
        let provider = Provider {
            id: "a".into(),
            kind: Kind::Anthropic,
            base_url: base,
            ..Default::default()
        };
        let reply = test_connection(&provider, "ak-1234").await.unwrap();
        assert_eq!(reply.text, "ok");
        let request = server.join().unwrap().to_ascii_lowercase();
        assert!(
            request.starts_with("post /v1/messages")
                && request.contains("x-api-key: ak-1234")
                && request.contains("anthropic-version: 2023-06-01")
        );
    }

    #[tokio::test]
    async fn sse_stream_is_parsed_into_fragments() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 4096];
            let _ = socket.read(&mut buffer);
            let body =
                "data: {\"model\":\"m1\",\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n\
data: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\n\
data: [DONE]\n\n";
            let _ = write!(
                socket,
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\n\r\n{body}",
                body.len()
            );
        });
        let provider = Provider {
            id: "t".into(),
            base_url: format!("http://127.0.0.1:{port}/v1"),
            model: "m0".into(),
            timeout_secs: 5,
            ..Default::default()
        };
        let mut parts = Vec::new();
        let reply = chat_stream(
            &provider,
            "",
            "s",
            &[("user".into(), "hi".into())],
            |delta| {
                parts.push(delta.to_string());
                true
            },
        )
        .await
        .unwrap();
        assert_eq!(parts, vec!["Hel", "lo"]);
        assert_eq!(reply.text, "Hello");
        assert_eq!(reply.model, "m1");
    }
}
