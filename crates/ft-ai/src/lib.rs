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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    OpenAi,
    Anthropic,
}

impl Kind {
    pub fn id(self) -> &'static str {
        match self {
            Kind::OpenAi => "openai",
            Kind::Anthropic => "anthropic",
        }
    }

    pub fn from_id(value: &str) -> Kind {
        if value.eq_ignore_ascii_case("anthropic") {
            Kind::Anthropic
        } else {
            Kind::OpenAi
        }
    }
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

pub const CHAT_SYSTEM_PROMPT: &str = "You are the Ferriterm terminal assistant. Reply in plain sentences without bold or headings. \
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
}

fn client(provider: &Provider) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(provider.timeout_secs.clamp(1, 600)))
        .build()
        .map_err(|err| AiError::Request(err.to_string()))
}

fn endpoint(provider: &Provider, path: &str) -> String {
    let base = provider.base_url.trim_end_matches('/');
    format!("{base}/{path}")
}

/// Keep provider error bodies short and never echo the key back.
fn scrub(body: &str, key: &str) -> String {
    let mut text: String = body.chars().take(400).collect();
    if key.len() >= 4 {
        text = text.replace(key, "[key]");
    }
    text
}

fn request_body(provider: &Provider, system: &str, messages: &[(String, String)]) -> serde_json::Value {
    let wire: Vec<serde_json::Value> = messages
        .iter()
        .map(|(role, content)| serde_json::json!({ "role": role, "content": content }))
        .collect();
    match provider.kind {
        Kind::OpenAi => {
            let mut all = vec![serde_json::json!({ "role": "system", "content": system })];
            all.extend(wire);
            serde_json::json!({
                "model": provider.model,
                "max_tokens": provider.max_tokens,
                "temperature": provider.temperature,
                "messages": all,
            })
        }
        Kind::Anthropic => serde_json::json!({
            "model": provider.model,
            "max_tokens": provider.max_tokens,
            "temperature": provider.temperature,
            "system": system,
            "messages": wire,
        }),
    }
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
    }
}

fn send_call(
    provider: &Provider,
    key: &str,
    body: &serde_json::Value,
) -> Result<(reqwest::RequestBuilder, reqwest::Url)> {
    let path = match provider.kind {
        Kind::OpenAi => "chat/completions",
        Kind::Anthropic => "messages",
    };
    let url = endpoint(provider, path);
    let parsed = reqwest::Url::parse(&url).map_err(|err| AiError::Request(err.to_string()))?;
    let builder = client(provider)?
        .post(parsed.clone())
        .header("content-type", "application/json")
        .json(body);
    Ok((apply_auth(builder, provider, key), parsed))
}

fn reply_text(provider: &Provider, value: &serde_json::Value) -> Option<String> {
    let pointer = match provider.kind {
        Kind::OpenAi => "/choices/0/message/content",
        Kind::Anthropic => "/content/0/text",
    };
    value
        .pointer(pointer)
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

/// One non-streaming round trip.
pub async fn chat(
    provider: &Provider,
    key: &str,
    system: &str,
    user: &str,
    max_tokens: Option<u32>,
) -> Result<Reply> {
    let messages = vec![("user".to_string(), user.to_string())];
    let mut body = request_body(provider, system, &messages);
    if let Some(limit) = max_tokens {
        body["max_tokens"] = serde_json::json!(limit);
    }
    let (request, _) = send_call(provider, key, &body)?;
    let started = std::time::Instant::now();
    let response = request
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
    let text = reply_text(provider, &value)
        .ok_or_else(|| AiError::Response(scrub(&value.to_string(), key)))?;
    Ok(Reply {
        text,
        latency_ms: started.elapsed().as_millis() as u64,
        model: response_model(provider, &value),
    })
}

/// A minimal real request used by "test connection".
pub async fn test_connection(provider: &Provider, key: &str) -> Result<Reply> {
    chat(provider, key, "ping", "ping", Some(1)).await
}

fn delta_text<'a>(provider: &Provider, value: &'a serde_json::Value) -> Option<&'a str> {
    match provider.kind {
        Kind::OpenAi => value
            .pointer("/choices/0/delta/content")
            .and_then(|node| node.as_str()),
        Kind::Anthropic => value
            .pointer("/delta/text")
            .or_else(|| value.pointer("/content_block/delta/text"))
            .and_then(|node| node.as_str()),
    }
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
    let mut body = request_body(provider, system, messages);
    body["stream"] = serde_json::json!(true);
    let (request, _) = send_call(provider, key, &body)?;
    let started = std::time::Instant::now();
    let response = request
        .header("accept", "text/event-stream")
        .send()
        .await
        .map_err(|err| AiError::Request(scrub(&err.to_string(), key)))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(AiError::Http(status.as_u16(), scrub(&body, key)));
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
        let text = reply_text(provider, &value)
            .ok_or_else(|| AiError::Response(scrub(&value.to_string(), key)))?;
        on_delta(&text);
        return Ok(Reply {
            text,
            latency_ms: started.elapsed().as_millis() as u64,
            model: response_model(provider, &value),
        });
    }

    let mut stream = response.bytes_stream();
    let mut buffer: Vec<u8> = Vec::new();
    let mut text = String::new();
    let mut model = provider.model.clone();
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
            if let Some(delta) = delta_text(provider, &value) {
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
    Ok(Reply {
        text,
        latency_ms: started.elapsed().as_millis() as u64,
        model,
    })
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
        assert_eq!(build_context(&input, &opts(Scope::Selection, true)).0, "sel");
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
        let reply = "\n\n{\"command\": \"find . -maxdepth 1 -type f -printf '%s %p\\\\n' | sort -rn";
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
        assert_eq!(extract_command("run `df -h` please").0, "run `df -h` please");
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
        assert!(error.contains("401") && !error.contains("sk-TESTKEY-9999"), "{error}");
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
            let body = "data: {\"model\":\"m1\",\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n\
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
