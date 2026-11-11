# Ferriterm

A cross-platform SSH terminal and file-transfer client built with Rust, Tauri 2
and xterm.js. Clean-room implementation: no code copied from Termora or any
other client; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

## Features

- Host tree with folders, tags, search and drag-and-drop; multi-tab sessions
  with split panes and layout restore.
- SSH with key/agent/password/keyboard-interactive auth, ProxyJump chains,
  TOFU host-key verification, keepalive, and legacy-algorithm presets for old
  servers; local PTY, serial and telnet sessions.
- SFTP file manager and a transfer queue with pause/resume and directory sync.
- Local, remote and dynamic SOCKS5 port forwarding; X11 forwarding (experimental).
- Local and remote host monitoring, command snippets, broadcast input with a
  production-host confirmation, command palette.
- Session recording (asciicast v2) and playback.
- Encrypted credential vault: device-key mode or master-password mode
  (PBKDF2 + ChaCha20-Poly1305), never touches the system keychain.
- Encrypted backup and sync (Gist/WebDAV/S3) with version history and
  reversible rollback.
- Themes and language packs; English and Simplified Chinese built in.
- Plugin host with a fuel- and memory-limited WASM sandbox.

## Install

Prebuilt bundles for macOS (arm64/x64), Windows (x64) and Linux (x64) are
attached to [GitHub releases](https://github.com/chensnails/ferriterm/releases).
Updates are checked against the release `latest.json` manifest; artifacts are
verified by SHA-256 and size before installation (Settings → About).

## Build from source

Requirements: Rust 1.99.1 (`rust-toolchain.toml`, installed by rustup on the
first build), Node 22 (`.nvmrc`), and the Tauri 2 platform prerequisites
(WebKitGTK 4.1 on Linux, Xcode command line tools on macOS, MSVC on Windows).

```sh
npm ci
npm run tauri dev      # development window
npm run tauri:build    # release bundles in src-tauri/target/release/bundle
```

Quality gates:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm run build
node tools/ui_lint.mjs
```

`testenv/setup.sh` plus the `loopback_sshd`, `ssh_smoke` and `ft_ssh_smoke`
examples provide a loopback SSH fixture used by CI and local smoke tests.

## Security notes

- Credentials stay in an encrypted local file; the master-password key exists
  only in memory while unlocked. Vault files are written atomically and keep a
  previous-generation backup for crash recovery.
- Unknown host keys are trusted on first use only after explicit confirmation.
- Update artifacts must be HTTPS and match the published SHA-256 digest and
  size; see [docs/RELEASING.md](docs/RELEASING.md).
- No telemetry, no analytics, no network calls except the ones you configure
  (SSH/SFTP targets, sync endpoints, update checks).

## Documentation

- [docs/PRD.md](docs/PRD.md) — requirements and acceptance gates
- [docs/UI_SPEC.md](docs/UI_SPEC.md) — interface specification
- [docs/AGENT.md](docs/AGENT.md) — local control service and `ferriterm-cli`
- [docs/RELEASING.md](docs/RELEASING.md) — release and signing handbook
- [docs/assessment-2026-11-10.md](docs/assessment-2026-11-10.md) — engineering
  assessment and remediation roadmap
- [docs/backlog.md](docs/backlog.md) — remaining CI and release work

## License

GPL-3.0-only. See [LICENSE](LICENSE). Contributions are accepted under the
same license; see [CONTRIBUTING.md](CONTRIBUTING.md).