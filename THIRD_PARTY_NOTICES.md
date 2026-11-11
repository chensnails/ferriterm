# Third-Party Notices

Ferriterm is an independent, clean-room project. It is not affiliated with,
endorsed by, or derived from Termora or ShellAX. No source code, icons,
screenshots, or UI copy from those projects is included in this repository.

This file lists the direct dependencies of Ferriterm and their licenses.
Full license texts ship with each package (crate sources under `~/.cargo`,
npm packages under `node_modules`); upstream links are provided for
attribution. Versions reflect the current lockfiles.

## Rust crates

### Runtime / shipped in the desktop binary

| Component | Version | License | Upstream |
| --- | --- | --- | --- |
| tauri | 2.12.5 | Apache-2.0 OR MIT | https://github.com/tauri-apps/tauri |
| tauri-build | 2.7.4 | Apache-2.0 OR MIT | https://github.com/tauri-apps/tauri |
| tauri-plugin-opener | 2.7.3 | Apache-2.0 OR MIT | https://github.com/tauri-apps/plugins-workspace |
| tauri-plugin-clipboard-manager | 2.5.0 | Apache-2.0 OR MIT | https://github.com/tauri-apps/plugins-workspace |
| russh | 0.65.1 | Apache-2.0 | https://github.com/Eugeny/russh |
| portable-pty | 0.9.0 | MIT | https://github.com/wezterm/wezterm |
| serial2 | 0.2.39 | MIT | https://github.com/de-vrije/serial2 |
| rusqlite | 0.41.1 | MIT | https://github.com/rusqlite/rusqlite |
| tokio | 1.54.1 | MIT | https://github.com/tokio-rs/tokio |
| serde | 1.0.229 | MIT OR Apache-2.0 | https://github.com/serde-rs/serde |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | https://github.com/serde-rs/json |
| reqwest | 0.13.5 | MIT OR Apache-2.0 | https://github.com/seanmonstar/reqwest |
| tracing | 0.1.44 | MIT | https://github.com/tokio-rs/tracing |
| tracing-subscriber | 0.3.23 | MIT | https://github.com/tokio-rs/tracing |

Notable transitive components:

| Component | Version | License | Notes |
| --- | --- | --- | --- |
| ring | 0.17.14 | Apache-2.0 AND ISC | cryptography for russh |
| flate2 | 1.1.11 | MIT OR Apache-2.0 | SSH compression |
| rsa | 0.10.0-rc.18 | MIT OR Apache-2.0 | RSA host/user keys |
| libsqlite3-sys | 0.39.1 | MIT | bundles SQLite |
| SQLite | bundled | Public domain | https://sqlite.org/copyright.html |
| webview2-com | 0.39.1 | MIT | Windows WebView2 bindings |

### Development only (test fixtures, examples)

The loopback SSH server and smoke-test examples use the same `russh` crate
listed above; no additional third-party crates are introduced.

## JavaScript / TypeScript packages

### Bundled into the frontend

| Package | Version | License | Upstream |
| --- | --- | --- | --- |
| react | 19.3.1 | MIT | https://github.com/facebook/react |
| react-dom | 19.3.1 | MIT | https://github.com/facebook/react |
| zustand | 5.1.0 | MIT | https://github.com/pmndrs/zustand |
| i18next | 24.2.3 | MIT | https://github.com/i18next/i18next |
| react-i18next | 15.7.4 | MIT | https://github.com/i18next/react-i18next |
| i18next-icu | 2.7.0 | MIT | https://github.com/i18next/i18next-icu |
| intl-messageformat | 10.7.18 | BSD-3-Clause | https://github.com/formatjs/formatjs |
| @xterm/xterm | 5.5.0 | MIT | https://github.com/xtermjs/xterm.js |
| @xterm/addon-fit | 0.10.0 | MIT | https://github.com/xtermjs/xterm.js |
| @xterm/addon-unicode11 | 0.9.0 | MIT | https://github.com/xtermjs/xterm.js |
| @xterm/addon-webgl | 0.19.0 | MIT | https://github.com/xtermjs/xterm.js |
| @xterm/addon-web-links | 0.11.0 | MIT | https://github.com/xtermjs/xterm.js |
| @tauri-apps/api | 2.12.4 | Apache-2.0 OR MIT | https://github.com/tauri-apps/tauri |
| @tauri-apps/plugin-opener | 2.7.3 | Apache-2.0 OR MIT | https://github.com/tauri-apps/plugins-workspace |
| @tauri-apps/plugin-clipboard-manager | 2.5.0 | Apache-2.0 OR MIT | https://github.com/tauri-apps/plugins-workspace |

### Build-time only (not shipped)

| Package | Version | License | Upstream |
| --- | --- | --- | --- |
| @tauri-apps/cli | 2.12.4 | Apache-2.0 OR MIT | https://github.com/tauri-apps/tauri |
| vite | 6.4.5 | MIT | https://github.com/vitejs/vite |
| @vitejs/plugin-react | 4.7.0 | MIT | https://github.com/vitejs/vite-plugin-react |
| typescript | 5.9.3 | Apache-2.0 | https://github.com/microsoft/TypeScript |
| @types/react | 19.3.0 | MIT | https://github.com/DefinitelyTyped/DefinitelyTyped |
| @types/react-dom | 19.3.0 | MIT | https://github.com/DefinitelyTyped/DefinitelyTyped |

## Bundled fonts

Latin subsets in `public/fonts/`, under the SIL Open Font License 1.1
(full text in `public/fonts/OFL.txt`).

| Font | Upstream |
| --- | --- |
| JetBrains Mono | https://github.com/JetBrains/JetBrainsMono |
| Fira Code | https://github.com/tonsky/FiraCode |
| IBM Plex Mono | https://github.com/IBM/plex |

## Notes

- Apache-2.0 components are used under the terms of the Apache License 2.0;
  refer to each upstream `NOTICE` file for additional attribution.
- Termora and ShellAX are licensed AGPL-3.0. Ferriterm shares no code with
  them; this project was implemented from scratch against public
  documentation and observed behavior.
- Ferriterm's own license has not been declared yet; it will be chosen before
  the first public release.
