# AIDE monitor

## AIDE monitor 0.3.1

- 悬浮窗及圆球右键菜单：刷新额度、展开/收起、隐藏、退出。
- 禁止最大化与边缘缩放，保留内容自适应和展开/收起。
- Antigravity 使用汇总接口，保留 Gemini / Claude 的 5h 与周额度；内置主题显示 Gemini 两组额度并与 Codex 对齐。
- 修复账号列表右侧输入框对齐，保留三套内置主题。
- 更新第三方主题系统规划；可执行主题运行时尚未实现。

[版本说明](docs/releases/v0.3.1.md) · [GitHub 标签](https://github.com/zakee039/chatgpt-HUD/releases/tag/v0.3.1)

## AIDE monitor 0.3.0（历史版本）

已更名为 **AIDE monitor**：#39C5BB 的 M 标志搭配两侧发箍。新增 Windows 开机自启开关、精简设置页、平台图标和多数据源选择弹窗。

- 平台：Codex、Claude OAuth、Antigravity、Grok CLI OAuth。按平台读取真实窗口／模型额度；不把缺失数据当满额。
- 来源：Cockpit 多平台账号；CC Switch 只读数据库中的 OAuth 配置；CLIProxyAPI OAuth JSON；Sub2API 本地账号导出 JSON；官方 Codex / Claude 本地登录文件。
- 在“常规 → 数据源 → 选择源”勾选多个来源，选择目录后保存并扫描。只扫描所选位置及约定子目录。
- 开机自启只写当前 Windows 用户的启动项，无需管理员权限；关闭开关会移除该项。默认为关闭。
- 保留已有应用标识和数据目录，升级继续使用 v0.2.3 的设置与账号别名。
- Codex 总额度估算仅统计 Codex；其他平台不与它相加。Antigravity / Grok 的模型或产品额度不推断为整个账号可用。

**兼容边界：** CC Switch 的任意 API Key／自定义余额脚本不执行；Sub2API 当前读取导出文件，不直连远程管理服务或 PostgreSQL。登录过期时在原客户端重新登录。本版未使用真实账号执行新增平台网络联调。

[本地安装包](artifacts/AIDE-monitor-0.3.0-windows-x64-setup.exe) · [直接运行](artifacts/AIDE-monitor-0.3.0-windows-x64.exe) · [支持矩阵与验证记录](docs/releases/v0.3.0.md)

下方保留 v0.2.x 的使用说明与历史发布记录。

A compact Windows quota HUD built with **Tauri 2 + Rust + React / TypeScript**. Cream background, mint accent (`#39C5BB`), tray controls and custom JSON themes. No Electron.

**Version 0.2.3** · [中文说明](#中文说明) · [API reference](public/api.html) · [TypeScript contract](contracts/hud-api.ts)

## Download

[Windows releases](https://github.com/zakee039/chatgpt-HUD/releases/tag/v0.2.3). Windows x64 with WebView2 is required.

## What's new

- Fold the HUD with the footer's `>` button into a draggable 43 × 43 quota orb with larger 16px text. Click it to expand. It shows the estimated sum across selected accounts, including totals above 100%; when all known quota is exhausted, it shows the footer's earliest usable countdown on two centered lines, such as `3H` / `16M` (or days / hours for longer waits).
- Content-sized account and quota columns, aligned across rows with 4-character spacing between columns. Percentages, separators and countdowns share their own aligned tracks. Weekly-only accounts omit the 5-hour limit; footer text shares one font and baseline. All three built-in themes use the same behavior.
- `quota.snapshot.get` includes `totalQuota: {percent, partial, weeklyScalePercent}`. Dual-limit accounts contribute `5hRemaining × min(weeklyRemaining / 15, 1)`; weekly-only accounts contribute their weekly remaining percentage directly. **15% is a local estimate requested for this HUD, not an official OpenAI conversion.** [Official Codex pricing](https://learn.chatgpt.com/docs/pricing) describes plan-dependent limits without prescribing this ratio. Failed, stale or unconfirmed accounts are omitted, with partial coverage retained in accessibility labels and `—` indicating an unknown total. Recommendation floors remain separate.

Version 0.2.0 also introduced:

- English by default, with an instantly saved English / 简体中文 selector at the top of General settings. Tray labels follow the selected language.
- Read-only official Codex client / CLI ChatGPT sign-ins from `CODEX_HOME/auth.json` (default `~/.codex/auth.json`), with Cockpit Tools compatibility. API-key accounts are excluded.
- Auto-saving display names in account settings; the HUD updates immediately. Clearing an alias restores the source name.
- A local, offline-capable API reference webpage from About. 17 public methods in the settings window; 9 in the HUD.
- Theme example download uses the native Save As dialog in the desktop app.
- New Chatgpt HUD branding and icon. Existing settings from `dev.cockpit-quota-hud.desktop` are copied on first launch when no new settings exist; originals are preserved.
- Removed the waiting-status dot, which was not an active-account indicator.

## Use

1. Sign in with ChatGPT in the official Codex client / CLI, or use existing Cockpit Tools login accounts.
2. Start Chatgpt HUD. Right-click the tray icon → Settings. Automatic source discovery prefers the official auth file; an existing explicitly selected source is preserved.
3. If needed, choose the directory containing `auth.json` or Cockpit's `codex_accounts.json`.
4. Select accounts and save the selection. Edit display names directly; they save automatically.
5. Use the HUD footer or tray menu to open settings, refresh all accounts or hide the HUD. Fold with `>`; click the orb to expand, or drag it to reposition. Only the settings window appears on the taskbar.

The source is one selected directory at a time. Official OS-keyring and memory-only credentials are not yet supported. The “Local sign-in” label identifies the saved official file at the last scan; it does not identify every open conversation's active account. Rescan after switching accounts. Expired sign-ins must be renewed in the original client; HUD never exchanges refresh tokens or modifies source files.

The HUD shows applicable 5-hour and weekly remaining quota, reset countdowns and provider-reported reset credits (`R`). Colors: below 20% red, 20–49% orange, 50–79% blue, 80–100% green. A current recommendation requires **5h ≥ 5% and weekly ≥ 2%** for applicable limits; a weekly-only account requires weekly ≥ 2%. Unknown, stale or failed samples are never treated as available. Countdown expiration requires verification.

## Themes and API

Three included themes: Cream Mint, Midnight and Daylight. Import a validated local JSON theme under Appearance; download the example there or use [theme-template.json](public/theme-template.json). Themes change appearance only.

Open About → API reference for parameters, results and examples. The API currently uses **in-app Tauri IPC**, not an external HTTP service. Untrusted webpages cannot invoke it. Credentials are never returned to the frontend. Architecture documents include future HTTP/SSE proposals; those are not implemented in 0.2.3.

## Develop

Prerequisites: Node.js 22+, Rust stable, Windows C++ build tools and WebView2.

```sh
npm ci
npm run dev
npm run tauri dev
```

Browser preview at `http://127.0.0.1:1420/?view=settings` uses fictional data only. Run either the browser development server or Tauri development command, not both on the same port.

```sh
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri build
```

The Windows installer is generated under `src-tauri/target/release/bundle/nsis/`. Run the built executable with `--smoke-test` for isolated native WebView/IPC checks; it exits and writes `artifacts/native-smoke.json` without using real accounts or sending quota requests.

## Architecture and upstream

- [Implementation status](docs/IMPLEMENTATION.md)
- [Architecture](docs/ARCHITECTURE.md) · [Quota rules](docs/DOMAIN.md)
- [API design](docs/API.md) · [Theme format](docs/THEMES.md)
- [Upstream reference and update process](docs/UPSTREAM.md)

This is an independent implementation, not a fork or runtime dependency of Cockpit Tools. Its adapter references observed storage and quota formats. Upstream changes require review, fixture updates and a new application release; they cannot be copied in blindly or automatically executed. Existing upstream attribution and compatibility notes remain under their original names. This project has not yet selected a distribution license.

## 中文说明

**Chatgpt HUD 0.2.3** 是基于 Tauri 的轻量配额悬浮窗，奶油背景、薄荷主题色，主要操作位于托盘右键菜单。

- 底栏 `>` 将 HUD 折叠为 43 × 43 圆球，点击圆球展开，按住拖动调整位置；总额度可超过 100%，全部可信额度耗尽时显示最近可用倒计时。
- 双限额账号按 `Σ[5H剩余% × min(周剩余% / 15, 1)]` 汇总；只有周限额的账号直接贡献周剩余百分比。15% 是本项目估算规则，官方未公布固定换算比例。过期、失败或等待核实的样本不计入，部分状态保留在无障碍说明中，数值不带约等号，未知使用 `—`。
- 账号与额度列由内容决定宽度，周限额单独存在时隐藏 5H；`now` 与底栏名称使用相同字体和基线，三个自带主题均生效。

- 常规顶部支持中英切换，默认英语；设置、托盘同步切换。
- 默认发现 Codex 官方客户端/CLI 的 ChatGPT 登录文件：`CODEX_HOME/auth.json`，一般为 `~/.codex/auth.json`；保留 Cockpit Tools 只读兼容，排除 API 模式。
- 每个账号右侧可填写显示名称，自动保存；清空恢复原名。下方“显示名称”为说明文字。
- 关于页面新增接口说明网页；主题示例通过系统“另存为”保存。
- 账号前旧圆点表示等待重置，不是当前账号，现已去掉。“当前本机登录”仅表示官方文件中保存的账号，切换后需重新扫描，不代表所有打开会话使用的账号。
- 5H 低于 5%、周额度低于 2% 的账号不列为当前可用推荐。`R` 为服务返回的可用重置次数。
- 目前一次选择一个数据目录；系统凭据库或仅内存保存的登录暂不支持。认证过期请在原客户端登录，HUD 不修改原账号文件。

版本测试使用合成账号，不能据此宣称所有真实账号或未来上游格式均兼容。当前公开接口为应用内部 IPC，外部 HTTP / SSE 尚未实现。
