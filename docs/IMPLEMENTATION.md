> 0.4.0 主题系统已更换为独立 Widget WebView，当前规范见 [THEMES](THEMES.md) 与 [API](API.md)。下文保留早期架构背景，旧 JSON 主题与旧主题命令已删除。

# v0.2.3 实现说明

更新日期：2026-10-05。0.1.2 在奶油薄荷紧凑 HUD 上加入登录账号过滤、额度分段色、底栏按钮、重置次数显示与任务栏隐藏。本文件记录当前源码已经提供的行为；其他设计文档中的完整目标不等于本版全部实现。

## 0. 0.2.3 更新

圆球保持 43 × 43，主字号从 12px 增至 16px。可用倒计时分两行居中显示小时 / 分钟（例如 `3H` / `16M`），超过一天时显示天 / 小时；长百分比继续按长度缩小，避免超出圆内。三个内置主题共用这些规则。

### 0.2.2 更新

账号、5H、周额度及 R 列之间留 4ch 空隙；百分比右对齐，分隔点与倒计时各自使用跨行共享列。混合套餐的周额度在同一列，全部仅周限额时仍隐藏 5H。圆球从 64 × 64 缩到 43 × 43（半径约 2/3），数值去掉约等号，接口 partial 状态继续保留。

折叠尺寸固定为 43 逻辑像素，避免高 DPI 下测量小数导致向上取整。原生验证允许系统物理像素取整带来的 1 逻辑像素差异，并在展开后的内容与原生尺寸一致时再继续检查。

### 0.2.1 更新

新增底栏 `>` 折叠与 64 × 64 可拖动圆球；点击圆球展开，原生窗口随布局缩放，使用透明背景消除方形角落。`now` 与名称统一字体及基线。账号与额度采用内容决定列宽，只有周限额时不保留 5H 列。

Rust 的 `quota_total` 模块按平台和订阅计算圆形悬浮窗额度，并提供独立推荐倒计时。双限额先剔除 H<5 / W<2，再计算 m×min(H,W/r)；纯周及混合账号统一换算到基础订阅单位，详见 DOMAIN.md。窗口与显示已独立成设置页；平台选项仅来自已勾选账号，失效时按账号排序自动回退并保存，订阅档位和估算比例可覆盖。设置窗口默认宽度为 740 逻辑像素，代理与自动刷新列缩窄。

适配器识别 primary 或 secondary 中的有效周限额及另一槽位 null/不存在；存在但损坏的槽位仍按未知处理。三个自带主题共用以上行为。

### 0.2.0 更新

项目现名 Chatgpt HUD。新增官方登录文件适配器 `adapters/source.rs`，优先发现 `CODEX_HOME/auth.json`（默认 `~/.codex/auth.json`），已选目录保持不变；系统凭据库与仅内存凭据暂不支持。`isCurrent` 标识上次扫描的官方文件身份，不表示所有打开会话的身份。

设置新增中英切换（默认英语）、每账号显示名称自动保存（`accounts.alias.update`）、离线接口网页入口和原生主题示例保存对话框。旧等待圆点已去掉，错误标记仍保留。新应用标识首次启动时只复制旧版设置及主题，不改写旧文件。

`accounts.alias.update` 接收 `{accountId, alias}`，最多 80 字符，空值恢复原名，不更改选择、不取消刷新；结果为最新 Settings。不采用其他设置写入的 expectedRevision，因为只更新单个别名。仅设置窗口可调用。公开方法共 17 个，HUD 仍为 9 个。

## 1. 当前范围

第一版是 Windows 桌面应用，底层为 Tauri 2 和 Rust，界面为 React / TypeScript。提供配额悬浮窗、独立设置窗口和系统托盘，不使用 Electron。

| 能力 | v0.2.3 状态 |
| --- | --- |
| Cockpit 账号发现 | 自动发现或手动选择目录；扫描索引和详情以识别标准 OAuth 登录账号，排除 API 模式；刷新时重读所选账号凭据 |
| 存储读取 | 已实现已识别的明文详情与 AES-256-GCM 加密 envelope v1；只读取已有 key |
| 配额查询 | 标准 OAuth；固定 HTTPS 配额端点；没有真实账号联调记录 |
| 多账号 HUD | 5h / 7d 剩余量、分段颜色、倒计时、可用账号与可用重置次数 |
| 账号显示 | 选择、排序、别名接口、隐私模式；稳定的不透明 HUD ID |
| 刷新 | 手动、定时、重置核实、任务查询、同组请求加入现有任务、冷却与退避 |
| 主题 | 三个内置主题、本地 JSON 导入、Schema/版本/可读性校验、合成数据预览和应用 |
| 窗口与托盘 | 托盘置顶勾选、设置、刷新全部、显示/隐藏、退出；HUD 宽高随内容调整，折叠圆球与展开、位置保存 |
| 开放接口 | 17 个版本化 Tauri IPC 方法与变更事件，仅供获授权的内置窗口使用 |
| 脱敏诊断 | 版本、来源状态、所选账号数、任务数和错误码 |
| 浏览器演示 | 合成账号与配额；不接触本机账号文件或真实配额服务 |

未实现：外部本机 HTTP / SSE 服务、文件实时监听、开机自启、历史持久化、低额度通知、其他 Provider、自动更新与自动跟踪上游。现已提供默认英语与简体中文界面及托盘语言切换。

## 2. 源码结构

```text
src/
  App.tsx          HUD、设置与事件后的快照同步
  api.ts           Tauri IPC 调用封装；浏览器模式转向演示适配器
  demo.ts          合成数据及演示操作
  themes.ts        主题类型与受控样式映射
src-tauri/src/
  lib.rs           IPC 权限校验、窗口、托盘、应用启动
  service.rs       状态、刷新任务、调度、配置更新与事件
  domain.rs        新鲜度、可用性与推荐规则
  model.rs         DTO 和结构化错误
  config.rs        独立设置文件与原子替换
  themes.rs        主题校验、导入、读取和列举
  smoke.rs         使用临时空账号验证原生 WebView 与 IPC 权限
  adapters/
    source.rs      官方 auth.json 只读读取及账号来源分派
    cockpit.rs     只读索引、凭据与加密存储适配
    usage.rs       固定端点请求和保守的响应解析
contracts/hud-api.ts       版本化领域契约
schemas/theme.schema.json  主题格式
public/theme-template.json 可导入的自定义示例
```

业务状态由 Rust 服务持有。前端不接收 access token、refresh token、解密 key、原始账号详情或原始 HTTP 响应。主题只映射白名单颜色、数字和布局字段，不执行代码。

## 3. 已实现的公开接口

接口版本为 `1.0`，传输方式为 `tauri`。方法名映射为 `hud_v1_` 加上将点替换为下划线的命令名，例如 `quota.snapshot.get` 对应 `hud_v1_quota_snapshot_get`。请求统一放在 Tauri invoke 的 `request` 字段中。

| 领域方法 | HUD 窗口 | 设置窗口 | 行为 |
| --- | --- | --- | --- |
| `capabilities.get` | ✓ | ✓ | 当前窗口实际可调用的方法、版本与 Provider |
| `accounts.list` | ✓ | ✓ | HUD 只返回所选账号；设置窗口返回可选账号目录 |
| `quota.snapshot.get` | ✓ | ✓ | 来源、所选账号、配额、可用性、推荐、下次定时刷新 |
| `recommendation.get` | ✓ | ✓ | 当前推荐及覆盖情况 |
| `refresh.request` | ✓ | ✓ | 刷新全部所选账号或其中指定的一组，返回任务 ticket |
| `refresh.status.get` | ✓ | ✓ | 查询任务和逐账号进度 |
| `settings.get` | ✓ | ✓ | 设置与 `settingsRevision` |
| `themes.list` | ✓ | ✓ | 内置及有效的已导入主题摘要 |
| `window.control` | ✓ | ✓ | `show`、`hide`、`restore_position`、`open_settings` |
| `accounts.selection.update` | — | ✓ | 更新所选账号顺序与别名；成功后尝试刷新 |
| `settings.update` | — | ✓ | 更新刷新与显示设置 |
| `themes.validate` | — | ✓ | 校验拟导入主题，返回结构化问题 |
| `themes.preview` | — | ✓ | 校验预览文档，返回预览标识；界面使用合成数据渲染 |
| `themes.import` | — | ✓ | 打开本机文件选择器，校验并保存主题；导入不自动应用 |
| `themes.select` | — | ✓ | 应用已安装的主题 |
| `diagnostics.get` | — | ✓ | 返回脱敏诊断摘要 |

另有 4 个内置界面命令：`hud_internal_source_get`、`hud_internal_source_choose`、`hud_internal_source_rescan` 和 `hud_internal_theme_get`。前 3 个限定在设置窗口中使用；它们尚不属于稳定的外部接口契约。

公开响应包含 `apiVersion`、`instanceId`、`revision`、`requestId`、`generatedAt`，以及成功 `data` 或结构化 `error`。空参数方法接收 `{}`，请求拒绝未识别字段。账号选择、设置修改和主题应用需要当前 `expectedRevision`，并发修改返回 `CONFLICT`。

前端内的调用示例：

```ts
import { call, hud } from "./api";

const snapshot = await call("quota.snapshot.get", {});
console.log(snapshot.data.recommendation);

const ticket = await call("refresh.request", {});
const job = await call("refresh.status.get", { jobId: ticket.data.jobId });

const unsubscribe = await hud.subscribe((event) => {
  if (event.type === "snapshot.changed") {
    // 重新读取快照；事件不是完整状态。
  }
});
```

`hud.call` 返回成功或失败信封；`call` 在失败时抛出 `HudApiError`。变更通过 `hud://v1/event` 定向发送到内置窗口，事件携带实例、状态版本和序列号。前端先订阅再读取快照，用版本避免旧请求覆盖更新。

这些方法可以用于扩展内置前端。当前没有对外监听端口，也没有为外部进程提供可调用的 HTTP SDK；`docs/API.md` 中的 HTTP / SSE 和鉴权内容属于后续设计。

## 4. 存储和查询边界

Cockpit 存储参考基线是提交 `2d0f13f9e2b1c7a2b30bab08b3805829808aa85f`，不是对所有 Cockpit 版本的兼容承诺。

- 读取 `codex_accounts.json` 中 `version: "1.0"` 的索引；已提供的 `detail_schema_version` 必须为受支持值。
- 账号详情从 `codex_accounts/{id}.json` 读取，必须匹配索引中的账号 ID。对文件大小、账号数、路径边界和可接受标识进行检查。
- 加密格式限定为 envelope v1、`kind: codex`、`algorithm: AES-256-GCM`、已识别 `key_id`；key 来自同目录已有的 `secure-account-storage.key`。缺 key、认证解密失败或未知格式均显式报错。
- 列举读取并按需解密详情，确认是标准 OAuth 登录账号才暴露给前端；过期登录保留，API Key 等不支持的认证类型不导入。详情不可读时返回来源错误，保留已有选择，避免短暂文件问题导致设置丢失。凭据不进入前端和日志。
- 标准 OAuth 查询使用已有 access token 与账号标识，访问 `https://chatgpt.com/backend-api/wham/usage`。禁止 HTTP 重定向，设置连接和请求超时、响应大小上限；错误不回传服务端原始 body。
- 401 时最多重新读取一次凭据；只有 Cockpit 已经更新 access token 才重试。不执行 token 兑换、订阅修改、账号迁移或任何 Cockpit 文件写回。

HUD 的 `settings.json` 与 `themes/` 存放在 Tauri 按应用标识 `dev.chatgpt-hud.desktop` 解析的系统应用数据目录。设置保存采用同目录临时文件和原子替换。这里包含 HUD 的来源路径、ID 映射、显示选择、别名、刷新设置和窗口位置，不存储凭据副本。配额和刷新任务目前只保存在内存中，重启后重新查询。

## 5. 调度与领域行为

默认开启 300 秒定时刷新，可设为 60–1800 秒。自动刷新开启时，应用启动、定时到期、已知重置边界和时间异常均会尝试核实；关闭后这些事件不触发网络查询，仍会将旧数据保守标记为待核实。保存新账号选择后会尝试刷新，手动刷新始终可用。

每个账号只允许一个进行中的查询，全局最多 2 个并行查询。相同账号集合的重复请求会加入尚未结束的现有任务；与进行中任务仅部分重叠的请求暂时返回 `RATE_LIMITED`，没有实现跨任务共享单个子查询。常规冷却为 15 秒，失败按情况退避并遵守可解析的服务端 `Retry-After`。来源失效时最多每 30 秒重扫一次，手动重新扫描可立即核实。已结束任务保留约 30 分钟，上限 100 个。

切换来源、改变所选账号或系统时间明显跳变，会使已有任务代际失效；迟到结果不得覆盖新状态。数据新鲜度同时考虑成功采样时间和单调经过时间。过期、来源不可用、未知窗口和认证错误不形成肯定推荐。重置时间到达后仅触发重新查询，不能单凭倒计时推定已恢复额度。

基础窗口与功能窗口分开解释。剩余量来自合法的 `used_percent`；缺失的窗口、时长、百分比和重置时间不猜测补全。推荐规则实现在 `domain.rs`，详细语义见 [DOMAIN.md](DOMAIN.md)。

## 6. 主题实现

内置 ID 为 `default`、`midnight`、`paper`。自定义主题不允许使用这些 ID，也不允许覆盖已导入的相同 ID。导入文件不超过 64 KiB，限制嵌套深度、拒绝重复 JSON 字段，并检查 Schema、最低 HUD 版本与关键颜色对比度；最多支持 100 个自定义主题。

本版主题可配置颜色与字体，HUD 使用统一的紧凑间距；默认主题为奶油薄荷。HUD 采用紧凑账号行和底部推荐，可折叠为额度圆球，不渲染进度条、大卡片、额外状态栏或可展开详情。Schema 中其他布局字段保留兼容，但本版统一按紧凑行显示，不显示悬停提示，旧 showHoverDetails 设置字段仅保留接口兼容。主行按实际基础窗口展示，确认不适用时隐藏；不明确的缺失或损坏仍显示未知，附加功能窗口可通过快照接口读取。预览使用合成数据，不暴露真实账号。活动主题损坏或不可读取时，启动回退到默认主题。

复制 [public/theme-template.json](../public/theme-template.json) 即可开始。`examples/themes/midnight/theme.json` 的 ID 与内置主题重合，须修改 ID 后再导入。

## 7. 验证与后续验收

当前记录：

- `npm run build` 已通过 TypeScript 检查和前端生产构建。
- `cargo test --manifest-path src-tauri/Cargo.toml` 已通过 58 项测试，覆盖存储/加密/路径只读边界、响应解析、领域规则、主题校验、配置原子替换、账号选择，以及来源失效、时钟变化、任务取消和隐私输出的回归；新增覆盖 15% 拉伸边界、超过 100% 总量、微小正余额、部分/未知总量、零总量恢复时间及两个槽位的周限额兼容。
- 浏览器合成演示已检查刷新进度、账号选择和排序保存、主题导入、预览与应用、隐私模式；0.1.2 复查了三个内置主题的分段色、无悬浮提示、底栏设置入口和刷新全部。
- Windows x64 Release EXE 与 NSIS 安装包已生成，交付文件及 SHA-256 在 `artifacts/`。EXE 约 12.2 MiB，安装包约 3.3 MiB。
- 原生检查覆盖 HUD/设置页面与资产加载、CSP、9/17 方法与 6/9 scope、HUD 写操作拒绝、设置提交、主题切换，以及 43 × 43 折叠、展开恢复内容尺寸、透明背景及未知总量展示。报告见 [native-smoke.json](../artifacts/native-smoke.json)。使用临时空账号，无真实配额查询。
- 没有使用真实 OAuth 凭据联调，安装与卸载流程尚未执行验收。

原生检查同时覆盖通过公开 IPC 打开设置、销毁后通过托盘工作线程重新打开，以及置顶设置、原生勾选与窗口实际状态的一致性。

下一步验收重点是托盘图标实际鼠标交互、安装/卸载、不同 Windows/WebView2 版本与显示缩放，以及用户已有 Cockpit 版本的本机查询。上游内部查询端点或存储格式发生变化时，应更新适配器和合成测试后发布应用，而非运行时自动导入上游代码。

许可证、来源和后续同步方式见 [UPSTREAM.md](UPSTREAM.md)。本版为独立实现，没有复制上游代码，也没有创建自动跟踪上游的计划任务。

## 0.1.2 补充约定

- 配额快照新增可选字段 `resetCreditsAvailable`（非负安全整数或 null），来源为 usage 响应的 `rate_limit_reset_credits.available_count`。0 显示 R: 0，缺失/负数/非整数/超出安全整数范围不显示；该字段不改变可用性判断，HUD 不执行重置操作。
- 配额色阈值为 20 / 50 / 80，边界归入后一档；深浅主题采用各自可读的红、橙、蓝、绿色。
- HUD 默认宽 390，三账号演示高度 115；紧凑底栏写作用户指定的 `avaliable`。设置、刷新全部、隐藏按钮不触发拖动。
- `skipTaskbar: true` 仅配置于 HUD，设置窗口显式保持任务栏入口。
- 新增测试覆盖明文/加密详情的 API 账号排除、过期 OAuth 保留，以及重置次数 0、缺失和异常值。

## 0.1.3 推荐门槛与字号

设置页正文 14px、辅助文字 13px；HUD 的 R 组与额度同为 Cascadia Mono / Consolas 12px、600 字重。
`domain::evaluate` 在保留服务端原始额度/耗尽标记的同时应用本地推荐门槛：实际时长 18000 秒的基础窗口需剩余至少 5%，604800 秒需至少 2%。低于门槛但未耗尽返回 `waiting / below_threshold`；全部相关窗口恢复的估计取最晚重置时间。时间未知保持 unknown，到达 reset 也必须重新查询确认。真实耗尽仍使用 `quota_exhausted`。只有满足门槛且来源可靠的账号参与 now 推荐，再按周重置时间排序。
新增两个测试覆盖严格低于与恰好等于门槛、双窗口等待、与真正耗尽组合、低余额账号周重置更早也不优先、到点不自动恢复。
