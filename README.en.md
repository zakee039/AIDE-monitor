# AIDE monitor

<p>
<a href="README.md"><img height="28" src=".github/assets/readme/zh-idle.svg" alt="简体中文" /></a>
<a href="README.en.md"><img height="28" src=".github/assets/readme/en-active.svg" alt="English" /></a>
</p>

AIDE monitor (AI IDE monitor) brings the quota and reset times of multiple AI accounts into one view. Built with Tauri 2, Rust and React, it supports a desktop widget and a dedicated display. Current version: **0.6.2**.

<p>
<a href="https://aide.zakee.fun"><img height="28" src=".github/assets/readme/preview-en.svg" alt="Live preview" /></a>
<a href="https://github.com/zakee039/AIDE-monitor/releases/latest"><img height="28" src=".github/assets/readme/download-en.svg" alt="Download" /></a>
<a href="https://zakee.fun"><img height="28" src=".github/assets/readme/homepage-en.svg" alt="Homepage" /></a>
<a href="https://ifdian.net/a/zakee/plan"><img height="28" src=".github/assets/readme/support-en.svg" alt="Support" /></a>
</p>

**Preview: [aide.zakee.fun](https://aide.zakee.fun)**

## What's new in 0.6.2

- Mint and Daylight dedicated-display themes adapt to the account count: one account shows two quota windows side by side; two accounts use two rows; three or four accounts use columns.
- More than four accounts slide to the next page every eight seconds, retaining quota values, reset times and quota colors.

[0.6.2 release notes (Chinese)](docs/releases/v0.6.2.md)

## Features

- **Four platforms:** Codex, Claude OAuth, Antigravity and Grok CLI OAuth. View multiple accounts together, including remaining quota and reset times.
- **Compact desktop widget:** keep it on top, lock its position, hide account names with privacy mode, or collapse it into a quota orb. The tray menu restores a hidden window.
- **Per-platform totals:** the orb estimates quota separately for the selected platform, accounting for subscription multipliers and supported quota windows. Missing or failed readings are never treated as full quota.
- **Per-account settings:** choose which accounts to display, reorder or rename them, and assign independent proxies and refresh intervals. Account refresh options can inherit the global setting or override it; disabling automatic refresh does not prevent manual refresh.
- **Dedicated display:** a separate, borderless window adapts to landscape, portrait and high-DPI screens. Desktop and dedicated-display themes have independent settings and formats.
- **Flexible themes:** desktop themes include Miku Mint, Dark and Light, with support for executable `.aidetheme` widget packages. Dedicated displays provide Mint, Daylight and Four Platforms themes, plus dedicated JSON theme imports that do not execute scripts.

The Four Platforms dedicated-display theme shows each platform's calculated total quota or recovery countdown, the nearest quota reset countdown, and the number of selected accounts. Unknown values remain unknown. Quota colors use the same thresholds as the main interface: red below 20%, orange below 50%, blue below 80%, and green at 80% or above.

Antigravity's built-in account view displays Gemini quota. Claude and Antigravity conversion ratios are provisional estimates. See [quota rules (Chinese)](docs/DOMAIN.md) for calculation details.

## Download and use

Download the Windows x64 installer, standalone executable or portable ZIP from [GitHub Releases](https://github.com/zakee039/AIDE-monitor/releases/latest). WebView2 is required.

1. In **Settings → General → Data sources**, select your account source.
2. In **Accounts**, select the accounts you want to display.
3. Choose your desktop theme and configure refresh intervals as needed.
4. To use a dedicated display, select its target screen and theme in the dedicated-display settings.

Supported sources include official clients, Cockpit Tools, CC Switch, CLIProxyAPI and local Sub2API exports. Login files are read without modifying the original accounts or executing custom balance scripts. If a login expires, sign in again in the original client.

Dedicated displays must be visible in Windows display settings. Devices using proprietary USB image-transfer protocols require separate integration. The application remembers the selected display, hides its window on disconnection, and restores it when that display reconnects.

If a custom desktop theme fails to load, use **Restore built-in theme** from the tray menu.

## Recent releases

- **0.6.1:** refined dedicated-display account columns, larger icons and names, clearer reset countdowns, automatic paging and shared quota color thresholds. [Notes](docs/releases/v0.6.1.md)
- **0.6.0:** unified theme-selector controls, independent desktop and dedicated-display themes, Four Platforms display, and dedicated JSON theme imports. [Notes](docs/releases/v0.6.0.md)
- **0.5.3:** platform selection follows selected accounts and falls back automatically when necessary. [Notes](docs/releases/v0.5.3.md)
- **0.5.2:** independent totals for four platforms, subscription multipliers and quota conversion settings. [Notes](docs/releases/v0.5.2.md)
- **0.5.1:** per-account proxies, recovery recommendation fixes and release update checks. [Notes](docs/releases/v0.5.1.md)
- **0.5.0:** executable widget themes, theme SDK, isolated preferences and automatic fallback. [Notes](docs/releases/v0.5.0.md)

Release notes linked above are in Chinese.

## Development and validation

Requires Windows x64, WebView2, Node.js, Rust and the Visual Studio C++ toolchain.

```powershell
npm ci
npm run tauri dev
npm test
cargo test --manifest-path src-tauri/Cargo.toml
powershell -ExecutionPolicy Bypass -File tools/build-artifacts.ps1
```

Run synthetic-account checks against a production build without accessing real credentials:

```powershell
.\src-tauri\target\release\aide-monitor.exe --smoke-test
.\src-tauri\target\release\aide-monitor.exe --smoke-test --theme-smoke
```

Results are written to `artifacts/native-smoke.json` and `artifacts/theme-smoke.json`. To package a theme template:

```powershell
.\tools\pack-theme.ps1
```

Release artifacts are stored in `artifacts/release-v0.6.2/`, including the standalone executable, installer, portable ZIP and SHA-256 checksums. Existing account settings are retained using the application's established identifier. Legacy third-party JSON desktop themes are not migrated.

## Project documentation

[Architecture](docs/ARCHITECTURE.md) · [API](docs/API.md) · [Theme development](docs/THEMES.md) · [Theme SDK](theme-sdk) · [Dedicated-display theme examples](examples/usb-themes) · [Upstream references](docs/UPSTREAM.md) · [UI plans](界面更新规划.md)

Most project documentation is currently in Chinese.

This project is an independent implementation, not a fork of Cockpit Tools and not dependent on it at runtime. Upstream adapter source names are retained. New upstream formats require review and testing. A distribution license has not yet been selected.

