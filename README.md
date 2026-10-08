# AIDE monitor

AIDE monitor（AI IDE monitor），专业的 AI IDE 订阅额度监视器。集中监控多账号配额与恢复时间，支持桌面悬浮窗和独立监视屏，基于 Tauri 2、Rust、React。当前版本 **0.6.1**。

[个人主页](https://zakee.fun) · [爱发电 / Buy Me a Coffee](https://ifdian.net/a/zakee/plan) · [下载最新版](https://github.com/zakee039/AIDE-monitor/releases/latest)

## 0.6.1

- 薄荷初音、白昼改为最多四账号分栏，超出自动翻页；移除标题、时钟和推荐栏，放大图标、账号名与黑色重置倒计时，长名称省略，不再显示 5h / 7d 标签。
- 四平台保持悬浮球的统一总额度计算；总额度下显示该平台最近一次额度重置倒计时，底部显示已选账号数量。
- 三款独立监视屏主题与主界面共用蓝、绿、橙、红额度分级；缺失或过期数据保持未知。

[0.6.1 版本说明](docs/releases/v0.6.1.md)

## 0.6.0

- 桌面与独立监视屏统一主题选择界面：导入、横向选择器、实时预览和应用按钮。选择只预览，点击应用后生效。
- 桌面小窗和悬浮球在同一行预览；两套主题保持独立，不混用配色、布局或主题包。
- USB 监视屏统一更名为“独立监视屏”，设备设置置顶，去除分辨率和冗余说明。屏幕布局自适应横屏、竖屏和高 DPI。
- 独立监视屏提供薄荷初音、白昼和四平台主题；四平台按 ChatGPT、Claude、Antigravity、Grok 分别累计额度，复用悬浮球计算规则。
- 四平台可用时显示百分比，等待恢复时显示双行倒计时，底部仅显示已选账号数量；缺失数据保持未知。
- 支持导入独立监视屏专用 JSON 主题，包含颜色和布局配置，不执行脚本。[主题示例与格式](examples/usb-themes)

[0.6.0 版本说明](docs/releases/v0.6.0.md)

## 0.5.3

- 悬浮窗额度平台仅显示已勾选账号的平台。当前平台失效时，按账号排序自动回退到第一个已选账号的平台，并保存结果。
- 未选择账号时禁用平台选择；重新扫描账号时同步处理平台回退。

[0.5.3 版本说明](docs/releases/v0.5.3.md)

## 0.5.2

- 圆形悬浮窗支持 ChatGPT、Claude、Antigravity、Grok 独立累计，只列出已有账号的平台。
- 5h 账号先剔除 5h < 5% 或周额度 < 2% 的余额，再按周额度可支持的实际容量折算；支持不同订阅倍率及 5h / 7d 混用。
- “窗口与显示”独立成设置页，位于常规下方。增加平台选择和按账号指定订阅档位、自定义倍率及换算比例。
- 缩窄账号表的代理和自动刷新列，设置窗口默认宽度从 720 调整到 740 逻辑像素。
- Claude / Antigravity 换算比例为暂定估算；Antigravity 采用 Gemini 额度池，Grok 高级订阅可自定义倍率。

[0.5.2 版本说明](docs/releases/v0.5.2.md)

## 0.5.1

- 常规设置新增多组代理，每行包含名称和地址，输入即自动保存，支持 HTTP / HTTPS / SOCKS5 / SOCKS5H。
- 每个账号可独立选择代理，默认“系统”，使用系统/环境代理设置；删除代理后关联账号恢复为系统。无效或无法连接的显式代理会报错，不静默回退。
- 修复已耗尽窗口与服务端总许可标记不同步时，漏选最早恢复账号的问题。5h 低于 5%、7d 低于 2% 不可用，恰好 5% / 2% 可用；只对不足窗口取最晚重置，再从各账号中选最早恢复者。
- 去除独立监视屏设置的状态行，更新关于页介绍，修正 `available` 拼写。
- 关于 → 版本更新通过 GitHub Releases 检查最新正式版，展示更新说明并打开 GitHub 下载页面。

[0.5.1 版本说明](docs/releases/v0.5.1.md)

## 0.5.0

- 保留薄荷初音、黑暗、明亮三套内置主题；支持安装 `.aidetheme` 可执行 Widget 主题。
- 独立主题窗口通过 `window.aide` / `@aide-monitor/theme-sdk` 获取已选择账号的脱敏额度，使用独立偏好存储。移除旧 JSON 主题导入、预览、导出及兼容入口。
- 主题包原子安装，覆盖安装确认，卸载可清理偏好；加载失败自动回退，托盘可恢复内置主题。
- 悬浮窗右键提供刷新额度、展开/收起、隐藏、退出。禁止最大化和边缘缩放，尺寸由内容控制。
- Antigravity 提供 Gemini / Claude 的 5h 与周额度，内置主题只显示 Gemini，并与 Codex 对齐。缺失额度显示未知。

[版本说明](docs/releases/v0.5.0.md) · [主题开发](docs/THEMES.md) · [SDK](theme-sdk) · [接口说明](docs/API.md) · [GitHub](https://github.com/zakee039/AIDE-monitor)

## 桌面与屏幕功能

- 薄荷初音底栏头像由 22px 放大为 32px，并收紧左侧及底部留白至约 3px。

- 账号表改为单行，自动刷新可选 `- / 禁止 / 1min / 5min / 15min / 1h`；`-` 继承全局，其他项覆盖全局开关及间隔。禁止不影响手动刷新。
- 窗口与显示可锁定位置，包括收起的圆窗和第三方主题拖动。
- 展开/收起在原生窗口尺寸及右边缘定位完成后显示，避免圆窗先在左侧闪现。
- 三套内置主题：miku mint / 薄荷初音、dark / 黑暗、white / 明亮。薄荷初音加入可伸缩葱分隔线和头像。
- 独立监视屏使用独立、无边框、不可缩放的全屏展示窗口；主题自适应横屏、竖屏及高 DPI，四平台竖屏采用两行排列。按 Windows 显示器接口身份记忆设备，断线隐藏，不选择其他屏幕，重连自动恢复。
- 独立监视屏模式支持 Windows 显示设置中可见的显示器；厂商私有 USB 传图设备需单独适配。首次使用在设置中选择目标屏幕。该窗口使用独立主题，不影响悬浮窗主题。

## 使用

在设置的“常规 → 数据源”选择来源，然后在“账号”勾选需要显示的账号。支持官方客户端、Cockpit Tools、CC Switch、CLIProxyAPI 与 Sub2API 本地导出。只读登录文件，不修改原账号或执行自定义余额脚本；登录过期后在原客户端重新登录。

支持 Codex、Claude OAuth、Antigravity、Grok CLI OAuth。配额失败或缺失时不会推断为满额。圆形悬浮窗按所选平台独立累计；其换算规则见 [额度规则](docs/DOMAIN.md)。

外观页保留三套内置主题，可安装本地 `.aidetheme` 文件；加载异常时从托盘选择“恢复内置主题”。折叠圆球、隐藏后均可通过托盘找回窗口。

## 开发与验证

需要 Windows x64、WebView2、Node.js、Rust 与 Visual Studio C++ 工具链。

```powershell
npm ci
npm run tauri dev
npm test
cargo test --manifest-path src-tauri/Cargo.toml
powershell -ExecutionPolicy Bypass -File tools/build-artifacts.ps1
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

发布产物统一位于 `artifacts/release-v0.6.1/`，包括独立运行 EXE、安装包、便携 ZIP 与 SHA-256 校验文件。本版继续使用已有应用标识以保留账号设置；旧第三方 JSON 主题不迁移。

## 项目资料

[界面更新规划](界面更新规划.md) · [上游参考](docs/UPSTREAM.md) · [架构](docs/ARCHITECTURE.md)

本项目为独立实现，不是 Cockpit Tools 的分支或运行依赖；上游适配来源名称保留。新增上游格式需经过审阅和测试。当前尚未选择分发许可证。
