//! `ferriterm-cli mcp` — the same control surface, spoken as MCP over stdio.
//!
//! An editor or an agent launches this as a child process and talks JSON-RPC
//! 2.0 to it, one message per line. Every tool call ends up going through the
//! running app exactly as a shell command would: same policy engine, same audit
//! chain, same refusal when an approval is needed.
//!
//! This file is deliberately a translation layer and holds no policy of its
//! own. A second gate here would be a second thing to keep correct, and the
//! first one is already the one the person can see and audit.

use std::process::ExitCode;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

use ft_agent::client::Client;
use ft_agent::protocol::{ExecRequest, HostSummary, SessionSummary};
use ft_agent::Endpoint;

use crate::{load_endpoint, pad, pick_session, reasons, risk_label, Failure};

/// Newest revision this server speaks. Sent when a client asks for one we do
/// not recognise, which is what the specification asks for.
const LATEST: &str = "2025-06-18";
/// Revisions answered in their own dialect. The tools surface is identical in
/// all three; only the envelope around it changed.
const SUPPORTED: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

/// What a connecting agent is told before it calls anything. The `ask` rule is
/// the part that matters: a model that reads it will not retry a refusal, and
/// a model that does not would sit in a loop.
const INSTRUCTIONS: &str = "\
Ferriterm drives the SSH sessions a person already has open in the desktop application; \
this service does not open connections of its own. `list_hosts` and `list_sessions` show \
where commands can run.\n\
\n\
`run_command` is graded by the app's policy engine before anything starts. `allow` runs \
the command and returns what it printed. `deny` refuses. `ask` also refuses — an approval \
belongs to a person at the window, and there is nobody here to give one — so report what \
you wanted to run and why, rather than retrying. Every decision, including the refusals, \
is appended to an audit chain the person can verify afterwards.\n\
\n\
Prefer read-only commands. When a change is needed, say what it will do before it does it, \
and run one step at a time so a failure is easy to place.\n\
\n\
Output comes back as text, capped at 256 KiB per call; a truncated answer says so at the \
end. Pass `host` or `session` only when more than one session is open.";

/// Reads requests until stdin closes, which is how an MCP client ends the
/// conversation. Nothing is ever written to stdout except a reply, because a
/// stray line would corrupt the stream for the whole session.
pub async fn serve(endpoint: Option<String>) -> Result<ExitCode, Failure> {
    let mut input = tokio::io::BufReader::new(tokio::io::stdin()).lines();
    let mut output = tokio::io::stdout();

    loop {
        let line = match input.next_line().await {
            Ok(Some(line)) => line,
            Ok(None) => break,
            Err(error) => {
                return Err(Failure::Other(format!("cannot read stdin: {error}")));
            }
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let reply = match serde_json::from_str::<Value>(line) {
            Ok(message) => answer(message, endpoint.as_deref()).await,
            Err(error) => Some(failure(
                Value::Null,
                -32700,
                format!("this line is not JSON: {error}"),
            )),
        };
        let Some(reply) = reply else {
            continue;
        };

        let mut encoded = serde_json::to_string(&reply).unwrap_or_else(|_| "{}".to_string());
        encoded.push('\n');
        if let Err(error) = output.write_all(encoded.as_bytes()).await {
            return Err(Failure::Other(format!("cannot write a reply: {error}")));
        }
        if let Err(error) = output.flush().await {
            return Err(Failure::Other(format!("cannot write a reply: {error}")));
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// Answers one message. `None` means it was a notification and needs no reply.
async fn answer(message: Value, endpoint: Option<&str>) -> Option<Value> {
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let Some(id) = message.get("id").cloned() else {
        // `notifications/initialized` is the one that always arrives and it
        // asks for nothing; every other notification is equally free to ignore.
        return None;
    };

    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let outcome: Result<Value, (i64, String)> = match method.as_str() {
        "initialize" => Ok(initialize(&params)),
        "notifications/initialized" => Ok(json!({})),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(tools()),
        "tools/call" => call(&params, endpoint).await,
        other => Err((-32601, format!("this server has no method {other}"))),
    };

    Some(match outcome {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => failure(id, code, message),
    })
}

fn failure(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message.into() }
    })
}

fn initialize(params: &Value) -> Value {
    let asked = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or_default();
    // Echo the client's revision when it is one we implement, otherwise offer
    // the newest we do and let the client decide whether to carry on.
    let version = if SUPPORTED.contains(&asked) {
        asked
    } else {
        LATEST
    };

    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": {
            "name": "ferriterm",
            "title": "Ferriterm",
            "version": env!("CARGO_PKG_VERSION")
        },
        "instructions": INSTRUCTIONS
    })
}

// ------------------------------------------------------------------- tools

fn tools() -> Value {
    json!({
        "tools": [
            {
                "name": "list_hosts",
                "title": "List saved hosts",
                "description": "The hosts saved in Ferriterm, each with the session serving it when a connection is open. Call this first: run_command can only go where a session already exists.",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                },
                "outputSchema": {
                    "type": "object",
                    "properties": {
                        "hosts": { "type": "array", "items": { "type": "object" } }
                    },
                    "required": ["hosts"]
                },
                "annotations": { "readOnlyHint": true, "openWorldHint": false }
            },
            {
                "name": "list_sessions",
                "title": "List open sessions",
                "description": "The connections open right now, with the id run_command takes. A host with no session is not reachable until someone connects to it in the app.",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                },
                "outputSchema": {
                    "type": "object",
                    "properties": {
                        "sessions": { "type": "array", "items": { "type": "object" } }
                    },
                    "required": ["sessions"]
                },
                "annotations": { "readOnlyHint": true, "openWorldHint": false }
            },
            {
                "name": "run_command",
                "title": "Run a command in a session",
                "description": "Run one shell command on a host, after the app's policy engine grades it. allow runs it and returns what it printed; deny refuses; ask refuses as well, because an approval belongs to a person at the window and not to this call. When it refuses, tell the person what you wanted to run and why instead of retrying. Every decision is written to an audit chain they can verify. Output is capped at 256 KiB and says so when cut.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The command line, exactly as a shell would take it. Quoting and escaping are yours; there is no surrounding prompt."
                        },
                        "host": {
                            "type": "string",
                            "description": "Host label, address or user@host, as list_hosts prints it. Omit when exactly one session is open."
                        },
                        "session": {
                            "type": "string",
                            "description": "Session id from list_sessions. Wins over host when both are given."
                        },
                        "timeoutSeconds": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 600,
                            "description": "How long the command may run before it is stopped. Defaults to 60."
                        }
                    },
                    "required": ["command"],
                    "additionalProperties": false
                },
                "annotations": {
                    "readOnlyHint": false,
                    "destructiveHint": true,
                    "idempotentHint": false,
                    "openWorldHint": true
                }
            }
        ]
    })
}

/// Dispatches a tool call. Only a name this server does not have is a protocol
/// error: everything that can go wrong while carrying the call out — no app, no
/// session, a refusal — comes back as a tool result with `isError`, because the
/// text is what a model needs in order to change course.
async fn call(params: &Value, endpoint: Option<&str>) -> Result<Value, (i64, String)> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    Ok(match name.as_str() {
        "list_hosts" => list_hosts(endpoint).await,
        "list_sessions" => list_sessions(endpoint).await,
        "run_command" => run_command(&arguments, endpoint).await,
        other => return Err((-32602, format!("this server has no tool {other}"))),
    })
}

async fn list_hosts(endpoint: Option<&str>) -> Value {
    let client = match connect(endpoint) {
        Ok(client) => client,
        Err(message) => return tool_error(message),
    };
    let hosts: Vec<HostSummary> = match client.hosts().await {
        Ok(hosts) => hosts,
        Err(error) => return tool_error(error.message),
    };

    let text = if hosts.is_empty() {
        "No saved hosts. Nothing can run until a host is added in the app.".to_string()
    } else {
        hosts
            .iter()
            .map(|host| {
                let open = match &host.session {
                    Some(session) => format!("  session {session}"),
                    None => "  no session".to_string(),
                };
                format!(
                    "{}{}@{}:{}  {}{}{}",
                    pad(&host.label, 22),
                    host.username,
                    host.host,
                    host.port,
                    host.protocol,
                    if host.production { "  production" } else { "" },
                    open
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    tool_result(text, Some(json!({ "hosts": hosts })))
}

async fn list_sessions(endpoint: Option<&str>) -> Value {
    let client = match connect(endpoint) {
        Ok(client) => client,
        Err(message) => return tool_error(message),
    };
    let sessions: Vec<SessionSummary> = match client.sessions().await {
        Ok(sessions) => sessions,
        Err(error) => return tool_error(error.message),
    };

    let text = if sessions.is_empty() {
        "No open sessions. Someone has to connect in the app before a command can run.".to_string()
    } else {
        sessions
            .iter()
            .map(|session| {
                format!(
                    "{}  {}  {}@{}  {}",
                    session.id, session.label, session.username, session.host, session.kind
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    tool_result(text, Some(json!({ "sessions": sessions })))
}

async fn run_command(arguments: &Value, endpoint: Option<&str>) -> Value {
    let command = arguments
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    if command.is_empty() {
        return tool_error("command is required");
    }

    let wanted_session = arguments
        .get("session")
        .and_then(Value::as_str)
        .map(str::to_string);
    let wanted_host = arguments
        .get("host")
        .and_then(Value::as_str)
        .map(str::to_string);
    let timeout_ms = arguments
        .get("timeoutSeconds")
        .and_then(Value::as_u64)
        .map(|seconds| seconds.min(600).saturating_mul(1000));

    let client = match connect(endpoint) {
        Ok(client) => client,
        Err(message) => return tool_error(message),
    };
    let session = match pick_session(&client, wanted_session, wanted_host).await {
        Ok(session) => session,
        Err(failure) => return tool_error(text_of(failure)),
    };

    let report = match client
        .exec(&ExecRequest {
            session,
            command,
            timeout_ms,
            origin: Some("mcp".to_string()),
        })
        .await
    {
        Ok(report) => report,
        Err(error) => return tool_error(error.message),
    };

    if report.ran {
        let mut text = match report.output.as_deref() {
            Some(output) if !output.trim().is_empty() => output.to_string(),
            _ => "The command ran and printed nothing.".to_string(),
        };
        if !text.ends_with('\n') {
            text.push('\n');
        }
        if report.truncated {
            text.push_str("[output stopped at the 256 KiB cap]\n");
        }
        return tool_result(text, None);
    }

    // Nothing ran. The reason matters more than the refusal: a model that is
    // told which risk or rule stopped it can change course instead of guessing.
    let mut text = format!(
        "Refused. The command did not run.\nrisk: {}\nreasons: {}\nverdict: {} in {} mode",
        risk_label(&report.risk),
        reasons(&report.reasons),
        report.verdict,
        report.mode
    );
    if let Some(rule) = &report.rule {
        text.push_str(&format!("\nrule: {rule}"));
    }
    if report.verdict == "ask" {
        text.push_str(
            "\nThe policy asks before commands like this. Nobody here can approve it, so ask the \
person to run it in the Ferriterm window, or to allow it in the policy.",
        );
    }
    tool_error(text)
}

// ----------------------------------------------------------------- helpers

fn connect(endpoint: Option<&str>) -> Result<Client, String> {
    // Re-read the file on every call. The port and the token are minted anew
    // each launch, so a cached endpoint would turn every later call after a
    // restart into an auth failure that looks like a bug.
    let endpoint: Endpoint = load_endpoint(endpoint).map_err(text_of)?;
    Ok(Client::new(&endpoint))
}

fn tool_result(text: impl Into<String>, structured: Option<Value>) -> Value {
    let mut result = json!({
        "content": [{ "type": "text", "text": text.into() }],
        "isError": false
    });
    if let Some(structured) = structured {
        result["structuredContent"] = structured;
    }
    result
}

/// A call that could not be carried out. The text goes into the model's
/// context, so it says what to do next rather than only what went wrong.
fn tool_error(text: impl Into<String>) -> Value {
    json!({
        "content": [{ "type": "text", "text": text.into() }],
        "isError": true
    })
}

fn text_of(failure: Failure) -> String {
    match failure {
        Failure::Usage(message) | Failure::Other(message) => message,
    }
}

/// Exposed for the tests below, which drive the whole handler without a
/// process or an app.
#[cfg(test)]
mod tests {
    use super::*;

    fn request(method: &str, params: Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params })
    }

    #[tokio::test]
    async fn initialize_answers_in_the_clients_revision() {
        let reply = answer(
            request("initialize", json!({ "protocolVersion": "2025-03-26" })),
            None,
        )
        .await
        .expect("initialize is a request");
        assert_eq!(reply["jsonrpc"], "2.0");
        assert_eq!(reply["id"], 1);
        assert_eq!(reply["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(reply["result"]["serverInfo"]["name"], "ferriterm");
    }

    #[tokio::test]
    async fn initialize_falls_back_to_the_newest_revision() {
        let reply = answer(
            request("initialize", json!({ "protocolVersion": "1999-01-01" })),
            None,
        )
        .await
        .expect("initialize is a request");
        assert_eq!(reply["result"]["protocolVersion"], LATEST);
    }

    #[tokio::test]
    async fn the_initialized_notification_gets_no_reply() {
        let reply = answer(
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
            None,
        )
        .await;
        assert!(reply.is_none());
    }

    #[tokio::test]
    async fn ping_is_answered_with_an_empty_object() {
        let reply = answer(request("ping", Value::Null), None)
            .await
            .expect("ping is a request");
        assert_eq!(reply["result"], json!({}));
    }

    #[tokio::test]
    async fn unknown_methods_are_rejected_by_code() {
        let reply = answer(request("resources/list", Value::Null), None)
            .await
            .expect("a request with an id is answered");
        assert_eq!(reply["error"]["code"], -32601);
    }

    #[tokio::test]
    async fn every_tool_declares_a_schema_and_a_name() {
        let listed = tools();
        let tools = listed["tools"].as_array().expect("tools is an array");
        assert_eq!(tools.len(), 3);
        for tool in tools {
            assert!(tool["name"].is_string());
            assert!(tool["description"].as_str().unwrap_or_default().len() > 40);
            assert_eq!(tool["inputSchema"]["type"], "object");
        }
    }

    #[tokio::test]
    async fn an_unknown_tool_is_a_parameter_error() {
        let reply = answer(
            request(
                "tools/call",
                json!({ "name": "delete_everything", "arguments": {} }),
            ),
            None,
        )
        .await
        .expect("tools/call is a request");
        assert_eq!(reply["error"]["code"], -32602);
    }

    #[tokio::test]
    async fn a_missing_command_says_so_without_touching_the_app() {
        let reply = answer(
            request(
                "tools/call",
                json!({ "name": "run_command", "arguments": { "host": "x" } }),
            ),
            None,
        )
        .await
        .expect("tools/call is a request");
        assert_eq!(reply["result"]["isError"], true);
        assert_eq!(reply["result"]["content"][0]["text"], "command is required");
    }

    #[tokio::test]
    async fn a_tool_without_an_app_reports_a_tool_failure() {
        // No endpoint file is reachable here, so the call is well formed but
        // cannot be carried out: a tool failure the model can read, not a
        // protocol error it cannot act on.
        std::env::set_var("FERRITERM_AGENT", "Z:\\definitely-not-here\\agent.json");
        let reply = answer(
            request(
                "tools/call",
                json!({ "name": "list_hosts", "arguments": {} }),
            ),
            None,
        )
        .await
        .expect("tools/call is a request");
        std::env::remove_var("FERRITERM_AGENT");
        assert_eq!(reply["result"]["isError"], true);
        let text = reply["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_default();
        assert!(text.contains("switched on"), "text was {text}");
    }
}
