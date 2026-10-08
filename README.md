# AIDE monitor

<p>
<a href="README.md"><img height="28" src=".github/assets/readme/zh-active.svg" alt="简体中文" /></a>
<a href="README.en.md"><img height="28" src=".github/assets/readme/en-idle.svg" alt="English" /></a>
</p>

**额度看得见，专注不断线。**

AIDE monitor 是一款 Windows 桌面 AI 账号额度监视工具。把 ChatGPT / Codex、Claude、Antigravity 和 Grok 的账号额度与重置时间放在一起，通过桌面悬浮窗或独立监视屏随时查看，减少来回切换平台。

<p>
<a href="https://aide.zakee.fun"><img height="28" src=".github/assets/readme/preview-zh.svg" alt="在线预览" /></a>
<a href="https://github.com/zakee039/AIDE-monitor/releases/latest"><img height="28" src=".github/assets/readme/download-zh.svg" alt="下载最新版" /></a>
<a href="https://zakee.fun"><img height="28" src=".github/assets/readme/homepage-zh.svg" alt="个人主页" /></a>
<a href="https://ifdian.net/a/zakee/plan"><img height="28" src=".github/assets/readme/support-zh.svg" alt="支持作者" /></a>
</p>

**预览页面：[aide.zakee.fun](https://aide.zakee.fun)**

## 一眼掌握账号额度

- **四大平台，一处查看**：集中展示多个账号的剩余额度和重置倒计时，支持 Codex、Claude OAuth、Antigravity 和 Grok CLI OAuth。
- **桌面常驻，随时可见**：悬浮窗支持置顶、位置锁定和隐藏账号名称；收起为悬浮球后，仍可查看所选平台的总额度。
- **账号按需管理**：选择参与展示的账号，调整顺序和名称，为每个账号单独设置代理与刷新间隔。
- **小屏独立显示**：把额度放到独立监视屏上，主屏留给工作。支持横屏、竖屏和高 DPI，断开后隐藏，重连后恢复。
- **外观自由选择**：桌面提供薄荷初音、暗夜与明亮主题，也支持安装自定义主题。独立监视屏使用自己的主题设置。

## 独立监视屏

薄荷初音与白昼主题根据账号数量调整布局：单账号的两项额度左右并排，双账号上下排列，三至四个账号分栏展示。超过四个账号时，每 8 秒切换一页。

四平台主题集中显示各平台总额度或额度恢复倒计时，同时展示最近一次额度重置倒计时和参与展示的账号数量。桌面与独立监视屏分别选择主题，互不影响。

支持 Windows 显示设置中可见的显示器；使用厂商私有 USB 传图协议的设备需要单独适配。

## 下载与开始使用

从 [GitHub Releases](https://github.com/zakee039/AIDE-monitor/releases/latest) 下载 Windows x64 安装包或便携版。运行需要 WebView2。

1. 打开设置，在 **常规 → 数据源** 选择账号来源。
2. 在 **账号** 中勾选需要展示的账号，并按需设置刷新间隔。
3. 选择桌面主题，开始查看额度；悬浮窗隐藏后可通过系统托盘找回。
4. 使用独立监视屏时，在对应设置中选择目标屏幕和主题并启用。

支持官方客户端、Cockpit Tools、CC Switch、CLIProxyAPI 和 Sub2API 本地导出。程序只读登录文件，不修改原账号，也不执行自定义余额脚本。登录过期时，请在原客户端重新登录。

## 如何理解额度

各平台独立计算总额度，不跨平台相加。悬浮球的汇总会结合订阅倍率和额度周期进行换算，适合快速判断可用情况；具体规则见 [额度计算说明](docs/DOMAIN.md)。Claude 与 Antigravity 的换算比例为暂定估算，Antigravity 内置账号视图显示 Gemini 额度。

额度颜色由低到高为红色（低于 20%）、橙色（低于 50%）、蓝色（低于 80%）和绿色（80% 及以上）。查询失败或数据缺失时显示未知，不会当作满额。

## 自定义主题

桌面支持安装 `.aidetheme` Widget 主题，加载异常时可从托盘选择“恢复内置主题”。独立监视屏支持专用 JSON 主题，用于配置颜色和布局，不执行脚本。两种主题格式独立使用。

[桌面主题开发](docs/THEMES.md) · [Theme SDK](theme-sdk) · [独立监视屏主题示例](examples/usb-themes)

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

发布产物位于 `artifacts/release-v<版本号>/`，包括独立运行 EXE、安装包、便携 ZIP 与 SHA-256 校验文件。

## 项目资料

[版本更新](https://github.com/zakee039/AIDE-monitor/releases) · [架构](docs/ARCHITECTURE.md) · [接口说明](docs/API.md) · [上游参考](docs/UPSTREAM.md) · [界面更新规划](界面更新规划.md)

本项目为独立实现，不是 Cockpit Tools 的分支或运行依赖；上游适配来源名称保留。新增上游格式需经过审阅和测试。当前尚未选择分发许可证。
