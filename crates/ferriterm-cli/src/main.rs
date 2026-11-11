//! `ferriterm-cli` — talk to a running Ferriterm from a shell.
//!
//! Every command goes through the app, never around it: `exec` is graded by the
//! same policy engine and written to the same audit log as the assistant's run
//! button, so a command run from here is indistinguishable from one run there,
//! apart from where the answer came from.
//!
//! Flags are long-form only, and they stop at the first word that is not one,
//! which is what lets `exec ls -la` mean what it looks like.

use std::process::ExitCode;

use ft_agent::client::{Client, ClientError};
use ft_agent::protocol::{
    CheckReport, CheckRequest, ExecReport, ExecRequest, Health, HostSummary, SessionSummary,
};
use ft_agent::{discover_endpoint, Endpoint};

/// The gate said no. Distinct from 1 so a script can tell "refused" from
/// "broken".
const EXIT_REFUSED: u8 = 3;
const EXIT_ERROR: u8 = 1;

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json = args.iter().any(|arg| arg == "--json");

    let outcome = match run(args).await {
        Ok(code) => code,
        Err(Failure::Usage(message)) => {
            eprintln!("ferriterm-cli: {message}");
            eprintln!("run `ferriterm-cli help` for the list of commands");
            ExitCode::from(EXIT_ERROR)
        }
        Err(Failure::Other(message)) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({ "error": message }).to_string()
                );
            } else {
                eprintln!("ferriterm-cli: {message}");
            }
            ExitCode::from(EXIT_ERROR)
        }
    };
    outcome
}

enum Failure {
    Usage(String),
    Other(String),
}

impl From<String> for Failure {
    fn from(message: String) -> Self {
        Failure::Other(message)
    }
}

impl From<ClientError> for Failure {
    fn from(error: ClientError) -> Self {
        Failure::Other(error.message)
    }
}

const HELP: &str = "\
ferriterm-cli — drive a running Ferriterm

Usage
  ferriterm-cli [--json] <command> [options]

Commands
  status                     endpoint file in use, app version, open sessions
  hosts                      saved hosts and whether a session serves them
  sessions                   live connections, with the id other commands take
  check  <command>           grade a command and stop; nothing runs
  exec   <command>           grade a command and run it when the gate allows
  upload <local> <remote>    write a file through the session
  download <remote> <local>  read a file through the session

Options
  --session <id>    act on this session (see `sessions`)
  --host <name>     pick the session by host label, address or user@host
  --timeout <secs>  command timeout for exec, default 60
  --endpoint <path> endpoint file to use instead of the discovered one
  --json            print the raw answer instead of a table
  --                everything after this is the remote command

The command is what follows the flags, so `exec ls -la` needs no quoting.
`exec` exits 3 when the policy refused the command, 0 when it ran.

Environment
  FERRITERM_AGENT  path to an endpoint file, for a second profile
  FERRITERM_DATA_DIR  application data directory, for tests
";

async fn run(args: Vec<String>) -> Result<ExitCode, Failure> {
    let mut json = false;
    let mut endpoint_override: Option<String> = None;
    let mut command: Option<String> = None;
    let mut rest: Vec<String> = Vec::new();

    let mut index = 0;
    while index < args.len() {
        let arg = args[index].clone();
        index += 1;
        match arg.as_str() {
            "--json" => json = true,
            "--endpoint" => {
                endpoint_override = Some(
                    args.get(index)
                        .cloned()
                        .ok_or_else(|| Failure::Usage("--endpoint needs a path".to_string()))?,
                );
                index += 1;
            }
            "-h" | "--help" | "help" => {
                print!("{HELP}");
                return Ok(ExitCode::SUCCESS);
            }
            "-V" | "--version" => {
                println!("ferriterm-cli {}", env!("CARGO_PKG_VERSION"));
                return Ok(ExitCode::SUCCESS);
            }
            other if other.starts_with('-') => {
                return Err(Failure::Usage(format!("unknown option {other}")));
            }
            other => {
                command = Some(other.to_string());
                rest = args[index..].to_vec();
                break;
            }
        }
    }

    let Some(command) = command else {
        print!("{HELP}");
        return Ok(ExitCode::from(EXIT_ERROR));
    };

    let endpoint = load_endpoint(endpoint_override.as_deref())?;
    let client = Client::new(&endpoint);

    match command.as_str() {
        "status" => status(&client, &endpoint, json).await,
        "hosts" => hosts(&client, json).await,
        "sessions" => sessions(&client, json).await,
        "check" => check(&client, rest, json).await,
        "exec" => exec(&client, rest, json).await,
        "upload" => upload(&client, rest, json).await,
        "download" => download(&client, rest, json).await,
        other => Err(Failure::Usage(format!("unknown command {other}"))),
    }
}

fn load_endpoint(explicit: Option<&str>) -> Result<Endpoint, Failure> {
    match explicit {
        Some(path) => Endpoint::load(std::path::Path::new(path)).map_err(Failure::Other),
        None => {
            let (path, endpoint) = discover_endpoint().map_err(|error| {
                Failure::Other(format!(
                    "{error}\nIs Ferriterm running? The endpoint file is written when it starts."
                ))
            })?;
            let _ = path;
            Ok(endpoint)
        }
    }
}

// --------------------------------------------------------------- arguments

/// Flags shared by the commands that need a session.
#[derive(Default)]
struct Target {
    session: Option<String>,
    host: Option<String>,
    timeout_ms: Option<u64>,
    command: Vec<String>,
}

/// Splits `rest` into flags and the remote command. Long options only, and
/// they stop at the first other word, so `-la` stays part of the command.
fn parse_target(rest: Vec<String>) -> Result<Target, Failure> {
    let mut target = Target::default();
    let mut flags_open = true;
    let mut index = 0;

    while index < rest.len() {
        let arg = rest[index].clone();
        index += 1;
        if !flags_open {
            target.command.push(arg);
            continue;
        }
        if arg == "--" {
            flags_open = false;
            continue;
        }
        if !arg.starts_with("--") {
            flags_open = false;
            target.command.push(arg);
            continue;
        }
        let (name, inline) = match arg.split_once('=') {
            Some((name, value)) => (name.to_string(), Some(value.to_string())),
            None => (arg.clone(), None),
        };
        let mut value = |flag: &str| -> Result<String, Failure> {
            match &inline {
                Some(value) => Ok(value.clone()),
                None => {
                    let value = rest
                        .get(index)
                        .cloned()
                        .ok_or_else(|| Failure::Usage(format!("{flag} needs a value")))?;
                    index += 1;
                    Ok(value)
                }
            }
        };
        match name.as_str() {
            "--session" => target.session = Some(value("--session")?),
            "--host" => target.host = Some(value("--host")?),
            "--timeout" => {
                let raw = value("--timeout")?;
                let seconds = raw
                    .parse::<u64>()
                    .map_err(|_| Failure::Usage(format!("--timeout wants seconds, got {raw}")))?;
                target.timeout_ms = Some(seconds.saturating_mul(1000));
            }
            "--json" => {}
            "--endpoint" => {
                let _ = value("--endpoint")?;
            }
            other => return Err(Failure::Usage(format!("unknown option {other}"))),
        }
    }
    Ok(target)
}

// ---------------------------------------------------------------- commands

async fn status(client: &Client, endpoint: &Endpoint, json: bool) -> Result<ExitCode, Failure> {
    let health: Health = client.health().await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&health).unwrap_or_default());
        return Ok(ExitCode::SUCCESS);
    }
    let path = discover_endpoint()
        .map(|(path, _)| path.display().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    println!("endpoint  {path}");
    println!("app       {} (agent {})", health.version, endpoint.version);
    println!("port      {} on loopback, token required", endpoint.port);
    println!("pid       {}", health.pid);
    println!("sessions  {}", health.sessions);
    if !health.ready {
        println!("state     still starting");
    }
    Ok(ExitCode::SUCCESS)
}

async fn hosts(client: &Client, json: bool) -> Result<ExitCode, Failure> {
    let hosts: Vec<HostSummary> = client.hosts().await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&hosts).unwrap_or_default());
        return Ok(ExitCode::SUCCESS);
    }
    if hosts.is_empty() {
        println!("no saved hosts");
        return Ok(ExitCode::SUCCESS);
    }
    for host in &hosts {
        let open = match &host.session {
            Some(session) => format!("  session {session}"),
            None => String::new(),
        };
        let production = if host.production { "  production" } else { "" };
        println!(
            "{}{}@{}:{:<5} {}{}{}",
            pad(&host.label, 22),
            host.username,
            host.host,
            host.port,
            host.protocol,
            production,
            open
        );
    }
    Ok(ExitCode::SUCCESS)
}

async fn sessions(client: &Client, json: bool) -> Result<ExitCode, Failure> {
    let sessions: Vec<SessionSummary> = client.sessions().await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&sessions).unwrap_or_default());
        return Ok(ExitCode::SUCCESS);
    }
    if sessions.is_empty() {
        println!("no open sessions");
        return Ok(ExitCode::SUCCESS);
    }
    for session in &sessions {
        println!(
            "{:<38} {:<20} {}@{}  {}",
            session.id, session.label, session.username, session.host, session.kind
        );
    }
    Ok(ExitCode::SUCCESS)
}

async fn check(client: &Client, rest: Vec<String>, json: bool) -> Result<ExitCode, Failure> {
    let target = parse_target(rest)?;
    let command = join(&target.command)?;
    let report: CheckReport = client
        .check(&CheckRequest {
            command,
            session: target.session,
            host: target.host,
        })
        .await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap_or_default());
    } else {
        println!("command  {}", report.command);
        println!("host     {}", if report.host.is_empty() { "-" } else { &report.host });
        println!("risk     {} ({})", risk_label(&report.risk), reasons(&report.reasons));
        println!("verdict  {}", report.verdict);
        println!("mode     {}", report.mode);
        if let Some(rule) = &report.rule {
            println!("rule     {rule}");
        }
    }
    if report.verdict == "deny" {
        return Ok(ExitCode::from(EXIT_REFUSED));
    }
    Ok(ExitCode::SUCCESS)
}

async fn exec(client: &Client, rest: Vec<String>, json: bool) -> Result<ExitCode, Failure> {
    let target = parse_target(rest)?;
    let command = join(&target.command)?;
    let session = pick_session(client, target.session, target.host).await?;
    let report: ExecReport = client
        .exec(&ExecRequest {
            session,
            command,
            timeout_ms: target.timeout_ms,
        })
        .await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap_or_default());
    } else if report.ran {
        if let Some(output) = &report.output {
            print!("{output}");
            if !output.ends_with('\n') {
                println!();
            }
        }
        if report.truncated {
            eprintln!("output was cut at the service's cap");
        }
    } else {
        // Nothing ran, so nothing goes to stdout: a caller piping this file
        // must not mistake a refusal for output.
        eprintln!(
            "refused: {} ({})",
            risk_label(&report.risk),
            reasons(&report.reasons)
        );
        eprintln!("verdict {} in {} mode", report.verdict, report.mode);
        if report.verdict == "ask" {
            eprintln!("an approval needs a person: approve it in the app, or set the policy to automatic");
        }
    }
    if report.ran {
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::from(EXIT_REFUSED))
    }
}

async fn upload(client: &Client, rest: Vec<String>, json: bool) -> Result<ExitCode, Failure> {
    let target = parse_target(rest)?;
    if target.command.len() != 2 {
        return Err(Failure::Usage(
            "upload needs a local file and a remote path".to_string(),
        ));
    }
    let local = std::path::Path::new(&target.command[0]);
    let remote = target.command[1].clone();
    let content = std::fs::read(local)
        .map_err(|error| Failure::Other(format!("cannot read {}: {error}", local.display())))?;
    let session = pick_session(client, target.session, target.host).await?;
    let report = client.upload(&session, &remote, &content).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap_or_default());
    } else {
        println!("wrote {} bytes to {}", report.written, report.path);
    }
    Ok(ExitCode::SUCCESS)
}

async fn download(client: &Client, rest: Vec<String>, json: bool) -> Result<ExitCode, Failure> {
    let target = parse_target(rest)?;
    if target.command.len() != 2 {
        return Err(Failure::Usage(
            "download needs a remote path and a local file".to_string(),
        ));
    }
    let remote = target.command[0].clone();
    let local = std::path::PathBuf::from(&target.command[1]);
    let session = pick_session(client, target.session, target.host).await?;
    let blob = client.download(&session, &remote).await?;
    let bytes = ft_agent::decode_base64(&blob.content_base64)?;
    std::fs::write(&local, &bytes)
        .map_err(|error| Failure::Other(format!("cannot write {}: {error}", local.display())))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&blob).unwrap_or_default());
    } else {
        println!("read {} bytes from {}", bytes.len(), blob.path);
    }
    Ok(ExitCode::SUCCESS)
}

// ----------------------------------------------------------------- helpers

fn join(parts: &[String]) -> Result<String, Failure> {
    if parts.is_empty() {
        return Err(Failure::Usage("a command is required".to_string()));
    }
    Ok(parts.join(" "))
}

/// Picks the session a command runs in. An explicit id always wins; a host is
/// matched against label, address and `user@host`; with neither, a single open
/// session is the obvious answer and more than one is an error rather than a
/// guess.
async fn pick_session(
    client: &Client,
    session: Option<String>,
    host: Option<String>,
) -> Result<String, Failure> {
    if let Some(session) = session {
        return Ok(session);
    }
    let sessions = client.sessions().await?;
    if let Some(wanted) = host {
        let wanted_lower = wanted.to_lowercase();
        let matches: Vec<&SessionSummary> = sessions
            .iter()
            .filter(|entry| {
                entry.label.to_lowercase() == wanted_lower
                    || entry.host.to_lowercase() == wanted_lower
                    || format!("{}@{}", entry.username, entry.host).to_lowercase() == wanted_lower
            })
            .collect();
        return match matches.len() {
            1 => Ok(matches[0].id.clone()),
            0 => Err(Failure::Other(format!(
                "no open session matches {wanted}; try `ferriterm-cli sessions`"
            ))),
            _ => Err(Failure::Other(format!(
                "{wanted} matches {} sessions: {}",
                matches.len(),
                matches
                    .iter()
                    .map(|entry| entry.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))),
        };
    }
    match sessions.len() {
        1 => Ok(sessions[0].id.clone()),
        0 => Err(Failure::Other("no open session".to_string())),
        count => Err(Failure::Other(format!(
            "{count} sessions are open; pass --host or --session"
        ))),
    }
}

/// Pads to a column width counted in character cells, so a Chinese host name
/// and an ASCII one still line up.
fn pad(text: &str, width: usize) -> String {
    let used: usize = text
        .chars()
        .map(|ch| if ch as u32 > 0x2E80 { 2 } else { 1 })
        .sum();
    let mut out = text.to_string();
    for _ in used..width {
        out.push(' ');
    }
    out
}

fn risk_label(risk: &str) -> String {
    match risk {
        "r0" => "R0 read-only".to_string(),
        "r1" => "R1 writes".to_string(),
        "r2" => "R2 modifies".to_string(),
        "r3" => "R3 privileged".to_string(),
        "r4" => "R4 destructive".to_string(),
        other => other.to_uppercase(),
    }
}

/// Turns the engine's camelCase reason names into words: `recursiveDelete`
/// reads better as "recursive delete" in a terminal.
fn reasons(list: &[String]) -> String {
    if list.is_empty() {
        return "no flags".to_string();
    }
    list.iter()
        .map(|reason| {
            let mut words = String::new();
            for (index, ch) in reason.char_indices() {
                if ch.is_uppercase() && index > 0 {
                    words.push(' ');
                }
                words.push(ch.to_ascii_lowercase());
            }
            words
        })
        .collect::<Vec<_>>()
        .join(", ")
}
