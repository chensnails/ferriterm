# Local control service and `ferriterm-cli`

Ferriterm listens on loopback so a script, an agent, or the bundled CLI can
drive it without a person clicking. This document is the contract: what the
service exposes, how it authenticates, and what it deliberately refuses to do.

## Why it exists

The assistant inside the app can propose a command, but so can an agent outside
it. Rather than give each of them its own path into the machine, they all go
through one service that applies the same policy engine and writes to the same
audit chain as the assistant's run button. The CLI and the MCP server below are
two clients of that service, not two implementations of it.

## Reaching it

| | |
| --- | --- |
| Address | `127.0.0.1` only, never a routable interface |
| Port | chosen by the kernel at each launch, so it cannot be aimed at from outside |
| Token | 32 random bytes from the OS, minted at each launch |
| Endpoint file | `<app data>/agent.json`, mode `0600` on Unix |

The endpoint file is the only discovery mechanism:

```json
{
  "version": "0.1.0",
  "port": 60303,
  "token": "…64 hex characters…",
  "pid": 7156
}
```

It is written during startup, before the window is usable, and removed on a
clean exit. A file left behind by a crash still points at a dead port, and its
token died with the process, so a stale file fails closed — the CLI says so and
suggests checking `status`.

Set `FERRITERM_AGENT` to point a client at a different endpoint file, and
`FERRITERM_DATA_DIR` to relocate the data directory (useful for tests).

## Routes

Every request needs `Authorization: Bearer <token>`; a wrong or missing token
is `401` with `kind: "auth"` and nothing else is revealed. Bodies are JSON,
answers are JSON, and one request uses one connection.

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/v1/health` | version, pid, open session count |
| `GET` | `/v1/hosts` | saved hosts, plus the session serving each one |
| `GET` | `/v1/sessions` | live connections and the ids other calls take |
| `POST` | `/v1/check` | grade a command; nothing runs |
| `POST` | `/v1/exec` | grade a command, run it when the verdict is `allow` |
| `POST` | `/v1/download` | read a file through the session, base64 |
| `POST` | `/v1/upload` | write a file through the session, base64 |

Limits: 64 KiB of headers, 32 MiB per body, 24 MiB per file, 256 KiB of command
output, 10 minutes per command. A larger file is refused rather than truncated.

Errors are `{ "error": "…", "kind": "…" }` where `kind` is one of `auth`,
`request`, `missing` or `backend`. A refused command is **not** an error: `exec`
answers `200` with `ran: false` and the gate's reasons, because "the policy said
no" and "the app is unreachable" must not look alike to a script.

## What `ask` means without a person

There is nobody to ask. An `ask` verdict reached from the service therefore
stays unrun and is recorded with `choice: "cancelled"`. Approving it is a
person's job, in the window. If an agent needs a command to run unattended, the
policy has to allow it — either by mode or by an explicit rule — and that
decision is visible in Settings → Security and in the audit chain.

Only `ssh` sessions can run commands; the service takes the host label the
window shows, so a rule scoped to `web-1` matches what an agent asks for.

## The CLI

```console
$ ferriterm-cli status
endpoint  C:\Users\…\AppData\Roaming\app.ferriterm.desktop\agent.json
app       0.1.0 (agent 0.1.0)
port      60303 on loopback, token required
pid       7156
sessions  1

$ ferriterm-cli hosts
生产服务器            admin@10.100.22.50:22    ssh  production  session c1bf2197-…

$ ferriterm-cli check rm -rf /
command  rm -rf /
risk     R4 destructive (recursive, bulk delete, system path, root delete)
verdict  deny

$ ferriterm-cli exec ls -la
total 24
…

$ ferriterm-cli exec rm -f /tmp/probe
refused: R2 modifies (no flags)
verdict ask in ask mode
an approval needs a person: approve it in the app, or set the policy to automatic
```

Exit codes: `0` ran (or answered), `1` usage or transport failure, `3` the gate
refused. Options are long-form only and stop at the first word that is not one,
which is why `exec ls -la` means what it looks like; `--` ends options
explicitly.

Files move whole:

```console
ferriterm-cli upload ./local.txt /tmp/remote.txt --host web-1
ferriterm-cli download /var/log/syslog ./syslog --session c1bf2197-…
```

Local paths are **Windows paths on Windows** — a Git Bash path like
`/c/Users/...` is not readable by the binary. Git Bash also rewrites arguments
that look like POSIX paths, so prefix commands with `MSYS_NO_PATHCONV=1` when
the remote path is absolute:

```console
MSYS_NO_PATHCONV=1 ferriterm-cli exec rm -f /tmp/probe
```

## As MCP

`ferriterm-cli mcp` speaks MCP on stdin and stdout, one JSON-RPC message per
line, so an editor or an agent can launch it as a child process:

```json
{
  "mcpServers": {
    "ferriterm": {
      "command": "ferriterm-cli",
      "args": ["mcp"]
    }
  }
}
```

It offers three tools, and no more:

| Tool | Reaches |
| --- | --- |
| `list_hosts` | saved hosts, and which one has a session open |
| `list_sessions` | live connections, with the id `run_command` takes |
| `run_command` | one command line, graded and then run — or refused |

`run_command` is not a second implementation of anything. It calls the same
`/v1/exec` route, so it is graded by the same engine and recorded in the same
chain as the run button in the window. The refusal semantics are the service's:
in `ask` mode it does not run, and the answer says which rule or risk stopped it
and that an approval needs a person at the window.

Nothing but replies is ever written to stdout — a stray line would corrupt the
stream for the whole session — so all diagnostics go to stderr, and the process
exits when stdin closes.

Failures are tool results with `isError: true`, not JSON-RPC errors, because the
text is what a model reads before deciding what to do next: "2 sessions are
open; pass --host or --session" is actionable in a way that `-32603` is not.
Only an unknown tool name or method is a protocol error.

Protocol revisions `2025-06-18`, `2025-03-26` and `2024-11-05` are answered in
their own dialect; anything else gets `2025-06-18` and the client decides.

Settings → Agents shows the binary's real path and this entry, ready to copy —
the path comes from the running process, so it points at a file that exists
rather than at whatever `ferriterm-cli` happens to resolve to in a shell.

Check it without a client:

```console
$ printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
| ferriterm-cli mcp
```

## Verifying it

`cargo test -p ft-agent` covers the HTTP layer against a mock backend: auth
rejection, unknown routes, oversized and malformed bodies, the exec report
round-trip, and base64 file transfer. That suite needs no window, no SSH, and no
database, which is the point of the `Backend` trait.

The end-to-end path was checked by hand against a real host: `exec ls -la` ran
and printed; `exec rm -f …` in ask mode refused and left the file in place;
`upload`/`download` round-tripped 26 bytes; the audit view then showed all ten
records with the chain reporting "complete".

`cargo test -p ferriterm-cli` covers the MCP envelope without a process or an
app: initialize in the client's revision and the fallback, silence for
`notifications/initialized`, `ping`, an unknown method, the tool list, an
unknown tool, a missing `command`, and a tool call with no reachable app. The
last one asserts the `isError` text mentions the endpoint file, because that is
the sentence a person reads when they forgot to start the app.

## Known edges

- The audit chain records what was decided, not who asked: a command run from
  the window, the CLI and an agent all look alike apart from the timestamp. An
  `origin` field would fix that and is worth adding before the service stops
  being a one-person convenience.
- The MCP surface has no file transfer on purpose. Writing a file is a write,
  so it belongs behind the same gate as everything else; an agent that needs one
  can redirect inside `run_command` and be graded for it.
- The fork-bomb detector reads the raw line before parsing, so
  `echo ':(){ :|:& };:'` is graded R4 even though it only prints. That is the
  conservative side of the trade, and it is why the line is shown verbatim in
  the approval card.
- Command output is capped, not streamed. A long-running command returns
  nothing until it finishes.
- There is no UI switch for the service yet: it starts with the app and stops
  with it. A disable toggle belongs with the agent settings in a later stage.
