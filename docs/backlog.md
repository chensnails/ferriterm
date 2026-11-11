# Backlog

Work that is not finished yet, with the state and evidence needed to resume.
Items are ordered by priority.

## 1. CI matrix

The End-to-end (WebDriver) job now passes on `ubuntu-24.04`
(run [43050736776](https://github.com/chensnails/ferriterm/actions/runs/43050736776),
commit `46cfb5c`). Two backend jobs are still red.

### 1.1 Backend (ubuntu-24.04): apt update aborts on a mirror mid-sync

- Symptom: the `Install Linux webview dependencies` step exits 100 during
  `apt-get update` against Aliyun:
  `E: Failed to fetch https://mirrors.aliyun.com/ubuntu/dists/noble-updates/main/i18n/Translation-en.xz File has unexpected size (314556 != 312708). Mirror sync in progress?`
- Cause: translation indexes are optional and are rewritten while a mirror
  syncs; a stale index checksum is fatal for `apt-get update` even though the
  Packages files the install needs are consistent.
- Next step: in `scripts/ci-apt.sh`, disable translation downloads
  (`-o Acquire::Languages=none`) and retry `apt-get update` in a bounded loop
  (3 attempts) before failing. Keep Tencent as priority 1 and Aliyun as
  priority 2, with proxies disabled for both, as the script already does.

### 1.2 Backend (windows-latest): ft-telnet exchange test sees no output

- Symptom: `tests::exchanges_bytes_with_a_listener` fails after the full 15 s
  poll window with `left: []`, `right: [108, 111, 103, 105, 110, 58, 32, 111,
  107, 13, 10]` (`b"login: ok\r\n"`) at `crates/ft-telnet/src/lib.rs:395`.
  macOS and Linux pass in under a second; the Windows job spends 15.06 s on
  the test.
- Relevant code: the client reader is a dedicated OS thread
  (`std::thread::spawn(move || pump(...))`, lib.rs:86). `pump` (lib.rs:248)
  silently stops on any read error outside
  `WouldBlock` / `TimedOut` / `Interrupted`, so a Windows-specific error kind
  would end delivery without a trace. The test cannot tell that apart from slow
  scheduling: it only asserts the sink contents, and the server thread's `read`
  has no timeout of its own (lib.rs:352), so it can block forever when the
  client bytes never arrive. No panic from the server thread appeared in the
  Windows log.
- Next step: make the failure observable before changing behavior. Record the
  last read error kind in the test sink (or the closed flag) and assert on it,
  add server-side accept/read timeouts, then classify the Windows error and fix
  `pump` or the test accordingly. Verify on the `windows-latest` job; a local
  macOS run does not reproduce it.

## 2. Release hardening (stage 3 remainder)

- Plugin signing and checksum verification for marketplace installs.
- Release pipeline CLI plus real Homebrew and winget manifests/hashes (the
  current values are placeholders).
- Reproducible builds: the toolchain is pinned; document and verify that
  artifacts are reproducible.
- Opt-in crash reporting, off by default and with no telemetry.
- Bump GitHub Actions to Node 24-compatible major versions. Runners warn that
  `actions/checkout@v4`, `actions/setup-node@v4` and `actions/upload-artifact@v4`
  still target Node.js 20.

## 3. Soak

- The 72 h soak started at 2026-11-11T12:42:53Z
  (`testenv/soak.sh 259200`, output under `soak-runs/20261111T124253Z`).
- The 12 / 24 / 48 / 72 h checkpoints have not been recorded yet. The local
  process was left running when this backlog was written; stop it if the soak
  is no longer needed.
