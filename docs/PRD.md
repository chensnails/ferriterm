# Ferriterm Product Requirements (PRD)

- Status: P0 baseline v1 (2026-11-08)
- Companion documents: [development-plan.md](development-plan.md) (reference analysis, phases), [UI_SPEC.md](UI_SPEC.md) (layout, tokens, acceptance)
- Clean-room rule: Termora and ShellAX (both AGPL-3.0) are analyzed for features and interaction flows only. No source code, icons, screenshots-as-assets, or UI strings are copied into this repository.

## 1. Product summary

Ferriterm is a cross-platform (macOS / Windows / Linux) SSH terminal client built with Rust + Tauri 2 + xterm.js. UI style and core interaction flows follow Termora; selected advanced terminal capabilities and engineering practices are adopted from ShellAX.

Language strategy: development is English-first (code, comments, commits, specs). The product ships `en` + `zh-CN` bundled; `en` is the source locale. Additional locales are delivered as data-only language packs through the plugin-host manifest interface.

## 2. Personas

| Persona | Primary needs |
|---|---|
| Ops engineer | Many hosts, jump hosts, production-host safety, bulk file transfer, port forwarding |
| Homelab enthusiast | Routers / NAS (OpenWrt, Linux boxes), quick commands, session resume |
| Developer | Local + remote shells, split views, layout restore, command history |

## 3. Functional requirements

Milestone tags: M1 = internal dogfood, M2 = public beta, M3 = v1.0.

### Terminal

| ID | Requirement | Milestone |
|---|---|---|
| TERM-01 | Multi-tab sessions with integrated top tab bar | M1 |
| TERM-02 | Horizontal/vertical split by dragging a tab to a panel edge | M1 |
| TERM-03 | In-terminal search (regex) | M1 |
| TERM-04 | Copy/paste, rectangular selection | M3 |
| TERM-05 | Font, color scheme, scrollback configuration | M1 |
| TERM-06 | Local shell sessions (PTY) | M1 |
| TERM-07 | Session logging to disk | M1 |
| TERM-08 | History-based command suggestion + prompt history dropdown, sensitive-command filtering | M3 |
| TERM-09 | asciicast v2 recording/playback, masked by default (no echo-less input recorded) | M3 |
| TERM-10 | Layout snapshot, rollback, restore on startup | M3 |
| TERM-11 | Broadcast input; confirmation required when targets include production hosts | M3 |

### SSH

| ID | Requirement | Milestone |
|---|---|---|
| SSH-01 | Password authentication | M1 (scaffold done) |
| SSH-02 | Public-key authentication | M2 |
| SSH-03 | ssh-agent integration (Windows via OpenSSH pipe) | M2 |
| SSH-04 | keyboard-interactive (2FA) | M2 |
| SSH-05 | known_hosts verification with TOFU fingerprint prompt; **blocking gate before beta** | M2 |
| SSH-06 | ProxyJump chains | M2 |
| SSH-07 | Local/remote/dynamic forwarding (-L/-R/-D), auto-start with host connect, live stats | M2 |
| SSH-08 | Keepalive, automatic reconnect | M2 |
| SSH-09 | X11 forwarding (experimental, labeled as such) | M2 |
| SSH-10 | Optional remote tmux session persistence | M3 |

### Hosts

| ID | Requirement | Milestone |
|---|---|---|
| HOST-01 | Nested folder tree (`a/b/c`) | M1 |
| HOST-02 | Color tags, tag management | M1 |
| HOST-03 | Host search with highlight | M1 |
| HOST-04 | Full context menu (connect with jump, new window, tags, import, properties) | M1 |
| HOST-05 | Properties dialog (auth, proxy, notes, production flag) | M1 |
| HOST-06 | Importers: Xshell, FinalShell, `~/.ssh/config` | M1 |
| HOST-07 | Open session in a new window | M1 |
| HOST-08 | Production-host marker used by broadcast/write confirmations | M1 |

### SFTP and transfer

| ID | Requirement | Milestone |
|---|---|---|
| SFTP-01 | SFTP floating panel over the terminal | M2 |
| SFTP-02 | Transfer tab: Local + N remote sub-tabs, dual pane | M2 |
| SFTP-03 | Six-column file table (name, size, type, modified, permissions, owner) | M2 |
| SFTP-04 | Bookmarks, hidden-files toggle | M2 |
| SFTP-05 | File context menu: transfer, edit, copy path, rename, delete, fast delete (warned), permissions, new | M2 |
| SFTP-06 | Visual rwx permission editor | M2 |
| SFTP-07 | Standalone editor window, save auto-uploads, conflict detection | M2 |
| XFER-01 | Queue with 6 concurrent transfers | M2 |
| XFER-02 | Recursive directory copy | M2 |
| XFER-03 | A↔B server-to-server via client relay, explicitly labeled | M2 |
| XFER-04 | Progress, speed, ETA, source/target columns | M2 |
| XFER-05 | Pause / cancel / retry | M2 |
| XFER-06 | Resume interrupted transfers | M2 |
| XFER-07 | Directory sync (one/bi-directional) with diff preview and delete confirmation | M2 |

### Monitoring, commands, plugins, sync

| ID | Requirement | Milestone |
|---|---|---|
| MON-01 | System info floating panel: CPU, memory, swap, load, uptime, filesystems | M3 |
| MON-02 | Nvidia GPU utilization | M3 |
| CMD-01 | Command library: groups, `{{variable}}` fill-in, insert by focus target | M3 |
| CMD-02 | Per-host and global command history card | M3 |
| PLUG-01 | Plugin manifest + built-in plugin modules | M3 |
| PLUG-02 | Language-pack registration interface (data-only packs) | M3 |
| PLUG-03 | WASM sandbox for external extensions (no imports, instruction/memory limits) | M3 skeleton |
| SYNC-01 | Encrypted config sync to Gist / WebDAV | M3 |
| SYNC-02 | Storage panes: WebDAV, S3 | M3 |

### Settings, i18n, security, release

| ID | Requirement | Milestone |
|---|---|---|
| SET-01 | Settings center grouped by domain | M3 |
| SET-02 | Multiple themes; import of own TOML scheme format | M3 |
| SET-03 | Customizable keymap and context menus | M3 |
| I18N-01 | `en` source locale; all UI strings via i18n keys; CI lint bans hardcoded literals | M1 skeleton |
| I18N-02 | `zh-CN` bundled; follows system language, manual override | M3 |
| I18N-03 | Other locales as downloadable language packs (manifest + checksum, no code execution) | M3 |
| I18N-04 | Missing keys fall back to `en` and are reported | M3 |
| SEC-01 | Host-key TOFU (see SSH-05) | M2 |
| SEC-02 | Credentials in an encrypted local vault (the system keychain is never used); never in config files | M1 |
| SEC-03 | Audit-log and permission-decision traits reserved at P0 even though enforcement lands later | M1 |
| SEC-04 | Recording/history masking defaults (see TERM-09, TERM-08) | M3 |
| REL-01 | Signed packages: macOS notarized, Windows signed, Linux deb/AppImage | M3 |
| REL-02 | Auto-update with SHA256 verification; failed verification refuses install | M3 |
| REL-03 | Distribution via brew / winget | M3 |

## 4. Out of scope for this release

Database / Redis / Docker / Homebrew modules and utility-tool collections; full AI assistant and agent-governance stack; Sixel/Kitty graphics protocols; Android/iOS. All are M4 candidates requiring a separate PRD review.

## 5. Non-functional requirements

| ID | Requirement |
|---|---|
| NFR-01 | Cold start ≤ 2 s on the reference Apple-Silicon Mac |
| NFR-02 | Idle resident memory ≤ 150 MB with one session (target, measured per release) |
| NFR-03 | Keystroke echo ≤ 50 ms on LAN SSH sessions |
| NFR-04 | Offline-first; no telemetry, no network calls except user-initiated connections/updates/sync |
| NFR-05 | Installer ≤ 30 MB per platform (target) |

## 6. Milestones and acceptance gates

- **M1 (P0–P2)**: testenv loopback SSH auto-login green; Home screen menu-path parity checklist signed; multi-tab terminal dogfoodable.
- **M2 (P3–P4)**: authentication/forwarding matrix green on testenv + real hosts; SSH-05 TOFU implemented; 1 GB transfer queue behavior checklist; sync diff preview correct.
- **M3 (P5–P7)**: clean install on three platforms; updater rejects bad checksum; en/zh switching and fallback verified; language-pack load verified.
- **M4**: optional extensions, separately reviewed, no date commitment.

## 7. Open decisions

- Product license: decided 2026-11-10, GPL-3.0-only (see `LICENSE`); clean-room
  obligations hold either way.
- X11 support depth after M2 experiments.
- Additional theme-import formats beyond own TOML.