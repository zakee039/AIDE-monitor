# AIDE monitor 0.4.0 接口

第三方主题使用独立的 **aide API 1**，见 [主题开发与方法表](THEMES.md)、[SDK 类型](../theme-sdk/index.d.ts)。接口没有 cockpit 命名或旧主题命令别名。

可信 React 设置页与内置 HUD 仍有内部 Tauri IPC，类型见 [内部契约](../contracts/hud-api.ts)；第三方主题无权调用它。新主题管理命令为 `aide_theme_list/select/install/uninstall`，可信窗口数据入口为 `aide_theme_data`，内置样式读取为 `aide_theme_builtin/builtins`。

没有公开 HTTP/SSE 服务器、远程调用令牌或网页访问入口。只读取用户选中的账号快照，不暴露 token、完整邮箱、凭据内容、任意本地路径或原始上游响应。隐私模式同样作用于主题数据。

旧 JSON 主题的 validate/preview/import/select/export 命令已删除。主题不能管理数据源、修改账号选择、安装其他主题或执行本地命令。


### 账号刷新与独立屏幕

`Settings.accountRefresh` 为账号 ID 到秒数的映射，完整替换写入。省略账号表示继承全局；`0` 禁止自动刷新；允许 `60 / 300 / 900 / 3600`。手动刷新不受此开关限制。自动刷新、重置核实和时钟变动后的自动查询均遵循覆盖设置。

`Settings.display.positionLocked` 控制悬浮窗及主题窗口的拖动。

`Settings.usbDisplay` 为 `{ enabled, deviceId, themeId, customThemes? }`，整体写入；独立监视屏主题使用 usb-mint、usb-day、usb-quad 或专用导入主题，不能使用桌面主题 ID。`deviceId` 为 Windows 监视器接口路径，不使用易变的显示器编号。配置修改继续要求 `expectedRevision`。设置窗口可通过 `aide_usb_displays` 获取当前连接屏幕；独立监视屏窗口只获读取权限。

## 0.5.1 设置扩展

`Settings` / `settings.update` 增加 `proxies: Array<{id,name,address}>` 和 `accountProxies: Record<accountId, proxyId>`。缺少账号映射表示系统代理；删除代理原子解除对应映射。代理配置允许输入中间状态，但发起配额请求前严格验证，失败不回退。两项沿用 `expectedRevision` 并发控制；旧设置缺省为空。

仅可信设置窗口可调用内部 `hud_internal_update_check`（检查 GitHub 最新正式版）和 `hud_internal_update_open`（打开固定 GitHub 下载页），均接受空 `request`。第三方主题不获得代理地址或更新权限。
