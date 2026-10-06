# AIDE monitor

Windows 轻量配额悬浮窗，基于 Tauri 2、Rust、React。当前版本 **0.5.0**。

## 0.5.0

- 保留薄荷初音、黑暗、明亮三套内置主题；支持安装 `.aidetheme` 可执行 Widget 主题。
- 独立主题窗口通过 `window.aide` / `@aide-monitor/theme-sdk` 获取已选择账号的脱敏额度，使用独立偏好存储。移除旧 JSON 主题导入、预览、导出及兼容入口。
- 主题包原子安装，覆盖安装确认，卸载可清理偏好；加载失败自动回退，托盘可恢复内置主题。
- 悬浮窗右键提供刷新额度、展开/收起、隐藏、退出。禁止最大化和边缘缩放，尺寸由内容控制。
- Antigravity 提供 Gemini / Claude 的 5h 与周额度，内置主题只显示 Gemini，并与 Codex 对齐。缺失额度显示未知。

[版本说明](docs/releases/v0.5.0.md) · [主题开发](docs/THEMES.md) · [SDK](theme-sdk) · [接口说明](docs/API.md) · [GitHub](https://github.com/zakee039/AIDE-monitor)

## 本次更新

- 薄荷初音底栏头像由 22px 放大为 32px，并收紧左侧及底部留白至约 3px。

- 账号表改为单行，自动刷新可选 `- / 禁止 / 1min / 5min / 15min / 1h`；`-` 继承全局，其他项覆盖全局开关及间隔。禁止不影响手动刷新。
- 常规 → 窗口与显示可锁定位置，包括收起的圆窗和第三方主题拖动。
- 展开/收起在原生窗口尺寸及右边缘定位完成后显示，避免圆窗先在左侧闪现。
- 三套内置主题：miku mint / 薄荷初音、dark / 黑暗、white / 明亮。薄荷初音加入可伸缩葱分隔线和头像。
- USB 屏幕使用独立、无边框、不可缩放的全屏展示窗口；三套内置主题等比适配横屏、竖屏及高 DPI。按 Windows 显示器接口身份记忆设备，断线隐藏，不选择其他屏幕，重连自动恢复。
- USB 模式支持 Windows 显示设置中可见的显示器；厂商私有 USB 传图设备需单独适配。首次使用在设置中选择目标屏幕。该窗口使用独立主题，不影响悬浮窗主题。

## 使用

在设置的“常规 → 数据源”选择来源，然后在“账号”勾选需要显示的账号。支持官方客户端、Cockpit Tools、CC Switch、CLIProxyAPI 与 Sub2API 本地导出。只读登录文件，不修改原账号或执行自定义余额脚本；登录过期后在原客户端重新登录。

支持 Codex、Claude OAuth、Antigravity、Grok CLI OAuth。配额失败或缺失时不会推断为满额。Codex 汇总估算不包含其他平台；其换算规则见 [额度规则](docs/DOMAIN.md)。

外观页保留三套内置主题，可安装本地 `.aidetheme` 文件；加载异常时从托盘选择“恢复内置主题”。折叠圆球、隐藏后均可通过托盘找回窗口。

## 开发与验证

需要 Windows x64、WebView2、Node.js、Rust 与 Visual Studio C++ 工具链。

```powershell
npm ci
npm run tauri dev
npm test
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri build
```

在生产构建上运行合成账号验收，不访问真实凭据：

```powershell
.\src-tauri\target\release\aide-monitor.exe --smoke-test
.\src-tauri\target\release\aide-monitor.exe --smoke-test --theme-smoke
```

结果写入 `artifacts/native-smoke.json` 与 `artifacts/theme-smoke.json`。主题模板打包：

```powershell
.\tools\pack-theme.ps1
```

安装包位于 `src-tauri/target/release/bundle/nsis`。本版继续使用已有应用标识以保留账号设置；旧第三方 JSON 主题不迁移。

## 项目资料

[界面更新规划](界面更新规划.md) · [上游参考](docs/UPSTREAM.md) · [架构](docs/ARCHITECTURE.md)

本项目为独立实现，不是 Cockpit Tools 的分支或运行依赖；上游适配来源名称保留。新增上游格式需经过审阅和测试。当前尚未选择分发许可证。
