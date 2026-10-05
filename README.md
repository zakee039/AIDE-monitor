# Chatgpt HUD

A compact Windows quota HUD built with **Tauri 2 + Rust + React / TypeScript**. Cream background, mint accent (`#39C5BB`), tray controls and custom JSON themes. No Electron.

**Version 0.2.0** · [中文说明](#中文说明) · [API reference](public/api.html) · [TypeScript contract](contracts/hud-api.ts)

## Download

[Windows releases](https://github.com/zakee039/chatgpt-HUD/releases/tag/v0.2.0). Build from source if release binaries are unavailable. Windows x64 with WebView2 is required.

## What's new

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
5. Use the HUD footer or tray menu to open settings, refresh all accounts or hide the HUD. Only the settings window appears on the taskbar.

The source is one selected directory at a time. Official OS-keyring and memory-only credentials are not yet supported. The “Local sign-in” label identifies the saved official file at the last scan; it does not identify every open conversation's active account. Rescan after switching accounts. Expired sign-ins must be renewed in the original client; HUD never exchanges refresh tokens or modifies source files.

The HUD shows 5-hour and weekly remaining quota, reset countdowns and provider-reported reset credits (`R`). Colors: below 20% red, 20–49% orange, 50–79% blue, 80–100% green. A current recommendation requires **5h ≥ 5% and weekly ≥ 2%**. Unknown, stale or failed samples are never treated as available. Countdown expiration requires verification.

## Themes and API

Three included themes: Cream Mint, Midnight and Daylight. Import a validated local JSON theme under Appearance; download the example there or use [theme-template.json](public/theme-template.json). Themes change appearance only.

Open About → API reference for parameters, results and examples. The API currently uses **in-app Tauri IPC**, not an external HTTP service. Untrusted webpages cannot invoke it. Credentials are never returned to the frontend. Architecture documents include future HTTP/SSE proposals; those are not implemented in 0.2.0.

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

**Chatgpt HUD 0.2.0** 是基于 Tauri 的轻量配额悬浮窗，奶油背景、薄荷主题色，主要操作位于托盘右键菜单。

- 常规顶部支持中英切换，默认英语；设置、托盘同步切换。
- 默认发现 Codex 官方客户端/CLI 的 ChatGPT 登录文件：`CODEX_HOME/auth.json`，一般为 `~/.codex/auth.json`；保留 Cockpit Tools 只读兼容，排除 API 模式。
- 每个账号右侧可填写显示名称，自动保存；清空恢复原名。下方“显示名称”为说明文字。
- 关于页面新增接口说明网页；主题示例通过系统“另存为”保存。
- 账号前旧圆点表示等待重置，不是当前账号，现已去掉。“当前本机登录”仅表示官方文件中保存的账号，切换后需重新扫描，不代表所有打开会话使用的账号。
- 5H 低于 5%、周额度低于 2% 的账号不列为当前可用推荐。`R` 为服务返回的可用重置次数。
- 目前一次选择一个数据目录；系统凭据库或仅内存保存的登录暂不支持。认证过期请在原客户端登录，HUD 不修改原账号文件。

版本测试使用合成账号，不能据此宣称所有真实账号或未来上游格式均兼容。当前公开接口为应用内部 IPC，外部 HTTP / SSE 尚未实现。
