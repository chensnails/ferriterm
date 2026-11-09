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
| `--font-mono` | `"JetBrains Mono", "Cascadia Code", Menlo, Consolas, monospace` | terminal, paths |
| `--space-1..5` | `4, 8, 12, 16, 24px` | spacing scale |
| `--radius-sm/md/lg` | `4, 6, 10px` | controls / cards / dialogs |
| `--control-h` | `28px` | inputs |
| `--button-h` | `32px` | buttons |
| `--row-h` | `32px` | tree/table rows |
| `--tab-h` | `36px` | tab bar |
| `--tab-max-w` | `220px` | maximum width of a session tab |
| `--home-w` | `720px` | Home content column max width |
| `--font-logo` | `28px` | Home logo mark |
| `--sidebar-w` | `300px` | host sidebar + settings category pane |
| `--dialog-w` | `440px` | dialog width cap |
| `--settings-w` | `760px` | settings center width |
| `--settings-h` | `540px` | settings center pane height |
| `--settings-nav-w` | `156px` | settings category nav width |
| `--split-gutter` | `4px` | split drag handle |
| `--focus-width` / `--focus-offset` | `2px` / `2px` | keyboard focus ring |
| `--scrollbar-w` / `--scrollbar-pad` | `10px` / `2px` | thin scrollbar geometry |
| `--backdrop-blur` | `2px` | modal backdrop blur |
| `--shadow-panel` | `0 8px 24px rgb(0 0 0 / .35)` | floating panels |
| `--motion-fast` | `120ms ease-out` | hover/open transitions; disabled by reduce-motion setting |
| `--motion-panel` | `160ms ease-out` | dialog entrance; disabled by reduce-motion setting |

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
| SettingsCenter | floating dialog; left category nav (General/Terminal/Keyboard/Transfer/Plugins/Sync/About) + scrolling pane | `--settings-w`, `--settings-h`, `--settings-nav-w` |
| TerminalView | xterm.js host; theme mapped from tokens | `--font-terminal`, `--bg-base` |
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
- [ ] Closing a session tab prompts only when a process is running (configurable).
- [ ] Floating SFTP/monitor panels open above terminal without hiding tabs.

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
- [ ] Language switch en ↔ zh-CN applies without restart; missing keys fall back to en.
- [ ] Theme switch updates all tokens live; terminal palette follows.
- [ ] Keyboard focus is visible on buttons, summaries and menu items.
- [ ] Reduce-motion setting disables transitions and entrance animations.

## 5. Phase maintenance rule

Each phase (P1…) updates this file before implementation: add/adjust screen checklist items, then implement, then sign the checklist in the phase review. Token additions require a table row here first.