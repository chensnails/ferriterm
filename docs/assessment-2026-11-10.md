# Engineering assessment — compatibility, stability, industrialization

Date: 2026-11-10. Scope: read-only audit of the whole workspace at commit
`114aee1`. Verdict: feature-complete beta. The gap to industrial grade is not
features but fault tolerance/recoverability, a closed release loop, and
cross-platform regression coverage.

Ratings: stability B−, compatibility B, industrialization C+.

## Existing guarantees worth keeping

- Stability: every Tauri command returns `Result`; zero `panic!/todo!` in
  non-test code; SSH connect timeout, keepalive, TOFU host keys; transfer
  pause/resume with restart recovery; sync uploads before recording a version
  and rolls back reversibly; vault rate limiting and constant-time comparison;
  xterm instances fully disposed; ~148 inline unit tests across 13 crates.
- Compatibility: three-branch platform conditionals (no macOS-private APIs);
  frontend Windows drive-letter path handling; SSH legacy algorithm presets
  (ssh-rsa, DH group1/14, CBC); ProxyJump and keyboard-interactive; en/zh-CN
  message catalogs fully aligned.
- Industrialization: CI runs fmt/clippy/tests plus a loopback SSH smoke;
  release workflow builds a four-platform matrix with Apple/Windows signing
  and updater artifacts; `docs/RELEASING.md`, `THIRD_PARTY_NOTICES.md`, and both
  lockfiles are committed.

## Stability gaps (severity order)

| Sev | Gap | Evidence | Fix |
| --- | --- | --- | --- |
| P0 | Vault/known_hosts written with truncate; a crash mid-write corrupts them. Mode switches write multiple files in an order that can leave the vault undecryptable. | `crates/ft-security/src/lib.rs` `write_private`, `enable_master_password` | tmp+rename atomic writes, keep previous generation as `.bak`, startup/unlock recovery, crash-safe ordering |
| P0 | SFTP `request()` holds the io mutex and waits forever; a half-open connection hangs the session and defeats cancellation. `run()` has no timeout or output cap. | `crates/ft-vfs/src/sftp.rs` | per-request timeout, mark connection broken on timeout (avoid protocol desync), cap exec output |
| P1 | Transfer event forwarder treats broadcast `Lagged` as termination; one lag stops UI progress forever. | `src-tauri/src/lib.rs` setup | continue on `Lagged` |
| P1 | No backend cleanup or auto-reconnect after disconnect; dead sessions/forwards linger. | `src/store.ts` markClosed | backend reaper on closed, optional backoff reconnect |
| P1 | Sync is last-write-wins without ETag/version checks; auto-sync failures only console.warn. | `crates/ft-sync/src/remote.rs`, `src/autoSync.ts` | conflict detection, retry, user-visible status |
| P1 | No frontend ErrorBoundary or unhandledrejection handler. | `src/main.tsx` | global boundary + error entry point |
| P2 | DB mutex poisoning chains panics; a corrupt DB aborts startup with no fallback. | `crates/ft-store/src/lib.rs` | poison recovery, degraded/repair mode |
| P2 | Resume compares size only, not prefix content. | `crates/ft-transfer/src/lib.rs` | prefix hash validation |
| P2 | Vault rate limit resets on restart; change/disable not rate limited; no zeroize. | `crates/ft-security/src/lib.rs` | persist failure count, unify limiting, zeroize key buffers |

## Compatibility gaps

| Sev | Gap | Evidence | Fix |
| --- | --- | --- | --- |
| P1 | Windows ssh-agent branch always errors but stays in the fallback chain. | `crates/ft-ssh/src/lib.rs` | named-pipe agent or hide option on Windows |
| P1 | Rust-side path joining only understands `/`; inconsistent with frontend `paths.ts`; UNC and `~`→USERPROFILE mismatch. | `crates/ft-vfs/src/lib.rs` | shared cross-platform path rules |
| P1 | `encoding` field has no consumer; GBK servers garble; SFTP names are lossy UTF-8. | `crates/ft-vfs/src/sftp.rs` | decoding layer + filename encoding fallback |
| P1 | CI backend job runs on macOS only. | `.github/workflows/ci.yml` | three-platform matrix |
| P2 | No unicode11/webgl addons; clipboard read has no fallback; terminal links use window.open which Tauri blocks. | `src/terminal.ts` | addons, fallback, opener plugin |
| P2 | Remote monitoring hardcodes /proc (Linux-only remotes). | `crates/ft-monitor/src/lib.rs` | declare boundary or degrade |
| P2 | Hashed known_hosts entries cannot be matched or removed. | `crates/ft-ssh/src/known_hosts.rs` | hashed entry comparison |

## Industrialization gaps

P0 (block a formal release): no LICENSE (legally undistributable); no
README/CHANGELOG/CONTRIBUTING; `security.csp = null`; auto-update not closed —
`ft-security::update::verify_download` has no production caller and no updater
plugin is wired, so manifests are published but clients never update.

P1: no cargo-deny/audit/dependabot and a hardcoded crates mirror
(`.cargo/config.toml`); toolchain unpinned (no rust-toolchain/.nvmrc, version
edited in three places); no frontend tests/e2e/coverage (ft-pty has zero
tests); lint:ui/eslint/pre-commit not in CI.

P2: no logging framework or crash reporting; plugin market downloads lack
checksums/signatures; monolith files (commands.rs ~3.4k lines, style.css ~7.4k);
stale `src-tauri/Cargo.lock`/`.cargo` copies; brew/winget placeholder hashes;
timestamp injection breaks reproducible builds.

## Roadmap

- Phase 0 "dare to release": LICENSE + README + CHANGELOG; baseline CSP;
  closed update loop; vault/known_hosts atomic writes with crash recovery;
  SFTP timeouts. Acceptance: release checklist green, kill -9 injection during
  vault writes never loses credentials.
- Phase 1 "dare to run for days": Lagged fix, disconnect cleanup/reconnect,
  ErrorBoundary, tracing to disk, sync conflicts/retry, resume validation,
  vault rate-limit persistence and zeroize. Acceptance: fault injection
  (disconnect/half-open) never hangs; 72h soak without leaks.
- Phase 2 "dare to install anywhere": three-platform CI, Windows agent/paths,
  GBK decoding, terminal addons/clipboard/links, hashed known_hosts.
  Acceptance: Windows/Linux CI smoke green plus a GBK legacy-server regression.
- Phase 3 "dare to hand to a team": dependency governance, toolchain pinning,
  e2e critical paths, plugin signing, monolith splits, reproducible builds,
  opt-in crash reporting.

Highest return per effort: atomic writes, SFTP timeout, the one-line Lagged
fix, CSP, and the update loop — each removes one of the five most
reputation-damaging failure modes (credential loss, permanent hang, frozen
progress, unprotected webview, updates users never receive).