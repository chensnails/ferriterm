# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow the
package version in `package.json`, which `tools/sync-version.mjs` copies into
the Cargo workspace.

## [Unreleased]

### Added

- Settings → Agents can switch the control service off without closing the app.
  The switch ends the listener and deletes the endpoint file, so the CLI fails
  immediately and says why instead of waiting on a dead port; hosts, rules and
  the audit chain are untouched. Turning it back on reuses the port and token
  from before, so a client configured a minute ago still works. The choice lives
  in `agent:enabled` and is applied before the window appears.
- Settings → Agents collects what an external agent needs: the CLI's real path
  and a ready `mcpServers` entry, each with a copy button. Nothing is written to
  anyone else's configuration — every MCP client keeps it in a different file,
  in a different dialect, sometimes with comments, and guessing at that is a
  good way to break someone's editor.
- `ferriterm-cli mcp` speaks MCP on stdin and stdout, so an editor or an agent
  can launch it as a child process and call `list_hosts`, `list_sessions` and
  `run_command`. `run_command` is the same `/v1/exec` route as everything else —
  same policy engine, same audit chain — and an `ask` verdict comes back as a
  refusal that says which rule or risk stopped it and that an approval needs a
  person at the window. Failures are tool results with `isError` rather than
  JSON-RPC errors, because the sentence is what a model actually reads.
- A local control service and `ferriterm-cli`: a token-guarded API on loopback
  that lets a script or an agent list hosts and sessions, grade a command,
  run it, and move files. The port is picked by the kernel at each launch and
  the token is minted then too, so neither can be guessed or reused; both live
  in `agent.json`, readable only by this user and removed on exit. `exec` goes
  through the same policy engine and the same audit chain as the assistant's
  run button — an `ask` verdict from the CLI is not a prompt but a refusal,
  because there is nobody to ask. Files move over the session's SFTP channel.
- Every routing decision is now recorded in a hash-chained audit log:
  `%APPDATA%/<app>/audit/audit-YYYYMM.jsonl`, one line per evaluation with the
  command, host, risk, reasons, matched rule, verdict, the answer the person
  gave and the mode in force. Each line carries the previous line's hash, so a
  rewritten or removed entry breaks the chain at that point; Settings →
  Security shows the recent entries per month and verifies the chain, naming
  the first record that no longer matches. Files are kept for 12 months.
- Settings → Security collects the policy in one place: the mode
  (auto / ask before writes / plan only), the rule list with add and delete,
  and the audit view above. The mode is saved as you switch it; deleting a rule
  asks first, because it makes previously allowed commands ask again.
- The assistant's command card can now run a command, not only paste it. Run
  goes through the approval policy first: allowed commands execute, higher-risk
  ones open a card showing the literal line, its risk level and the reasons it
  was flagged, with once / this session / always-allow answers. "Insert" leaves
  the line at the prompt without pressing Enter, so pasting stays a preview.
  Always-allow writes an exact-match rule scoped to that host.
- Command approval policy (`ft-policy`): a shell line is split into simple
  commands, graded R0 (read only) to R4 (fatal) from its program name plus the
  modifiers that make the same program dangerous — recursion, `sudo`, system
  paths, block devices, credential files, `curl | sh`, disk writes — and
  answered with allow / ask / deny. User rules are glob patterns scoped to a
  host, last match wins, and a fatal delete can never be whitelisted. Typed
  commands are not routed through it: the engine exists for the paths a person
  does not type, which today means the assistant's run action.
- Built-in AI assistant (bring your own key): OpenAI-compatible and Anthropic
  endpoints, streaming chat, natural-language-to-command with an explicit
  insert step, and failure explanations. Providers and their models are managed
  in Settings → AI; API keys live in the credential vault under `ai:<id>` and
  are never returned to the interface. Terminal context is opt-in (none / last
  command / selection / screen), command output is off by default, and secrets
  are redacted in Rust before a request leaves the machine. Works with Qwen,
  DeepSeek, GLM, Kimi and Ollama through their OpenAI-compatible endpoints.
- Engineering assessment and phased remediation roadmap
  (`docs/assessment-2026-11-10.md`).
- LICENSE (GPL-3.0-only), README, CONTRIBUTING and this changelog.
- Self-update in Settings → About: manifest check, streamed download with
  progress, SHA-256/size verification, then platform install (macOS in-place
  replace + restart, Linux AppImage, Windows installer handoff).
- Baseline content security policy for production and development webviews.
- Global React error boundary with retry/reload, plus `window.onerror` and
  `unhandledrejection` logging.
- Vault password-throttle state persists across restarts and reboots, and
  derived key material is zeroized when dropped.
- SSH tabs reconnect on their own after a dropped connection (1s, 2s, 4s, 8s,
  15s backoff); after the last attempt the tab falls back to the manual
  Reconnect button instead of retrying forever.
- Automatic sync reports its outcome in Settings (success time, failure
  reason, conflict) and retries a transient failure after 1 and 3 minutes.
- Diagnostics are written to `<app data>/logs/ferriterm.log` at INFO and above
  (`RUST_LOG` overrides the filter). Session open/reap and failed connection
  attempts are recorded without credentials. The file rotates at 4 MiB with one
  previous generation, and panics are recorded with a backtrace.
- Fault-injection tests cover an abrupt server loss and a blackholed link: a
  session is reaped and surfaced as closed within a bounded time instead of
  hanging.
- Windows SSH agent support: the Agent option connects to the OpenSSH named
  pipe and falls back to Pageant, so it no longer only works on Unix.
- Per-host character set (UTF-8 by default; GBK, GB18030, Big5, Shift_JIS,
  EUC-JP, EUC-KR, Windows-1252): shell output is streamed through a decoder,
  input is re-encoded before it reaches the shell, and SFTP names, the home
  path and text files use the same charset.
- File-pane path joins and parent lookups handle Windows drive roots and UNC
  shares the same way on both the local and remote side.
- Hashed known_hosts entries (`|1|…`) are listed, matched and removable
  instead of being skipped.
- CI runs the backend suite, clippy and the loopback SSH smoke tests on
  Linux, macOS and Windows.
- Terminals use the Unicode 11 width table (correct emoji/CJK cell widths) and
  the WebGL renderer when available, falling back to the DOM renderer. Links
  open in the system browser through the opener plugin.
- Clipboard reads and writes go through a shared helper that falls back to the
  native clipboard when the webview denies `navigator.clipboard`, covering
  terminal copy/paste, OSC 52, key import and copied host/path addresses.
- FTP servers can be mounted as a file-pane target next to WebDAV and S3:
  passive mode (`EPSV` with a `PASV` fallback), `MLSD` listings with a `LIST`
  parser for servers that lack it, `REST`/`APPE` for resumed transfers, and
  `MFMT` timestamps when the server advertises them. FTP has no POSIX
  permissions, so files report `0644` and directories `0755`; `chmod` is
  refused rather than silently ignored. Plain FTP only — FTPS is not
  implemented, and the dialog says so.

### Changed

- Linux bundles are paused (2026-11-12): releases are built for macOS
  (arm64/x64) and Windows (x64). Linux source and CI jobs remain in tree, and
  the release matrix entry is commented out so resuming needs no porting.
- AI model compatibility is now driven by three protocol families behind one
  adapter — OpenAI-compatible, Anthropic Messages and Azure OpenAI — so adding
  a vendor is a preset, not a new client. Eighteen presets ship with their base
  URLs and models filled in (OpenAI, Anthropic, Gemini, Azure, DeepSeek, Qwen,
  GLM, Kimi, MiniMax, SiliconFlow, Volcengine Ark, Qianfan, Hunyuan, Yi,
  StepFun, Groq, OpenRouter, Ollama) and any field stays editable.
- Base URLs are normalised before use: a bare host, a versioned base
  (`/v1`, `/v2`, `/api/v3`), an already-complete `/chat/completions` path and
  both Azure URL styles all resolve to the right endpoint instead of producing
  a doubled path.
- Provider settings gained the parameters vendors disagree on: output-cap field
  name (`max_tokens` vs `max_completion_tokens`), a sampling on/off switch for
  reasoning models, `top_p`, stop sequences, system-prompt folding, stream usage,
  reasoning effort, `api-version`, extra request headers and an explicit proxy.
  Numeric values are clamped to each protocol's accepted range on save.
- A request rejected with 400/422 is retried once with the offending difference
  removed (cap field name, sampling parameters, or system role), and the
  interface says which adjustment was applied instead of failing silently.
- Settings → AI can pull the model list from the vendor (`GET /models`,
  including the `v1beta/openai` and Azure variants) and offer it as autocomplete;
  the field stays hand-editable when the call fails.

### Fixed

- The GPU probe and the updater no longer flash a console window: both spawn
  helper processes (`powershell`, `tar`, `taskkill`) without a window, which a
  console-less GUI build would otherwise pop onto the screen on every probe.
- Settings → Security refreshed its audit view when the month changed but not
  when new records landed, so a command run from the CLI left the list and its
  count behind the chain's own answer. Verifying now reloads both.
- A confirmation asked from inside Settings now appears on top of it instead of
  underneath: dialogs share one z-index in the stylesheet, so they are stacked
  by mount order and only the topmost one answers Escape. Closing a confirm no
  longer closes the dialog that asked it, and the request is no longer left
  hanging behind a card the person cannot see.
- Transfer progress forwarding survives a lagged broadcast instead of stopping
  the UI feed for every running job.
- WebDAV uploads are guarded by the remote ETag: when another device wrote the
  bundle since this device's last transfer, the upload is refused and Settings
  offers "Pull remote" or "Overwrite remote" instead of silently replacing that
  device's data. Servers that publish no ETags keep the unconditional upload.
- A session that dies on its own releases what it held: port forwards, the
  SFTP channel, the remote-monitor feed and its queued transfers pause instead
  of spinning on a channel that can never answer. The cleanup runs before the
  closed event, so an immediate reconnect is not torn down by the old loop.
- Resume no longer trusts a target file's size alone: the bytes already there
  are compared against the source before the copy continues, and a mismatch
  restarts the copy instead of silently producing a corrupt file.
- Vault and known_hosts files are replaced atomically and keep a previous
  generation; interrupted enable/disable/change-master windows roll back on
  startup or unlock instead of leaving an undecryptable vault (kill -9
  injection tested).
- SFTP requests time out after 30s and retire the connection; owner-name exec
  is bounded by a 15s deadline and a 1 MiB output cap.
- Loopback smoke examples compile against the current `SshConnectArgs`.
- Vault backups are flushed through a write handle before the rename. Windows
  rejects `FlushFileBuffers` on a read-only handle, so every vault write after
  the first one failed with "Access is denied".

## [0.1.0] - 2026-11-09

First feature-complete build.

### Added

- Host tree, tags, search, multi-tab terminals with split panes and layout
  restore; SSH (keys, agent, keyboard-interactive, ProxyJump, TOFU known_hosts,
  legacy algorithm presets), local PTY, serial and telnet sessions.
- SFTP file manager, transfer queue with pause/resume, directory sync.
- Local/remote/SOCKS5 forwarding, X11 forwarding (experimental).
- Local and remote monitoring, snippets, broadcast input with production
  confirmation, command palette, terminal search, asciicast recording and
  playback.
- Encrypted credential vault with device-key and master-password modes.
- Encrypted backup/sync over Gist, WebDAV and S3 with version history and
  reversible rollback.
- Themes, language packs (en, zh-CN), plugin host with WASM sandbox.
- CI (fmt/clippy/tests/loopback smoke) and a four-platform release workflow
  with signing hooks and a SHA-256 update manifest.