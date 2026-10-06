# AIDE monitor 0.4.0 接口

第三方主题使用独立的 **aide API 1**，见 [主题开发与方法表](THEMES.md)、[SDK 类型](../theme-sdk/index.d.ts)。接口没有 cockpit 命名或旧主题命令别名。

可信 React 设置页与内置 HUD 仍有内部 Tauri IPC，类型见 [内部契约](../contracts/hud-api.ts)；第三方主题无权调用它。新主题管理命令为 `aide_theme_list/select/install/uninstall`，可信窗口数据入口为 `aide_theme_data`，内置样式读取为 `aide_theme_builtin/builtins`。

没有公开 HTTP/SSE 服务器、远程调用令牌或网页访问入口。只读取用户选中的账号快照，不暴露 token、完整邮箱、凭据内容、任意本地路径或原始上游响应。隐私模式同样作用于主题数据。

旧 JSON 主题的 validate/preview/import/select/export 命令已删除。主题不能管理数据源、修改账号选择、安装其他主题或执行本地命令。
