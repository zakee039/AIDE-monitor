# Widget 主题开发（0.4.0 / API 1）

三套内置主题保留。第三方主题是 `.aidetheme` ZIP 包，旧 `theme.json` 导入格式和旧主题命令已移除。

## 最小主题

复制 `examples/widget` 开始开发。包根目录必须有 `manifest.json`、`index.html`；脚本、样式、字体、图片均放包内，HTML 使用相对路径。原生 HTML 或 React/Vue/Svelte 构建产物均可，Vite 应设置 `base: './'`。不得使用 CDN、内联脚本、eval、远程字体、iframe、worker 或远程 API。

```json
{"schemaVersion":1,"id":"dev.author.widget","name":"My Widget","version":"1.0.0","author":"Author","apiVersion":"1"}
```

ID 为 1–100 位小写 ASCII 字母、数字、点或连字符，字母开头，无连续点或结尾点；default/midnight/paper 保留。version 必须是 SemVer，author 可选。不要增加入口、权限或窗口字段。

```js
const { aide } = window;
const unsubscribe = await aide.usage.onChanged(render);
await aide.window.resize(380, 160);
await aide.theme.ready();
// 组件销毁时 unsubscribe()；页面销毁时 SDK 自动清理。
```

构建项目可通过 `npm install ./theme-sdk` 安装本地 SDK，使用 `import { aide } from '@aide-monitor/theme-sdk'`；SDK 必须打包到主题，不能外链。普通脚本直接使用 `window.aide`。SDK 不访问本机 HTTP 端口。

## API

| 方法 | 行为 |
| --- | --- |
| `aide.accounts.list()` | 当前选中账号，脱敏名称和不透明 ID |
| `aide.accounts.refresh({accountIds?})` | 请求刷新所选账号，返回任务票据；不接受未选中账号 |
| `aide.usage.get()` | 完整缓存快照，不触发网络刷新 |
| `aide.usage.onChanged(listener)` | 返回 Promise<取消订阅函数>；先给快照，随后约每秒同步变化 |
| `aide.window.resize(width,height)` | 逻辑像素，返回裁剪后的实际目标尺寸 |
| `aide.window.drag()` | 主题在前台且鼠标按下时拖动 |
| `aide.window.hide()` | 隐藏；用户仍可从托盘找回 |
| `aide.storage.get/set/remove` | 当前安装身份隔离的 JSON 偏好 |
| `aide.app.openSettings()` | 打开可信设置窗口 |
| `aide.theme.ready()` | 首屏完成后调用，才显示新主题 |

[类型定义](../theme-sdk/index.d.ts) 与 [额度数据](../theme-sdk/contracts.d.ts)。未知数值为 null，不能当作 100%；Antigravity 使用 Gemini/Claude 家族各自的 5h/weekly 条目，不能把它们求和。

宿主注入请求 ID、会话 ID、API 版本；回复校验对应关系。每次变化携带完整快照，以 instanceId + revision 去重及拒绝旧数据；sequence 跳号和实例变化通过完整快照重同步。订阅使用约 1 秒轮询，不是服务端推送。请求 6 秒超时，销毁时中断。

## 窗口与恢复

尺寸限制 40–1000 × 40–560 逻辑像素，并裁剪到当前显示器工作区。无最大化、边缘缩放；右键由宿主管理。8 秒内未 ready 或脚本失败自动恢复可用界面；已显示且未隐藏的主题 15 秒无响应也回退。加载期间保留上一可用主题，成功后撤销旧会话。托盘可随时恢复默认内置主题。

## 安装与隔离

ZIP 限制：压缩包/单文件各 20 MiB、实际解压总量 80 MiB、2000 项、相对路径 180 字符。拒绝路径穿越、驱动器/UNC/ADS、Windows 保留名、链接和重复/大小写冲突路径。所有条目先验证，使用独立安装代目录和原子指针提交；失败不覆盖现有版本。

同 ID 不保证同作者，替换需确认并选择清理/保留偏好。卸载时可保留偏好，再安装时仍须同意交给新包。偏好单值 64 KiB，总计 1 MiB；每种 storage 方法每秒最多 30 次，resize 20 次，其余方法 10 次。超限返回错误。

主题独立 WebView 和浏览器数据目录，没有 Tauri capability。宿主关闭 WebMessage、默认浏览器菜单与开发工具，以原生请求过滤、导航/新窗口/下载拒绝和宿主 CSP 限制资源；只允许当前安装包和当前会话桥接。主题不能获取凭据、源路径、其他主题文件或任意联网权限。系统权限请求统一拒绝。

从项目根目录运行 `tools/pack-theme.ps1`，得到 `artifacts/example-widget.aidetheme`；在设置的外观页安装。测试使用合成账号，不宣称覆盖所有真实账号、显示器及 DPI 组合。
