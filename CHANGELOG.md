# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow the
package version in `src-tauri/tauri.conf.json`.

## [Unreleased]

### Added

- Engineering assessment and phased remediation roadmap
  (`docs/assessment-2026-11-10.md`).
- LICENSE (GPL-3.0-only), README, CONTRIBUTING and this changelog.
- Self-update in Settings → About: manifest check, streamed download with
  progress, SHA-256/size verification, then platform install (macOS in-place
  replace + restart, Linux AppImage, Windows installer handoff).
- Baseline content security policy for production and development webviews.

### Fixed

- Vault and known_hosts files are replaced atomically and keep a previous
  generation; interrupted enable/disable/change-master windows roll back on
  startup or unlock instead of leaving an undecryptable vault (kill -9
  injection tested).
- SFTP requests time out after 30s and retire the connection; owner-name exec
  is bounded by a 15s deadline and a 1 MiB output cap.
- Loopback smoke examples compile against the current `SshConnectArgs`.

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