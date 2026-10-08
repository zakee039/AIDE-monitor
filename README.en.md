# AIDE monitor

<p>
<a href="README.md"><img height="28" src=".github/assets/readme/zh-idle.svg" alt="简体中文" /></a>
<a href="README.en.md"><img height="28" src=".github/assets/readme/en-active.svg" alt="English" /></a>
</p>

**Your quota in sight. Your focus uninterrupted.**

AIDE monitor is a Windows desktop tool for monitoring AI account quotas. View quota balances and reset times for ChatGPT / Codex, Claude, Antigravity and Grok in a desktop widget or on a dedicated display, with less switching between platforms.

<p>
<a href="https://aide.zakee.fun"><img height="28" src=".github/assets/readme/preview-en.svg" alt="Live preview" /></a>
<a href="https://github.com/zakee039/AIDE-monitor/releases/latest"><img height="28" src=".github/assets/readme/download-en.svg" alt="Download" /></a>
<a href="https://zakee.fun"><img height="28" src=".github/assets/readme/homepage-en.svg" alt="Homepage" /></a>
<a href="https://ifdian.net/a/zakee/plan"><img height="28" src=".github/assets/readme/support-en.svg" alt="Support" /></a>
</p>

**Preview: [aide.zakee.fun](https://aide.zakee.fun)**

## See your quotas at a glance

- **Four platforms in one place:** view remaining quota and reset countdowns across multiple accounts. Supports Codex, Claude OAuth, Antigravity and Grok CLI OAuth.
- **Always within reach:** keep the desktop widget on top, lock its position or hide account names. Collapse it into an orb to see the selected platform's total quota.
- **Account-level controls:** select, reorder and rename accounts, with independent proxies and refresh intervals for each account.
- **A dedicated display:** move quota monitoring to a separate screen. Landscape, portrait and high-DPI displays are supported; the window hides on disconnection and returns when the display reconnects.
- **Choose your appearance:** desktop themes include Miku Mint, Dark and Light, with custom theme support. Dedicated-display themes are configured separately.

## Dedicated display

Mint and Daylight adapt to the account count: one account shows two quota windows side by side, two accounts use two rows, and three or four accounts use columns. More than four accounts switch pages every eight seconds.

The Four Platforms theme shows each platform's total quota or recovery countdown, its nearest quota reset countdown, and the number of selected accounts. Desktop and dedicated-display themes can be chosen independently.

Displays must appear in Windows display settings. Devices using proprietary USB image-transfer protocols require separate integration.

## Download and get started

Download the Windows x64 installer or portable version from [GitHub Releases](https://github.com/zakee039/AIDE-monitor/releases/latest). WebView2 is required.

1. Open **Settings → General → Data sources** and select your account source.
2. In **Accounts**, select the accounts to display and configure refresh intervals as needed.
3. Choose a desktop theme to start monitoring. Use the system tray to restore a hidden widget.
4. To use a dedicated display, select its target screen and theme in the corresponding settings, then enable it.

Supported sources include official clients, Cockpit Tools, CC Switch, CLIProxyAPI and local Sub2API exports. Login files are read without modifying the original accounts or executing custom balance scripts. If a login expires, sign in again in the original client.

## Understanding quota values

Totals are calculated separately for each platform. The orb accounts for subscription multipliers and quota windows to estimate available capacity; see the [quota calculation guide (Chinese)](docs/DOMAIN.md) for details. Claude and Antigravity conversion ratios are provisional estimates. Antigravity's built-in account view displays Gemini quota.

Quota colors run from red (below 20%) to orange (below 50%), blue (below 80%) and green (80% or above). Failed queries and missing data appear as unknown, never as full quota.

## Custom themes

The desktop supports `.aidetheme` Widget packages. If a theme fails to load, select **Restore built-in theme** from the tray menu. Dedicated displays support separate JSON themes for colors and layout, without script execution. The two theme formats are independent.

[Desktop theme development](docs/THEMES.md) · [Theme SDK](theme-sdk) · [Dedicated-display theme examples](examples/usb-themes)

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

Release artifacts are stored in `artifacts/release-v<version>/`, including the standalone executable, installer, portable ZIP and SHA-256 checksums.

## Project documentation

[Releases](https://github.com/zakee039/AIDE-monitor/releases) · [Architecture](docs/ARCHITECTURE.md) · [API](docs/API.md) · [Theme development](docs/THEMES.md) · [Theme SDK](theme-sdk) · [Dedicated-display theme examples](examples/usb-themes) · [Upstream references](docs/UPSTREAM.md) · [UI plans](界面更新规划.md)

Most project documentation is currently in Chinese.

This project is an independent implementation, not a fork of Cockpit Tools and not dependent on it at runtime. Upstream adapter source names are retained. New upstream formats require review and testing. A distribution license has not yet been selected.

