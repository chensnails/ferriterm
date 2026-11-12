# Ferriterm 开发计划：Termora × ShellAX 双项目分析与自研路线

- 状态：草案 v1（2026-11-08）
- 参考基线：Termora `TermoraDev/termora` 分支 `2.x`；ShellAX `daxiangme/ShellAX` 分支 `master`（Cargo 版本 1.3.5）
- 本仓库现状：Rust + Tauri 2 + xterm.js 脚手架已验证可编译运行（`cargo check`、`vite build`、`tauri dev` 冒烟通过）
- 总原则：**UI 与核心交互优先对齐 Termora；工程实践与进阶能力选择性吸收 ShellAX；全部代码净室自研**

---

## 1. 背景与目标

自研一款跨平台（macOS / Windows）SSH 终端客户端，代号 Ferriterm。
**平台决策（2026-11-12）**：暂停 Linux 版本支持——不再构建/发布 Linux 安装包，
但 Linux 相关代码与 CI 任务全部保留（防止代码腐烂，恢复时无需重写）；
详见 `docs/backlog.md` 第 0 节。
产品定位：以 Termora 的界面风格与交互流程为基准（主机树 + 一体化标签 + 浮动面板 + 双栏传输），
吸收 ShellAX 在终端进阶能力、安全治理与工程体系上的优秀设计，形成自有产品。

目标分解：

1. **UI/交互对齐 Termora**：布局、组件形态、菜单结构、操作路径逐屏对照验收。
2. **核心功能对齐 Termora**：终端/SSH/主机管理/SFTP 传输/系统监控/快捷指令/插件与同步。
3. **差异化吸收 ShellAX**：命令建议与历史、会话录制、广播确认、布局恢复、目录同步、
   凭据钥匙串、审计与权限引擎（AI 阶段）、WASM 沙箱扩展、回环测试环境与文档驱动流程。
4. **语言策略**：开发以英文为基础，软件提供中英双语，其他语言预留语言包插件接口（见 6.4）。
5. **不复制**两个参考项目的任何代码、图标、文案（两者均为 AGPL-3.0，见第 5 章）。

---

## 2. Termora 分析

### 2.1 技术架构（已核实，2.x 构建脚本）

| 层 | 实现 |
|---|---|
| 语言/运行时 | Kotlin/JVM，推荐 JetBrainsRuntime |
| UI | Swing + FlatLaf 主题（**并非** Compose；KMP/Compose 是官方未来方向） |
| 终端模拟 | 自研，部分实现 XTerm 控制序列 |
| 本地终端 | JetBrains pty4j |
| SSH/SFTP | Apache MINA SSHD（sshd-core） |
| 存储 | Exposed ORM + SQLite，exposed-crypt 加密 |
| 同步/扩展 | JGit（Gist）、OkHttp（WebDAV）、插件目录机制 |
| 打包 | macOS 签名+公证、Windows Appx/商店、Linux deb |
| 许可 | AGPL-3.0 或付费商业许可（双许可） |

### 2.2 优点

**架构**
- 模块边界清晰：终端核心 / SSH 传输 / SFTP / 主机存储 / 插件各自独立，云存储（S3/OSS/COS/OBS/SMB/WebDAV）全部插件化，主程序保持精简。
- 存储用 SQLite + ORM + 字段加密，主机/凭据/配置统一持久化，方案成熟。
- 插件机制经受了真实生态检验（Geo、Sync、Editor、多种对象存储）。

**功能**
- SSH 全功能：X11 转发、SSH-Agent、端口转发、跳板。
- SFTP 产品化程度高：A↔B 互传、递归、6 并发队列、独立编辑器窗口保存自动上传、可视化权限编辑、`rm -rf` 快删。
- 主机管理：文件夹树 + 彩色标签 + 从其它工具导入。
- 系统信息面板与 Nvidia GPU 监控贴合运维场景。

**交互（官方截图核实）**
- 顶栏一体化标签：Home 标签 + 会话标签 + 新建按钮；右侧一排工具图标（分屏/SFTP/监控/快捷指令/转发/搜索/设置）。
- Home 页 = 居中 logo + 搜索框 + 主机树；右键菜单完备（Connect with 跳板、新窗口打开、Tags 子菜单含 Manage tags、Import、Properties 等）。
- **浮动面板**设计：系统信息、SFTP 以可弹出窗口浮在终端之上，不打断主视图。
- Transfer 独立标签：Local + N 远程子标签、双 pane 六列文件表、底部传输队列表（进度/速度/ETA/源/目标）。
- 文件右键含 Transfer/Edit/Copy Path/Rename/Delete/rm -rf(警示)/Change Permissions/New，操作路径短。

### 2.3 局限

- JVM + 捆绑运行时：安装包与内存占用大；Swing 的 UI 上限与 HiDPI/动画表现一般。
- 终端协议自研且"部分实现"，图形协议（Sixel/Kitty）、录制等进阶能力缺失。
- 无命令建议/历史智能、无会话录制、无布局恢复、无目录同步。
- 安全治理（审计、权限引擎、AI 管控）空白。

---

## 3. ShellAX 分析

### 3.1 技术架构（已核实，Cargo.toml 与源码树）

| 层 | 实现 |
|---|---|
| 语言/UI | Rust + GPUI（`gpui-pre` 0.3.9，Zed 同源 GPU 渲染框架）+ gpui-component |
| 终端核心 | **alacritty_terminal 0.26** + portable-pty 0.9 |
| SSH | **russh 0.64**（aws-lc-rs 后端）+ tokio |
| 形态 | 单主 crate（src/ 约 145 个 .rs）+ 独立 extensions _workspace_（WASM） |
| companion | `shellax-cli` 二进制 + 本地策略服务/daemon（GUI 与策略引擎分离） |
| 凭据 | 系统钥匙串优先，不可用时退回加密文件，不写配置文件 |
| 审计 | 追加式 JSONL + SHA-256 哈希链；组织托管策略 yaml 强制覆盖 |
| 扩展 | WASM 沙箱：无导入、限指令/内存、每次调用新建实例 |
| 工程 | docs/PRD.md、DEVELOPMENT.md、EXTENSIONS.md、UI_REDESIGN_PLAN.md；CI/release/ext-release 三条流水线；testenv 回环 sshd 测试 harness；THIRD_PARTY_NOTICES.md |
| 平台/许可 | Win x64、Mac Apple Silicon（Linux 仅源码）；AGPL-3.0；安装包未签名/未公证 |

### 3.2 优点

**架构与工程**
- 与本项目脚手架同选 russh + portable-pty，**交叉验证了我们 SSH/PTY 选型的可行性**。
- GUI / CLI / 策略服务三分离：外部 Agent 经 CLI 钩子过权限引擎，凭据永不出主程序——安全架构范本。
- WASM 沙箱扩展：能力与主程序隔离，扩展独立 workspace 独立发布流水线。
- 文档驱动：PRD 先行；UI 规格书带"实测问题清单 + 验收章节"（其 UI_REDESIGN_PLAN 审计出 453 处字号硬编码等设计债，反面教训：**design token 必须第一天强制执行**）。
- 回环 sshd 测试环境（一次性密钥 + 127.0.0.1:2222）使 SSH 功能可自动化回归。
- 更新机制：通道选择 + SHA256 校验 + 分平台安装策略。

**功能**
- 终端进阶：正则搜索、矩形选择、OSC 52、关键字高亮、**Sixel/Kitty 图形协议**、**asciicast v2 录制回放（默认脱敏）**、**广播输入（含生产主机需确认）**、布局快照/回滚/启动恢复。
- 命令体验：fish 式灰色历史建议（→/Tab 接受）、提示符历史下拉、按主机历史、读取远端 shell 历史、敏感命令过滤规则可自定义。
- SSH：keyboard-interactive(2FA)、known_hosts 校验、ProxyJump、HTTP CONNECT/SOCKS5 代理、连接复用、断线重连、可选远端 tmux 保会话。
- 主机：`a/b/c` 嵌套分组、**生产主机标记**、`~/.ssh/config` 导入、`ssh://` 链接、图标自定义。
- SFTP：断点续传、编辑器**冲突检测**、**目录同步（单/双向、差异预览、删除确认）**。
- 端口转发：-L/-R/-D、保存项随连接自动启动、实时连接数与流量。
- 广度的运维模块：数据库（含达梦/MongoDB/MCP）、Redis、Docker、Homebrew、44 个小工具。
- AI 治理：多服务商、上下文范围可选、发送前脱敏、**权限引擎三模式 + 风险规则**、外部 Agent 状态识别与审批超时、哈希链审计、组织托管策略。

**交互**
- 左活动栏模块切换 + 各模块独立标签 + 记住最近标签；右栏六卡片（主机工具/AI/常用命令/历史/目录/MCP）；底部工具窗口（传输队列 + **执行日志**）。
- 命令面板（Ctrl+Shift+P）、右键菜单与快捷键可自定义、`.itermcolors` 配色导入、减少动态效果选项。
- 常用命令 `{{变量}}` 填参后按焦点插入到终端/SQL/Redis 对应位置——跨模块统一命令库的设计优秀。

### 3.3 局限

- GPUI 为预发布 pin 版本（`=0.3.9`），生态与文档风险高；平台覆盖窄（无 Linux 包、无 Intel Mac）。
- 单 crate 145 文件的 monolith，模块边界靠目录约定，重构成本高（其 UI 审计文档即设计债证据）。
- 功能广度大但项目年轻（2026-10 创建），深度与稳定性待验证；安装包未签名/公证。
- 数据库/Redis/Docker 等模块与"SSH 终端"核心耦合在同一窗口，学习成本高。

---

## 4. 对比与取舍矩阵

| 维度 | Termora | ShellAX | Ferriterm 取舍 |
|---|---|---|---|
| UI 基准 | 扁平深色、一体化标签、浮动面板 | Island 骨架、活动栏+卡片 | **Termora 布局/交互为基准**；吸收命令面板、底部执行日志、右键/快捷键可自定义 |
| 终端核心 | 自研 XTerm（部分） | alacritty_terminal | xterm.js（VSCode 验证）；图形协议/录制列进阶期 |
| SSH 库 | MINA SSHD | russh 0.64 | **russh 0.65**（脚手架已验证） |
| 本地 PTY | pty4j | portable-pty 0.9 | **portable-pty 0.9**（已验证） |
| 存储 | SQLite+ORM+加密 | TOML 配置+钥匙串 | SQLite（结构化主机/队列/历史）+ **本地加密保险库（不调用系统钥匙串）** |
| 扩展机制 | JVM 插件目录 | WASM 沙箱 | 内置模块 + manifest；外部扩展走 **WASM 沙箱** |
| 安全治理 | 空白 | 权限引擎/审计/托管策略 | 主机密钥 TOFU 必做；审计与权限引擎随 AI 阶段引入 |
| 测试/工程 | 常规 | 回环 sshd + 三流水线 + 文档驱动 | **全套吸收**（testenv、CI、PRD/UI 规格、token 强制） |
| 运维广度模块 | 无 | DB/Redis/Docker/brew/工具 | **核心期不做**；列 M4 可选扩展，避免范围失控 |
| 国际化 | 多语言（README 含 en/de/pt/zh） | 中英双语 | **en 源语言 + zh 内置**；其他语言走语言包插件接口（6.4） |
| 许可 | AGPL 双许可 | AGPL | 净室自研；自有许可待定（建议 MIT/Apache 或闭源，均不抄代码） |

---

## 5. 合规与净室边界

两个参考项目均为 **AGPL-3.0**（Termora 另售商业许可）。本计划边界：

- **允许**：功能清单、交互流程、布局结构、设计思想的分析与独立实现（思想/功能不受著作权保护）。
- **禁止**：复制源码、反编译产物、图标/截图资产、文案/翻译字符串进入本仓库；UI 对照仅用于验收比对。
- 图标全部重绘；ASCII logo 原创；主题色板自行测量定义 token，不搬运其主题文件。
- 依赖库（russh、portable-pty、xterm.js 等）按其各自许可正常引用，维护 `THIRD_PARTY_NOTICES.md`（学习 ShellAX）。
- 若未来改变路线直接 fork 任一项目：须全项目遵守 AGPL 或取得商业许可，并另行评审。

---

## 6. 需求范围

### 6.1 核心范围（Termora 对齐，M1–M3 交付）

1. 终端：多标签、横竖分屏、搜索、复制粘贴、字体/配色/scrollback、本地终端、会话日志。
2. SSH：密码/公钥/agent/keyboard-interactive、known_hosts TOFU 弹窗、跳板链、端口转发(-L/-R/-D)、X11（实验性）、keepalive 重连。
3. 主机：文件夹树、彩色标签、搜索、右键全菜单、属性对话框、导入（Xshell/FinalShell 优先，`~/.ssh/config` 吸收自 ShellAX）、新窗口打开。
4. SFTP：浮动面板 + Transfer 标签（Local+N 远程）、六列文件表、书签/隐藏文件、全套文件右键、rwx 可视化编辑、独立编辑器窗口+保存自动上传+冲突检测（吸收 ShellAX）。
5. 传输：6 并发队列、递归、A↔B（客户端中继并明示）、进度/速度/ETA、暂停/取消/重试、断点续传（吸收 ShellAX）。
6. 监控：系统信息浮动面板（CPU/Mem/Swap/load/uptime/文件系统）、Nvidia GPU。
7. 效率：快捷指令/常用命令库（分组 + `{{变量}}` 填参 + 按焦点插入，吸收 ShellAX）、历史命令卡片。
8. 插件与同步：内置插件模块 + manifest；Sync 加密包 → Gist/WebDAV；存储 pane 扩展（WebDAV/S3 优先）。
9. 设置/主题/i18n：设置中心、Termora 风格多主题；英文为基础、内置 en/zh 双语，其他语言经语言包插件接口（见 6.4）。
10. 发布：三平台打包、mac 签名公证、Win 签名、自动更新（SHA256 校验，吸收 ShellAX）。

### 6.2 吸收范围（ShellAX 差异化，穿插各阶段）

- 命令建议与历史下拉、敏感命令过滤（P2/P5）。
- asciicast v2 录制回放（默认脱敏）、广播输入生产确认、布局快照/启动恢复（P2/P5）。
- 目录同步（差异预览+删除确认）（P4）。
- 本地加密保险库（不调用系统钥匙串）、生产主机标记（P1/P3）。
- 命令面板、底部执行日志、右键/快捷键可自定义（P2/P7）。
- 工程体系：testenv 回环 sshd、CI 三流水线、PRD/UI 规格文档、design token 强制、THIRD_PARTY_NOTICES（P0 起贯穿）。

### 6.3 明确不做（本期 out of scope）

- 数据库/Redis/Docker/Homebrew 模块、44 个小工具（列 M4 可选，独立评审）。
- AI 助手与 Agent 管控完整体系（权限引擎/审计/托管策略/MCP）——列 M4；但**审计日志接口与权限判定接口在 P0 预留抽象**。
- Sixel/Kitty 图形协议（xterm.js  addon 生态成熟后再评估，列 M4）。
- Android/iOS（Tauri 2 mobile 保留可能性，不承诺）。

### 6.4 语言策略（英文为基础 · 双语 · 语言包插件化）

- **开发以英文为基础**：代码标识符、注释、提交信息、PRD/UI 规格以英文为基础语言；全部 UI 文案走 i18n 层，`en` 为源语言（source locale），禁止硬编码文案字面量（CI lint）。本计划为中文讨论稿，后续正式规格按英文基础约定执行。
- **双语内置**：v1.0 默认随包提供 `en` + `zh-CN`，跟随系统语言并可在设置中手动切换；zh 与 en 同仓维护，缺译回退 en。
- **语言包插件接口**：其他语言不进主仓库，以可下载语言包（纯数据扩展包）交付，复用 plugin-host 的 manifest 注册/下载/启停机制：
  - 包结构：`manifest.json`（id/version/locale/name/sha256）+ `locales/<code>.json`（ICU MessageFormat 字符串）；
  - 纯数据包不执行代码，校验和验证后加载；与后期 WASM 扩展共用同一套分发与扩展管理 UI；
  - 节奏：P0 建 i18n 骨架与 key 命名规范 → P6 plugin-host 预留语言包注册接口 → P7 完成语言切换与语言包管理 UI。
- 缺失 key 回退 en 并记录，可输出翻译完成度报告，便于社区贡献语言包。

---

## 7. 功能模块划分

| 模块 | 职责 | 后端 crate/目录 | 前端目录 | 优先级 |
|---|---|---|---|---|
| terminal-core | 会话生命周期、输入输出、resize、录制接口 | `src-tauri/src/session.rs` 演进为 crate `ft-core` | `src/terminal/` | P0 |
| ssh-service | 认证/跳板/转发/agent/X11/重连 | `ft-ssh` | 连接对话框 | P0 |
| pty-service | 本地 shell | `ft-pty` | 同上 | P0 |
| host-store | 主机树/标签/凭据引用/导入器/SQLite | `ft-store` | `src/hosts/` | P0 |
| sftp-vfs | VFS trait（local/sftp/云存储）、文件操作、编辑器后端 | `ft-vfs` | `src/transfer/` | P0 |
| transfer-queue | 并发调度、断点、中继、进度事件 | `ft-transfer` | 队列表 | P0 |
| monitor | /proc 采样、nvidia-smi 解析 | `ft-monitor` | 浮动面板 | P1 |
| commands | 常用命令/历史/建议/过滤 | `ft-store` + `ft-core` | 右栏卡片 | P1 |
| plugin-host | manifest 注册、语言包（纯数据，首个包类型）、内置插件、WASM 沙箱（后期） | `ft-plugin` | 插件管理 UI | P1 |
| sync | 加密打包、Gist/WebDAV 传输 | `ft-sync` | 设置页 | P1 |
| ui-shell | 标签栏/工具栏/浮动面板/菜单/命令面板/token | — | `src/shell/` | P0 |
| security | TOFU、本地加密保险库、审计接口、权限接口（预留） | `ft-security` | 指纹/确认弹窗 | P0 |
| settings-i18n | 设置中心、主题、i18n（en 源语言 + zh 内置）、语言包加载 | `ft-store` | `src/settings/` | P1 |
| release-eng | 打包/签名/更新/CI/testenv | `tools/`、`.github/` | — | P0 |

模块纪律（吸取 ShellAX monolith 教训）：后端按上表拆 workspace crate，禁止跨 crate 直接引用内部类型；前端按目录 + 公共组件层，**所有尺寸/颜色走 token**。

---

## 8. 技术选型

| 层 | 选择 | 理由 | 备选/否决 |
|---|---|---|---|
| 壳 | Tauri 2 | 三平台含 Linux/Intel Mac；安装包小；脚手架已验证 | GPUI（ShellAX 路线）：预发布 pin、平台窄、生态风险 → 否决 |
| 前端 | React 19 + TypeScript + Zustand | 树/表/菜单/浮动面板复杂度需要组件生态 | vanilla TS（现脚手架）撑不住 → 升级；Solid 备选 |
| 终端渲染 | xterm.js + fit/web-links/search addon | VSCode 同源、协议覆盖最全 | alacritty_terminal+自绘（ShellAX 路线）与 Tauri WebView 架构不匹配 → 否决 |
| 表格/树 | TanStack Table + 自研树组件 | 六列文件表/队列表/主机树虚拟化 | 手写 → 否决 |
| 样式 | CSS custom properties design token + 扁平深色基准 | Termora 风格；token 第一天强制（ShellAX 设计债教训） | Tailwind 可选但不引入原子类泛滥 |
| i18n | i18next + ICU MessageFormat；en 源语言、zh 内置；语言包经 plugin-host manifest | 英文基础双语；包接口与扩展统一（6.4） | 自研字符串字典（无复数/上下文）→ 否决；Fluent 备选 |
| SSH | russh 0.65（ring 后端） | 已验证；ShellAX 同库交叉验证 | MINA SSHD 需 JVM → 否决 |
| PTY | portable-pty 0.9 | 已验证；ShellAX 同库 | — |
| 存储 | rusqlite + migration；ring/ChaCha20-Poly1305 本地加密保险库（不调用系统钥匙串） | Termora 的 SQLite 经验 + ShellAX 的凭据经验 | Exposed 需 JVM → 否决 |
| 异步 | tokio（sync/rt/time） | 与 russh 一致 | — |
| 扩展 | 内置 trait 对象 + manifest；外部 WASM（wasmtime，沙箱参数学 ShellAX） | 先简后繁 | JVM 插件 → 否决 |
| 日志/审计 | tracing；审计 JSONL+哈希链接口预留 | ShellAX 范本 | — |
| 测试 | testenv 回环 sshd + cargo test；前端 vitest + Playwright（WebView 冒烟） | ShellAX 范本 | — |
| CI/发布 | GitHub Actions：ci / release / ext-release；tauri updater + SHA256 | ShellAX 范本 + Tauri 生态 | — |

---

## 9. 阶段划分与里程碑

估时为单人全栈人日；含 15% UI 对照与返工缓冲。

| 阶段 | 内容 | 验收 | 估时 |
|---|---|---|---|
| **P0 工程基准** | 参考机安装与录屏归档；PRD + UI 规格书（token 表/组件清单/验收章节，学 ShellAX 文档格式）；React 骨架替换 vanilla；workspace crate 拆分；SQLite schema；i18n 骨架（en 源语言 + key 规范）；testenv 回环 sshd；CI ci.yml；THIRD_PARTY_NOTICES | 规格书评审；testenv 内 SSH 登录自动化通过 | 4–5 |
| **P1 Home 与主机** | 主机树/标签/搜索/右键全菜单/属性框；导入器(Xshell/FinalShell/ssh_config)；本地加密保险库；生产标记；多窗口 | 与 Termora Home 逐菜单比对 | 6–8 |
| **P2 终端主视图** | 一体化标签+工具栏；分屏拖拽；搜索/快捷键矩阵；命令面板；命令建议+历史下拉+敏感过滤；布局快照/恢复；会话日志；录制接口落地(asciicast v2 脱敏) | 分屏/搜索/建议/恢复矩阵比对；testenv 回归绿 | 8–10 |
| **P3 SSH 补齐** | 公钥/agent/2FA；TOFU 弹窗；跳板链；-L/-R/-D + 自动启动 + 流量统计；X11 实验性；重连；tmux 保会话可选 | 真机+testenv 认证/转发矩阵全通 | 7–9 |
| **P4 SFTP 与传输** | 浮动面板/Transfer 多 pane；六列表+右键+rwx 编辑；编辑器窗口+自动上传+冲突检测；队列 6 并发/递归/中继/断点/暂停重试；目录同步(预览+删除确认) | 1GB 队列行为比对；同步差异预览正确 | 10–12 |
| **P5 监控与效率** | 系统信息面板/GPU；常用命令库({{变量}}/按焦点插入)+历史卡片；广播输入生产确认；底部执行日志 | 面板数据比对 htop/nvidia-smi；广播确认路径 | 5–6 |
| **P6 插件与同步** | plugin-host + manifest；Sync(Gist/WebDAV)；WebDAV/S3 pane；WASM 沙箱外部扩展骨架 | 同步往返；沙箱逃逸测试用例通过 | 6–8 |
| **P7 设置与发布** | 设置中心/多主题；en/zh 双语切换与语言包管理 UI（接口 P6 预留）；右键与快捷键自定义；macOS/Windows 签名打包（Linux 暂停，代码保留）；自动更新 SHA256；brew/winget | 双平台干净安装启动；en/zh 切换与缺译回退正确；更新校验失败拒绝安装 | 5–6 |
| **P8 可选扩展(M4)** | AI 助手+权限引擎+哈希链审计+托管策略；DB/Redis/Docker 模块评审立项；Sixel/Kitty 评估 | 独立 PRD 评审后另排 | 另估 |

**里程碑**
- **M1 = P0–P2**：可日常使用的多标签终端 + 主机管理（内部 dogfood 起点）。
- **M2 = P3–P4**：SSH/SFTP 核心功能对齐 Termora（对外 beta）。
- **M3 = P5–P7**：完全对齐 + macOS/Windows 发布（v1.0）；Linux 已暂停发布，代码与 CI 保留。
- **M4 = P8**：差异化扩展（v1.x，独立评审）。

**合计 M1–M3 约 51–64 人日**（单人 2.5–3.5 个月；双人并行可压至 ~2 个月，关键路径 P4）。

---

## 10. 风险与应对

| 风险 | 等级 | 应对 |
|---|---|---|
| AGPL 传染/资产混入 | 高 | 净室纪律（第 5 章）；CI 加资产来源检查；PR 模板声明参考方式 |
| 范围被 ShellAX 广度带偏 | 高 | 6.3 明确不做清单；M4 独立评审；每阶段验收只对照 Termora 基准 |
| UI 像素级对照成本 | 中 | 15% 缓冲；P0 产出 token 表后禁止硬编码（lint 规则） |
| russh 能力缺口（X11/agent 平台差异） | 中 | X11 标实验性；Windows agent 走 OpenSSH pipe；必要时上游 PR |
| A↔B 中继带宽翻倍 | 中 | UI 明示中继；队列限速选项 |
| GPUI 路线诱惑/技术摇摆 | 中 | 第 8 章已否决并记录理由；变更需 RFC |
| 凭据安全 | 高 | 本地加密保险库（不调用系统钥匙串）；配置文件零明文；testenv 覆盖凭据路径 |
| 主机密钥 MITM | 高 | P3 TOFU 必做指纹弹窗；known_hosts 持久化 |
| 浮动面板/拖拽前端复杂度 | 中 | P0 组件库先行；TanStack + 自研 dock 组件；Playwright 冒烟 |
| 单人带宽 | 中 | 里程碑 dogfood 切分；M4 不承诺日期 |
| 录制/历史泄密 | 中 | 默认脱敏（学 ShellAX）：无回显输入不录、敏感命令过滤 |

---

## 11. 工程约定

1. **文档驱动**：每阶段先更新 `docs/` 下 PRD 增量与 UI 规格（含验收章节），再写代码（ShellAX 范本）。
2. **测试**：testenv 回环 sshd 为 SSH/SFTP 回归基座；前端 vitest 单测 + Playwright WebView 冒烟；CI 全绿才可合并。
3. **Design token**：颜色/字号/间距/圆角/行高全部 token；CI lint 禁止字面量（ShellAX 453 处硬编码的反面教训）。
4. **模块边界**：后端 workspace crate 化；前端公共组件层；跨模块只走事件/接口。
5. **许可 hygiene**：`THIRD_PARTY_NOTICES.md` 随依赖变更更新；图标/文案原创声明入 PR 模板。
6. **提交/版本**：Conventional Commits；semver；release 流水线产出 SHA256 清单。
7. **安全预留**：P0 定义审计与权限判定 trait（即使 M4 才实现），避免后期重构。
8. **英文基础与 i18n**：代码/注释/提交信息/正式规格以英文为基础；UI 文案全部走 i18n key，en 为源语言、zh 随包内置、其他语言走语言包插件接口；CI lint 检查硬编码文案与缺失 key（见 6.4）。

---

## 12. 参考资料

- Termora 仓库与截图：`TermoraDev/termora`（2.x）`docs/readme.png`、`host.png`、`transfer.png`、`transfer-edit.png`；README.zh_CN.md
- ShellAX 仓库：`daxiangme/ShellAX`（master，v1.3.5）README.md、Cargo.toml、`docs/UI_REDESIGN_PLAN.md`、`docs/PRD.md`、`docs/EXTENSIONS.md`
- 本仓库脚手架验证记录：会话 2026-11-08（cargo check / vite build / tauri dev 冒烟 / 私有库推送）