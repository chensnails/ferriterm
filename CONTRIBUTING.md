# Contributing to Ferriterm

Thanks for considering a contribution. Ferriterm is a clean-room project: do
not copy code, assets or strings from Termora or any other terminal client,
even GPL-licensed ones. Describe behaviour in your own words and implement it
independently.

## Setup

```sh
npm ci
npm run tauri dev
```

Rust 1.99.1 and Node 22 are the reference toolchain: `rust-toolchain.toml`
makes rustup install the compiler, rustfmt and clippy at that exact version,
and `.nvmrc` names the Node release CI uses. Linux needs WebKitGTK 4.1 and the
usual Tauri 2 prerequisites.

Builds fetch crates from crates.io. If your network needs a mirror, put the
source replacement in your own `~/.cargo/config.toml`; the repository does not
carry one, so CI and every contributor resolve the same public index.

## Before opening a pull request

Run every gate locally; CI runs the same set:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm run build
npm test
node tools/ui_lint.mjs
```

- Adding or bumping a dependency also means `cargo deny check` (install once
  with `cargo install cargo-deny`); it enforces the policy in `deny.toml` for
  licenses, RustSec advisories and registry sources.
- UI changes must be exercised in a running app (dev or release), not only
  typechecked. Include a screenshot for visual changes.
- User-visible strings go through i18n: add the key to both `src/i18n/en.json`
  and `src/i18n/zh-CN.json`; English is the source of truth.
- New backend behaviour needs unit tests in the owning `ft-*` crate; keep
  Tauri commands thin (`src-tauri/src/commands.rs` delegates to crates).
- Credential, key material and host-key code paths must fail closed: return
  errors, never unwrap on user data, never log secrets.
- Commits follow `type(scope): summary` (feat, fix, docs, chore, refactor,
  test); keep one logical change per commit.

### End-to-end smoke test

CI drives the real app through `tauri-driver`, which only supports Linux and
Windows. The flow creates a host, connects to the loopback fixture, trusts its
host key and checks the command output in the terminal. On Linux:

```sh
cargo install tauri-driver --locked
bash testenv/setup.sh
cargo build -p ferriterm --examples

./target/debug/examples/loopback_sshd &
VITE_E2E=1 npm run build && cargo build -p ferriterm
TAURI_WEBVIEW_AUTOMATION=true xvfb-run -a npm run e2e
```

The app writes to an isolated data directory per run, and screenshots plus the
page source of a failure land in `e2e-artifacts/` (gitignored). Run the build
with `VITE_E2E=1` before `npm run e2e`; without it xterm renders to a canvas
that WebDriver cannot read.

## Reporting bugs and security issues

Open a GitHub issue with reproduction steps, platform and build label
(Settings → About). For security-sensitive reports (vault, host-key trust,
update verification), email the maintainer listed on the GitHub profile
instead of opening a public issue.

## License

By contributing you agree that your contributions are licensed under
GPL-3.0-only, the project license (see `LICENSE`).