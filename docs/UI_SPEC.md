# Ferriterm UI Specification

- Status: P0 baseline v1 (2026-11-08)
- Baseline style: Termora-like flat dark UI with integrated tab bar and floating panels. Interaction flows are compared against a locally installed Termora during acceptance; no Termora/ShellAX assets, icons, or strings are reused.
- Enforcement: every color/size/spacing/radius/duration comes from the token table below. CI lint rejects literals outside the token source. Every user-visible string is an i18n key (`en` source).

## 1. Layout skeleton

Single main window:

- **Top tab bar** (height `--tab-h`): a leftmost host-sidebar toggle button, a scrolling tab strip (Home tab with house glyph + session tabs + Transfer tab + "new tab" button) and a right-aligned, non-scrolling tool icon row (split, broadcast, SFTP, exec log, commands, monitor, forwarding, search, theme, language, settings). Active tab uses `--bg-elevated` + accent top rule; closed tabs keep a muted style until dismissed.
- **Host sidebar**: persistent, collapsible left pane (`--sidebar-w`, `#host-sidebar`) shared by all views; holds the host tree with search, new-host/new-folder/tag actions, `user@host` subtitles on host rows and host counts on folders/header. Toggle state persists via settings (`sidebarOpen`).
- **Home tab content**: centered original logo, tagline and quick-action buttons (quick connect, new host, new folder). The host tree lives in the sidebar, not in Home.
- **Session view**: terminal fills the content area; splits divide it with draggable gutters (`--split-gutter`).
- **Keep-alive views**: Home, Transfer and the terminal workspace stay mounted and are toggled with `hidden`, so xterm sessions survive tab switches.
- **Floating panels**: system info opens as a floating panel above the content (`--shadow-panel`); it never replaces the main view.
- **Transfer tab**: fills the content area like a session tab; dual file panes; transfer queue table docked at the bottom of the tab.
- **Status strip**: bottom of window, connection state + latency; hidden on Home.

## 2. Design tokens (own definitions)

### Color — dark (default)

| Token | Value | Use |
|---|---|---|
| `--bg-base` | `#0f1117` | window/terminal background |
| `--bg-panel` | `#151922` | sidebar, cards, dialogs |
| `--bg-elevated` | `#1c2130` | active tab, floating panels, menus |
| `--border` | `#2a3040` | separators, control borders |
| `--fg-primary` | `#d4d4d8` | body text |
| `--fg-muted` | `#8a93a9` | secondary text, placeholders |
| `--fg-on-accent` | `#ffffff` | text/icons on accent-filled controls |
| `--accent` | `#4f8cff` | primary actions, focus ring, active underline |
| `--accent-hover` | `#6ba1ff` | hover of primary actions |
| `--danger` | `#ef4444` | destructive actions, production marker |
| `--warning` | `#f59e0b` | caution states |
| `--success` | `#22c55e` | connected/healthy states |
| `--selection` | `#33415588` | text/table selection |
| `--overlay` | `rgb(0 0 0 / 0.45)` | modal backdrop scrim |
| `--tag-blue` | `#4f8cff` | host tag palette (shared across themes) |
| `--tag-green` | `#22c55e` | host tag palette |
| `--tag-amber` | `#f59e0b` | host tag palette |
| `--tag-red` | `#ef4444` | host tag palette |
| `--tag-purple` | `#a855f7` | host tag palette |
| `--tag-cyan` | `#06b6d4` | host tag palette |
| `--tag-pink` | `#ec4899` | host tag palette |
| `--tag-gray` | `#8a93a9` | host tag palette |

### Color — light

| Token | Value |
|---|---|
| `--bg-base` | `#f5f6f8` |
| `--bg-panel` | `#ffffff` |
| `--bg-elevated` | `#eef1f5` |
| `--border` | `#d7dce3` |
| `--fg-primary` | `#1f2430` |
| `--fg-muted` | `#6b7280` |
| `--fg-on-accent` | `#ffffff` |
| `--accent` | `#2563eb` |
| `--accent-hover` | `#1d4ed8` |
| `--danger` | `#dc2626` |
| `--warning` | `#d97706` |
| `--success` | `#16a34a` |
| `--selection` | `#bfdbfe88` |
| `--overlay` | `rgb(0 0 0 / 0.45)` |
| `--shadow-panel` | `0 12px 32px rgb(15 23 42 / 0.16)` |
| `--tag-blue` | `#4f8cff` |
| `--tag-green` | `#22c55e` |
| `--tag-amber` | `#f59e0b` |
| `--tag-red` | `#ef4444` |
| `--tag-purple` | `#a855f7` |
| `--tag-cyan` | `#06b6d4` |
| `--tag-pink` | `#ec4899` |
| `--tag-gray` | `#8a93a9` |

### Typography, spacing, geometry, motion

| Token | Value | Use |
|---|---|---|
| `--font-ui` | `13px / 1.4` | default UI text |
| `--font-ui-sm` | `12px / 1.4` | table meta, badges |
| `--font-title` | `15px / 1.4, 600` | dialog/section titles |
| `--font-terminal` | `14px / 1.2` | terminal default, user-adjustable |
| `--font-mono` | `"Ferriterm JetBrains Mono", "JetBrains Mono", "Cascadia Code", Menlo, Consolas, monospace` | terminal, paths (bundled latin subset) |
| `--space-1..5` | `4, 8, 12, 16, 24px` | spacing scale |
| `--radius-sm/md/lg` | `4, 6, 10px` | controls / cards / dialogs |
| `--radius-pill` | `999px` | capsule badges and chips |
| `--control-h` | `28px` | inputs |
| `--button-h` | `30px` | buttons |
| `--row-h` | `30px` | tree/table rows |
| `--tab-h` | `30px` | tab bar (compact chrome; 36px total with padding and rule) |
| `--tab-max-w` | `200px` | maximum width of a session tab |
| `--home-w` | `720px` | Home content column max width |
| `--font-logo` | `28px` | Home logo mark |
| `--sidebar-w` | `300px` | host sidebar + settings category pane |
| `--dialog-w` | `440px` | dialog width cap |
| `--suggest-w` | `420px` | inline command suggestion dropdown max width |
| `--settings-w` | `760px` | settings center width |
| `--settings-h` | `540px` | settings center pane height |
| `--settings-nav-w` | `156px` | settings category nav width |
| `--recordings-list-w` | `240px` | recordings list column width |
| `--player-h` | `360px` | asciicast replay viewport height |
| `--split-gutter` | `4px` | split drag handle |
| `--focus-width` / `--focus-offset` | `2px` / `2px` | keyboard focus ring |
| `--scrollbar-w` / `--scrollbar-pad` | `10px` / `2px` | thin scrollbar geometry |
| `--backdrop-blur` | `2px` | modal backdrop blur |
| `--shadow-panel` | `0 8px 24px rgb(0 0 0 / .35)` | floating panels |
| `--motion-fast` | `120ms ease-out` | hover/open transitions; disabled by reduce-motion setting |
| `--motion-panel` | `160ms ease-out` | dialog entrance; disabled by reduce-motion setting |
| `--disabled-opacity` | `0.5` | single disabled treatment for every control (baseline state token) |

### 2.1 Visual polish phase (S1–S4)

These phases refine the existing baseline; they do not redefine it. Rulings:

- **One disabled state.** Every `:disabled` / `.disabled` rule uses
  `--disabled-opacity`. Scattered `0.4`–`0.55` literals were retired so disabled
  controls no longer read as different "kinds" of unavailable.
- **No undefined tokens.** `--lh-ui` and `--bg-raised` were unresolved and
  silently dropped their declarations; they now resolve to `--line-ui` and
  `--bg-elevated`. `--split-ratio` is intentionally injected at runtime by
  `TerminalArea.tsx` and is the only variable without a stylesheet definition.
- **No sub-scale literals.** Badge typography/padding/radius and the two `3px`
  theme-card radii now come from tokens; `::placeholder` no longer applies an
  `0.8` alpha over `--fg-muted` (which dropped its contrast below 4.5:1).
- **Theme scope.** S1–S4 touch `:root` (shared by all themes) plus the default
  `dark`/`light` blocks only. The 37 bundled themes inherit the shared rules
  and are not individually rewritten. The retired `one-dark` (a duplicate of
  `atom-one-dark`) is resolved through `normalizeThemeId()` in `theme.ts`, so
  a stored setting from an older build still lands on a real theme.
- **Theme families.** The 37 bundled themes are grouped in the picker by source:
  `社区暗色` / `社区亮色` (popular open-source colour schemes — Nord, Dracula,
  Catppuccin, Tokyo Night, Gruvbox, Rosé Pine, Ayu, Monokai, Solarized, GitHub,
  Atom One, iTerm2, Termius, Tango, One Half…) and `软件原生` (the first-party
  baseline + warm/slate originals plus traditional emulator colour tables —
  Campbell, Campbell PowerShell, Vintage, SecureCRT Solarized Light). Community
  and emulator schemes keep their upstream ANSI palettes and chrome colours
  verbatim (same as the long-shipped nord/dracula/etc., which carry their
  authentic contrast), so a scheme looks exactly as its ecosystem defines it; the
  stricter 7:1 / 4.5:1 budget in §2.2 is held by the first-party warm/slate
  themes only.
- **Three-state kit (S2).** `EmptyState`, `Skeleton`, `LoadingState`,
  `ErrorState` and `Notice` share one rhythm and one accent edge. An empty
  state has a title/description hierarchy; loading uses a spinner plus a
  spoken label (`role="status"`); recoverable errors use a tinted danger block
  (`role="alert"`) with heading, detail and optional action. Full-screen
  render failures keep `ErrorBoundary` but reuse the same icon and button
  material. Panels must reuse these primitives instead of bare `hint` text for
  real loading/empty/error states; prose hints stay prose.
- **Warnings are not copy.** `.hint.warn` previously had no rule, so plugin and
  desktop-shell blockers read like ordinary hints. It now carries a warning
  edge and a tinted band; `.error-text` uses `--fg-primary` body text on a
  danger band so long messages stay legible in every theme.
- **One press treatment (S3).** Every control family answers a press with
  `--selected-bg`; `primary` keeps `--accent-hover`. Before this only the
  settings action row had `:active`, so clicks read the same as hovers. The
  switch is deliberately excluded (its thumb carries the pressed state) and
  its hard-coded white/radius/duration now come from tokens.
- **Layout stability (S3).** `--split-ratio` gains a `1fr` fallback so a split
  cannot collapse before `TerminalArea` injects the inline value. Long
  unbroken values in state/title/notice copy wrap instead of pushing controls
  out of compact panes. Verified by capture at 1518×1047, 1100×800, 860×700
  and 660×620 physical px: the 1080/860/720/620/560/440 tiers degrade the
  status bar and settings nav without overlap or clipping.

### 2.2 Warm grey family

Four themes — `warm-linen`, `warm-taupe` (light), `warm-graphite`, `warm-pine`
(dark) — ride one warm neutral surface ramp and differ by accent, not by
temperature. They are held to a stricter budget than the inherited themes, so
a warm background can never cost readability:

| Pair | Minimum | Why |
|---|---|---|
| `--fg-primary` on `--bg-base` / `--bg-panel` | 7:1 | body copy |
| `--fg-muted` on `--bg-base`, `--bg-panel`, `--bg-elevated` | 4.5:1 | hints sit on all three |
| `--accent` on `--bg-base` / `--bg-panel` | 4.5:1 | link/ghost button text |
| `--fg-on-accent` on `--accent` | 4.5:1 | primary button label |
| `--danger` / `--warning` / `--success` on `--bg-panel` | 4.5:1 | state text |
| `--tag-*` on `--bg-base` | 4.5:1 | chips only ever sit on the base surface |

The terminal palettes keep the same luminance bands (no ANSI colour below
4.5:1 on `--bg-base`), so `ls --color` output stays legible next to the chrome.
These four are additive: the 18 inherited themes still ship their upstream
palettes unchanged.

The achromatic sibling `slate-mono` follows the same table with one twist: the
surface ramp is a flat grey (#141414 / #1f1f1f / #262626 over a #3b3b3b
hairline) and `--accent` is `#e0e0e0` — a brighter surface instead of a hue, so
focus rings, the active-tab edge and the primary button gain contrast without
introducing a second colour. Its terminal palette keeps the same 4.5:1 floor on
`--bg-base`, so the chrome stays quiet while `ls --color` remains the only
chromatic content on screen.

## 3. Component inventory

| Component | Behavior notes | Tokens |
|---|---|---|
| TabBar / TabItem | drag to reorder and to split; middle-click closes; overflow scroll | `--tab-h`, `--bg-elevated`, `--accent` |
| ToolIconButton | tooltip with i18n label + shortcut; pressed state | `--button-h`, `--fg-muted` |
| HostTree / Node | nested folders, expand/collapse, drag-move, color tag chip, production dot | `--row-h`, `--danger` |
| SearchField | debounced filter, highlight matches | `--control-h` |
| ConnectDialog | host/port/user/auth method; key picker; jump chain editor | `--radius-lg`, `--space-*` |
| FloatingPanel | drag by header, resize edges, remember geometry, close button | `--shadow-panel`, `--bg-elevated` |
| FileTable | six columns, virtualized, multi-select, sort indicators | `--row-h`, `--font-ui-sm` |
| TransferQueueTable | progress bar, speed, ETA, pause/cancel/retry actions | `--row-h`, `--accent` |
| RwxEditor | 3×3 checkbox grid + octal preview | `--control-h` |
| ContextMenu | keyboard navigable, submenu support, customizable entries | `--bg-elevated`, `--radius-md` |
| TagManagerDialog | create/rename/recolor/delete tags; deleting detaches from hosts | `--bg-panel`, `--tag-*` |
| CommandPalette | fuzzy actions, Ctrl+Shift+P | `--bg-elevated` |
| SettingsCenter | floating dialog; left category nav (General/Credentials/Terminal/Keyboard/Transfer/Plugins/Sync/About) + scrolling pane; 12 built-in themes (each with a full ANSI palette) plus iTerm2/TOML scheme import; terminal pane offers bundled font presets, custom stacks and a live preview; credentials section manages the vault; the Plugins pane has Installed and Market tabs, the market listing coming from the GitHub-hosted index | `--settings-w`, `--settings-h`, `--settings-nav-w` |
| CredentialVault | Settings › Credentials pane: vault mode badge (local vault / master password), lock state, unlock/enable/disable/change/reset actions, auto-lock selector, per-host stored-secret list with delete/clear, recent access log | `--bg-panel`, `--radius-md`, `--danger` |
| KeyManagerDialog | list managed keys (name, algorithm, SHA256 fingerprint, comment, passphrase badge); import from `~/.ssh` scan, a file path, pasted OpenSSH text with live inspection, or generate ed25519/RSA; copy public key; delete | `--dialog-w`, `--font-mono`, `--row-h` |
| KnownHostsDialog | list trusted host keys (host, port, algorithm, fingerprint) with remove action, opened from the credentials section | `--dialog-w`, `--font-mono`, `--danger` |
| UnlockDialog | master-password entry on startup or when a connect needs a locked vault; wrong-password feedback and cancel | `--dialog-w`, `--control-h`, `--danger` |
| TerminalView | xterm.js host; theme mapped from tokens; inline history suggestion with ghost text and prompt-anchored dropdown; OSC 52 clipboard writes; configurable keyword tinting | `--font-terminal`, `--bg-base`, `--suggest-w`, `--tag-*` |
| RecordingsDialog | list of stored `.cast` files (title, date, length, size); read-only replay with play/pause, restart, seek and 1×/2×/4× speed | `--recordings-list-w`, `--player-h`, `--bg-base` |
| SerialDialog | port picker from detected devices with refresh, baud-rate presets, 2×2 grid for data bits / stop bits / parity / flow control; warns when the serial plugin is missing or the app runs outside the desktop shell; Connect stays disabled without a port | `--dialog-w`, `--serial-grid`, `--control-h` |
| HostDialog | protocol tabs (SSH/SFTP/Telnet/Serial/Local) that swap the fields and default port for the chosen protocol; host icon picker grouped into system, network and device badges with a live preview beside the name | `--dialog-w`, `--control-h`, `--radius-md` |
| SftpPanel | tab for SFTP hosts: remote-first file browser with bookmarks, inline editor and permissions dialog; reuses the SSH runtime, has no terminal, and is excluded from broadcast targets and recording | `--row-h`, `--font-ui-sm`, `--bg-panel` |
| MonitorPanel | floating panel with a target selector (local machine or a live SSH/SFTP session) driving CPU, memory, swap and disk meters plus load and uptime; remote probes are read-only | `--bg-elevated`, `--font-ui-sm`, `--accent` |
| WindowChrome / ResizeHandles | frameless Windows/Linux windows draw caption buttons and eight edge/corner resize handles that start a native resize drag; macOS keeps the overlay traffic lights | `--tab-h`, `--bg-elevated`, `--accent` |
| StatusStrip / Toast | connection state, transient errors | `--font-ui-sm` |

## 4. Screen specs and acceptance checklists

Acceptance = perform each path in Ferriterm and in the reference install; record pass/fail in the phase review. Paths describe interactions, not copied artwork.

### 4.1 Home + host sidebar

- [ ] Home shows centered logo, tagline and quick-connect/new-host/new-folder actions.
- [ ] Sidebar toggle in tab bar shows/hides the host sidebar across all views; state persists.
- [ ] Sidebar header shows host count; folders show child host counts; host rows show `user@host`.
- [ ] Typing in sidebar search filters tree and highlights matched substring.
- [ ] Double-click host opens a session tab; Enter on selected node does the same.
- [ ] Right-click host menu contains: Connect, Connect with jump…, Open in new window, Tags ▸ (including Manage tags…), Import…, Properties.
- [ ] Right-click folder menu contains: New host, New folder, Rename, Delete (confirm).
- [ ] Drag host onto folder reparents; drag onto tab bar opens session.
- [ ] Production hosts show a danger-colored dot in tree and tab.

### 4.2 Session view

- [ ] Tab bar shows Home + sessions + new button; tool icon row right-aligned.
- [ ] Drag tab to left/right/top/bottom edge creates split; gutter drag resizes; double-click gutter resets.
- [ ] Ctrl+Shift+P opens command palette; Esc closes.
- [ ] Search overlay finds matches with next/previous and regex toggle.
- [ ] Typing a prefix of a previous command shows ghost completion at the cursor; Right-arrow or Tab accepts it; Escape/other cursor keys dismiss it.
- [ ] Up to five matching commands appear in a dropdown below the prompt; clicking one replaces the current line.
- [ ] Command history is per host when the session belongs to a host, otherwise per session.
- [ ] Recordings dialog lists saved `.cast` files and replays output-only content with play/pause, restart, seek and speed control.
- [ ] Remote programs writing an OSC 52 escape sequence update the local clipboard; the Terminal setting can block this.
- [ ] Keywords configured in settings tint matching output text (case-insensitive) and take effect on open sessions immediately.
- [ ] Closing a session tab prompts only when a process is running (configurable).
- [ ] Floating SFTP/monitor panels open above terminal without hiding tabs.
- [ ] Serial tabs reconnect nothing on launch: layout snapshots keep SSH and local tabs but drop serial ones, since devices are often unplugged.

### 4.3 Connect dialog / properties

- [ ] Auth method switch reveals matching fields (password / key / agent / interactive).
- [ ] Key passphrase and passwords never echoed into logs or config.
- [ ] Unknown host key shows fingerprint dialog with Accept-once / Always trust / Cancel (M2 gate).

### 4.4 Transfer tab

- [ ] Local + remote sub-tabs; each pane independent cwd with breadcrumb path.
- [ ] File table shows six columns; double-click directory navigates; double-click file opens editor.
- [ ] Drag files between panes enqueues transfers; queue shows progress/speed/ETA and pause/cancel/retry.
- [ ] File context menu: Transfer, Edit, Copy path, Rename, Delete, Fast delete (red, confirm), Change permissions, New file/folder.
- [ ] Permissions dialog edits rwx grid and applies on confirm.
- [ ] Editor window saves upload automatically; remote-changed file triggers conflict dialog.

### 4.5 Settings and i18n

- [ ] Settings categories navigable by keyboard; search jumps to matching field.
- [ ] Settings panes are left-aligned: category header, field labels, hints, previews and actions share one left edge.
- [ ] Language switch en ↔ zh-CN applies without restart; missing keys fall back to en.
- [ ] Theme switch updates all tokens live; terminal palette follows.
- [ ] Importing an iTerm2 `.itermcolors` preset or a Ferriterm `.toml` scheme applies the full ANSI palette to open terminals, derives app chrome tokens and persists across restarts; imported schemes can be reapplied or removed.
- [ ] Keyboard focus is visible on buttons, summaries and menu items.
- [ ] Reduce-motion setting disables transitions and entrance animations.

### 4.6 Credentials, keys and host trust

- [ ] Settings › Credentials shows the vault mode and lock state; the encrypted local vault is the default and the system keychain is never used.
- [ ] Enabling a master password migrates stored secrets, requires the password on later launches and after auto-lock; cancelling the unlock dialog leaves the app usable but blocks SSH connects with a clear message.
- [ ] Changing the master password re-encrypts existing entries; disabling it returns to the local vault; resetting wipes secrets, managed keys and their host references after confirmation.
- [ ] Auto-lock selects 0/5/15/30/60 minutes of inactivity and locks without a restart.
- [ ] Stored-secret list shows one row per secret, each deletable; "clear all" asks for confirmation; deleting a host removes its secrets.
- [ ] Settings › Credentials offers a password manager for named password credentials: create, rename, change the secret, add notes, and delete; the stored password is never shown again.
- [ ] Host properties can link a named password credential or a managed key; the connect dialog lists saved passwords and signs in without prompting while the vault is unlocked.
- [ ] A password typed in the connect dialog is written to the vault only after the SSH authentication succeeds; a failed attempt never overwrites the saved secret and the dialog reopens with the error instead of silently retrying.
- [ ] Unlinking or forgetting a password makes the next connect ask again; deleting a host clears its pending retry state and one-shot secrets.
- [ ] Recent access log lists the last credential reads, writes and deletes with timestamps.
- [ ] Key manager imports a private key from a `~/.ssh` candidate or a typed path, rejects non-key files, and shows algorithm plus SHA256 fingerprint.
- [ ] Key manager generates ed25519 and RSA keys with an optional passphrase, shows the public key and copies it to the clipboard.
- [ ] Host properties can select a managed key or keep a key path; a managed key wins when both are set.
- [ ] The connect dialog shows a saved badge instead of pre-filling passwords or passphrases; entering a new secret replaces it after a successful sign-in.
- [ ] SSH connects read secrets only in the Rust process; the webview never receives stored plaintext.
- [ ] Authentication falls back key → agent → password when the previous method is rejected, without exceeding the server's failure limit.
- [ ] Jump-host hops resolve their own saved password, passphrase or managed key.
- [ ] Known-hosts dialog lists entries with fingerprints and removes a selected entry; the next connect prompts for that host again.
- [ ] Backup and remote sync include secrets only when the "include credentials" option is on; importing restores them into the vault.

### 4.7 Serial sessions and the plugin marketplace

- [ ] New Serial… appears in the tab-bar new-tab menu and the command palette; it stays disabled outside the desktop shell.
- [ ] The serial dialog lists detected ports with a refresh action, accepts a typed device path, and offers baud, data-bit, stop-bit, parity and flow-control choices.
- [ ] Connecting opens a normal terminal tab titled `<port> · <baud>`; keystrokes reach the device and device output renders in the tab.
- [ ] Nothing is written to the serial port until the Serial Ports plugin (kind `serial`, permission `serial`) is installed; without it the dialog and backend both explain why.
- [ ] Settings › Plugins › Market loads the index from the GitHub repository, shows name, version, author and description, and marks already-installed entries.
- [ ] Install fetches the manifest, validates it, writes it to the plugin directory and refreshes the installed list; uninstall removes it after confirmation.
- [ ] Marketplace fetches accept only https URLs, reject loopback hosts, cap the response size and validate every field before installing.
- [ ] With the serial plugin installed, its Installed row offers Connect… which opens the serial dialog.

### 4.8 Multi-protocol hosts, icons, SFTP tab, remote monitor and frameless resize

- [ ] New host offers five protocol tabs (SSH, SFTP, Telnet, Serial, Local); switching protocol swaps the fields, and the port follows the protocol default unless it was typed by the user.
- [ ] Telnet hosts show host and port only; SFTP hosts keep the SSH credential fields beside the address.
- [ ] Serial hosts take a device path with the serial port picker plus baud, data-bit, stop-bit, parity and flow-control choices; Local hosts take an optional shell and working directory.
- [ ] The icon picker groups badges into system (Ubuntu, Debian, Armbian, Raspberry Pi OS, CentOS, Red Hat, Fedora, Rocky, Alma, openSUSE, Arch, Alpine, Linux, macOS, Windows, FreeBSD), network (Huawei, H3C, Cisco, Ruijie, ZTE, TP-Link, MikroTik, Fortinet, Juniper) and device (router, switch, firewall, access point, server, NAS, printer).
- [ ] The chosen icon previews beside the host name field, then renders in the host tree and the session tab; hosts without an icon fall back to the protocol badge (SSH/local terminal, SFTP files, serial port).
- [ ] Double-clicking a Telnet host opens a terminal tab that carries keystrokes through the telnet transport.
- [ ] Double-clicking an SFTP host opens a file-browser tab (no terminal) with remote panes, bookmarks, the inline editor and the permissions dialog; closing the tab releases the SSH session.
- [ ] SFTP tabs never appear in broadcast targets and do not show the recording control.
- [ ] The monitor panel target selector lists the local machine and every live SSH/SFTP session; switching targets reloads within two seconds and dropping a session returns the selector to the local machine.
- [ ] Remote monitoring runs the fixed read-only probe over the session's exec channel with a 6 s timeout and a 64 KiB output cap; CPU usage comes from `/proc/stat` deltas between polls, and a host without `/proc` shows an explicit unsupported message instead of a zeroed panel.
- [ ] Disconnecting a session forgets its cached CPU counters so a reconnect starts a fresh delta.
- [ ] Frameless Windows and Linux windows expose resize handles on all four edges and four corners; dragging one starts a native resize, and macOS keeps its overlay traffic lights.

### 4.9 Window chrome, sidebar width, icon set, settings layout and transport plugins

- [ ] A vertical handle between the host sidebar and the content resizes the sidebar between 220 px and 460 px; dragging follows the pointer, arrow keys move it by 16 px, and the width persists across restarts.
- [ ] On macOS, pressing the frameless header background starts a native window drag; presses that land on tabs, buttons, inputs, menus or other interactive controls never start a drag.
- [ ] Double-clicking empty header space zooms the window; the host sidebar and tools sidebar title bars drag the window the same way while their controls keep their own clicks.
- [ ] Functional buttons use the shared stroke icon set (close, record/stop, pause, reconnect, refresh, download/upload, check, copy, trash, theme, split, search and similar); text glyphs are no longer used as button labels, and icon + label toolbars centre the glyph against the text.
- [ ] Telnet is an optional plugin (kind `telnet`, permission `telnet`); Settings › Plugins › Installed lists the built-in Serial Ports and Telnet entries with Enable/Uninstall, and enabling one writes `<id>.json` into the plugin folder.
- [ ] Without the telnet plugin, the Telnet host tab explains how to enable it and `telnet_open` is rejected; with it, Telnet hosts connect normally.
- [ ] Settings panes open with the category title and a one-line description; legend, hint, action and message blocks share the left content edge instead of being centred or indented, while field hints stay in the control column.

## 5. Phase maintenance rule

Each phase (P1…) updates this file before implementation: add/adjust screen checklist items, then implement, then sign the checklist in the phase review. Token additions require a table row here first.

## 6. Copy & terminology conventions (文案与术语规范)

All user-visible strings live in `src/i18n/{en,zh-CN}.json`; components must not
hardcode visible copy. This section is the ruling source for wording, alongside
the token table in §2. Both languages are first-class: every value change
updates both files, and leaf keys stay parity-checked (no orphan keys, no silent
fallback to the other language).

### 6.1 Term glossary (术语对照)

Recurring concepts use **one term per concept, in both languages**, with no drift
between modules. When an existing string violates the table, the table wins and
the string is migrated.

| Concept | zh-CN | en | Notes |
|---|---|---|---|
| Saved host profile | 主机 | Host | The stored entity. Not 连接. |
| The act of connecting | 连接 | Connect | Verb/state, not the stored profile. |
| Live terminal/SFTP tab | 会话 | Session | An open tab. |
| Permanently remove an item | 删除 | Delete | Destructive; item is gone. |
| Detach from a list (item survives) | 移除 | Remove | From queue, detach a tag, etc. |
| Wipe a single value/state (container kept) | 清除 | Clear | A field, "clear finished", etc. |
| Empty a container/collection | 清空 | Clear all / Empty | History, cache, all credentials. |
| Stop remembering a saved secret | 忘记 | Forget | Credential-specific; not 清除. |
| Restore to default (non-destructive) | 恢复默认 | Reset / Restore defaults | Settings only; never deletes data. |
| Establish a session | 连接 | Connect | Connect-dialog action. |
| Begin a continuous service | 开始 | Start | Forwarding, broadcast, sync, monitor. |
| Run a one-shot program/command/script | 运行 | Run | Plugin run, run a chosen command. |
| Execute a gated / AI-suggested op | 执行 | Execute | Policy-gated or AI command. |
| Stop a continuous service | 停止 | Stop | Distinct from 取消 (cancel an op). |
| Acknowledge / generic OK | 确定 | OK | Button only; no punctuation. |
| Confirm an action (title/verb) | 确认 | Confirm | Confirm-dialog titles. |
| Dismiss / abort | 取消 | Cancel | |
| Create a new entity (host/folder) | 新建 | New | |
| Add to an existing list (tag/rule/pw) | 添加 | Add | |
| Persist changes | 保存 | Save | |

### 6.2 Titles (标题)

- Dialog / panel / group titles are noun phrases or gerunds. **No trailing period.**
- Confirmation titles may end with `？` (zh) / `?` (en) when they ask a question:
  `关闭会话？` / `Close session?`.
- One concept per title; avoid full sentences.

### 6.3 Hints / descriptions (说明文字)

- One short sentence. en ends with a period; zh ends with `。` when it is a full
  sentence, omitted when a fragment (e.g. a placeholder-style hint).
- Describe the **effect**, not the mechanism, where possible.
- Field hints stay in the control column; legend / hint / action / message blocks
  share the left content edge (see §4.9).

### 6.4 Button labels (按钮标签)

- Verb-first, **no trailing punctuation**: `保存`, `连接`, `新建主机`, `开始转发`.
- A destructive confirm names the verb (`删除`) and pairs it with `取消`; do not
  use a bare `确定` for destructive actions.
- Icon + label buttons centre the glyph against the text; the label is the verb.

### 6.5 Selects / options (选择框与选项)

- `<select>` (`.ui-select`) and the custom combobox (`ModelCombo`) are **one
  control family**: same height, same border/radius tokens, same focus ring.
- Option labels are short nouns/phrases, no period.
- Placeholders are example-style hints (`192.168.1.10`), not sentences.

### 6.6 Icons (图标)

- Functional buttons use the shared stroke set in `icons.tsx`; never use raw
  text glyphs as button labels.
- Icon + label toolbars centre the glyph against the text.