# Chatgpt HUD 0.2.0

- Rename the application and project, with a new cream/mint icon.
- Add English (default) and Simplified Chinese settings and tray labels.
- Read official Codex client/CLI ChatGPT sign-ins from CODEX_HOME/auth.json; exclude API-key accounts.
- Add auto-saving account display names and an explicit saved-login label.
- Fix theme example saving with a native Save As dialog.
- Add a bilingual offline API reference from About.
- Remove the waiting-status dot; keep conservative quota recommendations (5h >= 5%, weekly >= 2%).

Windows x64. Requires WebView2. OS-keyring and memory-only credentials are not supported. Public methods are in-app Tauri IPC; external HTTP/SSE is not implemented.

Validation: frontend production build, 53 Rust tests, isolated native settings/IPC/tray smoke checks, browser language/alias/download checks, and native Save As export with a verified file hash. Tests use synthetic accounts; no claim of compatibility with every live account or future upstream format.
