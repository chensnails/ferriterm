# Local control service and `ferriterm-cli`

Ferriterm listens on loopback so a script, an agent, or the bundled CLI can
drive it without a person clicking. This document is the contract: what the
service exposes, how it authenticates, and what it deliberately refuses to do.

## Why it exists

The assistant inside the app can propose a command, but so can an agent outside
it. Rather than give each of them its own path into the machine, they all go
through one service that applies the same policy engine and writes to the same
audit chain as the assistant's run button. A command run from the CLI is
indistinguishable in the audit log from one run in the window, apart from where
it came from.

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

## Verifying it

`cargo test -p ft-agent` covers the HTTP layer against a mock backend: auth
rejection, unknown routes, oversized and malformed bodies, the exec report
round-trip, and base64 file transfer. That suite needs no window, no SSH, and no
database, which is the point of the `Backend` trait.

The end-to-end path was checked by hand against a real host: `exec ls -la` ran
and printed; `exec rm -f …` in ask mode refused and left the file in place;
`upload`/`download` round-tripped 26 bytes; the audit view then showed all ten
records with the chain reporting "complete".

## Known edges

- The fork-bomb detector reads the raw line before parsing, so
  `echo ':(){ :|:& };:'` is graded R4 even though it only prints. That is the
  conservative side of the trade, and it is why the line is shown verbatim in
  the approval card.
- Command output is capped, not streamed. A long-running command returns
  nothing until it finishes.
- There is no UI switch for the service yet: it starts with the app and stops
  with it. A disable toggle belongs with the agent settings in a later stage.
