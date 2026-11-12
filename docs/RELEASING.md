# Releasing Ferriterm

The release workflow builds signed bundles for macOS (arm64 and x64) and
Windows (x64), then publishes `latest.json` with a SHA-256 digest for every
updater artifact. Linux bundles are paused as of 2026-11-12: the source and
Linux CI jobs stay in tree, and the `ubuntu-22.04` matrix entry in
`.github/workflows/release.yml` is commented out rather than deleted.

## 1. Bump the version

`package.json` is the only file you edit: the Tauri config points at it, and
`tools/sync-version.mjs` copies the number into the Cargo workspace as npm's
`version` lifecycle script. With a clean tree, run:

```sh
npm version minor -m "chore: release v%s"
git push origin main --tags
```

`npm version` accepts `patch`, `minor`, `major` or an explicit `x.y.z`, and
creates the `v*` tag the workflow expects. CI runs `npm run version:check`, so
a hand-edited `Cargo.toml` that disagrees with `package.json` fails before a
bundle is built.

Pushing a `v*` tag triggers `.github/workflows/release.yml`. The
`workflow_dispatch` entry point accepts the same version but assumes the tag and
version were bumped first.

## 2. Configure repository secrets

| Secret | Purpose |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | Updater signing key (required for `.sig` artifacts) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Password for the updater key |
| `APPLE_CERTIFICATE` | Base64 Developer ID Application `.p12` |
| `APPLE_CERTIFICATE_PASSWORD` | Password for the macOS certificate |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Example (TEAMID)` |
| `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | Notarization credentials |
| `WINDOWS_CERTIFICATE` | Base64 Authenticode `.pfx` |
| `WINDOWS_CERTIFICATE_PASSWORD` | Password for the Windows certificate |

Generate the updater key pair once with the bundled CLI:

```sh
npm run tauri signer generate -- -w ~/.tauri/ferriterm.key
```

Store the private key (and its password) as the two `TAURI_SIGNING_*` secrets.
Keep `~/.tauri/ferriterm.key.pub` in the repo or password manager; it is the
public half used by clients to verify update signatures.

MacOS jobs import the certificate into a temporary keychain and Windows jobs
import the `.pfx` and generate `src-tauri/tauri.windows.conf.json` with the
certificate thumbprint. Without those secrets the workflow still builds, but
the bundles are unsigned.

## 3. What the workflow produces

Each matrix job builds with `src-tauri/tauri.release.conf.json`, which enables
`bundle.createUpdaterArtifacts`. Tauri emits `.app.tar.gz`, `.AppImage.tar.gz`,
`.nsis.zip` and `.msi.zip` updater bundles next to their installers, each with a
`.sig` sidecar. All jobs upload to the same draft GitHub release. While Linux
bundles are paused no `.AppImage.tar.gz` is produced, so `latest.json` has no
`linux-x86_64` entry.

The final `manifest` job downloads the draft assets and runs
`scripts/release_manifest.py`, which writes `latest.json`:

```json
{
  "version": "0.2.0",
  "notes": "",
  "pubDate": "2026-11-08T00:00:00Z",
  "platforms": {
    "darwin-aarch64": {
      "url": "https://github.com/.../Ferriterm_0.2.0_aarch64.app.tar.gz",
      "signature": "…",
      "sha256": "…",
      "size": 12345678
    }
  }
}
```

`latest.json` is uploaded back to the release. Publishing the draft is the last
manual step; review the assets first.

## 4. Client-side verification

`ft-security::update` parses the manifest and verifies a downloaded artifact
before installation (`verify_download`). It rejects:

- unknown platforms, missing or malformed SHA-256 digests
- non-HTTPS download URLs
- size mismatches and digest mismatches (`ChecksumMismatch`)

The client consumes this in `src-tauri/src/updater.rs`: Settings → About →
"Check for updates" runs `update_check` against
`https://github.com/chensnails/ferriterm/releases/latest/download/latest.json`,
and "Download and install" runs `update_install`, which streams the artifact
with progress events, calls `verify_download`, and only then installs:
macOS extracts the `.app.tar.gz` and `ditto`s it over the running bundle
before restarting, Linux replaces the running AppImage (or installs to
`~/.local/bin`), Windows expands the NSIS zip, launches the installer and
exits so the bundle is unlocked. Draft releases are not served by the
`latest` URL, so checks only see published releases.

The rejection paths are covered by unit tests in
`crates/ft-security/src/update.rs` (`rejects_tampered_artifact`,
`rejects_insecure_url`, `rejects_missing_or_malformed_checksum`, and others).
Run them with:

```sh
cargo test -p ft-security update
```

## 5. Package managers

- **Homebrew** — update `packaging/homebrew/ferriterm.rb`: version, both DMG
  URLs and the `sha256` for each architecture, then open a PR against
  `Homebrew/homebrew-cask`.
- **Winget** — update `packaging/winget/*.yaml`: version, MSI URL and
  `InstallerSha256`, then submit to `microsoft/winget-pkgs` with
  `wingetcreate submit` or a manual PR.

Both package managers require the published (non-draft) release so their
download URLs and hashes resolve.
