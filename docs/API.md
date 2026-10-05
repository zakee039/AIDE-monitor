# 开放接口 V1

> 0.2.0 实际提供 17 个设置窗口 IPC 方法（HUD 为 9 个）。新增 `accounts.alias.update({accountId, alias}) -> Settings`，仅设置窗口可用，自动保存单个别名，空值恢复原名；`AccountSummary` 新增 `alias` 与 `isCurrent`。后文 HTTP / SSE 部分仍为未来设计，当前说明请见 [离线接口网页](../public/api.html)。

v0.1.0 已实现本文 16 个公开 Tauri IPC 方法，调用适配见 [src/api.ts](../src/api.ts)。HTTP / SSE、外部授权、DTO 自动生成和部分重合任务共享仍待实现；当前差异见 [当前实现](IMPLEMENTATION.md)。类型见 [hud-api.ts](../contracts/hud-api.ts)，后续以 Rust DTO 生成类型并在 CI 检查漂移。

## 1. 开放哪些能力

除刷新与配额，V1 还开放账号显示信息、推荐与解释、数据源健康状态、刷新进度、主题切换、显示设置、窗口操作和脱敏诊断。通知规则、配额历史、更多 Provider 列入后续能力，不提前宣称可用。

开放接口不等于公开凭据。所有快照都没有 token、key、完整邮箱、源文件正文、原始上游响应或任意本地路径。名称使用用户别名或短名，仍可能有辨识性；隐私模式下改为“账号 1”等中性名称，HTTP 输出也遵守隐私模式。

## 2. 传输与方法映射

Tauri command 名称统一为 `hud_v1_` 加逻辑方法名的下划线形式，例如 `refresh.request` → `hud_v1_refresh_request`；参数固定放在 `request` 对象中，空参数为 `{}`。command 使用显式权限，不直接暴露通用 `invoke(methodName, payload)` 任意分发入口。

本机 HTTP 是 V1 后段里程碑的可选功能，默认关闭。它与 IPC 使用相同 DTO / 错误语义，不访问 Tauri 内部路由。V1 URL 前缀 `/v1`，JSON 请求字段为 camelCase。

| 逻辑方法 | IPC 调用方 | 本机 HTTP | scope / 说明 |
| --- | --- | --- | --- |
| `capabilities.get` | HUD / 设置 | `GET /v1/capabilities` | 任意有效 token；返回当前调用方能力 |
| `accounts.list` | HUD / 设置 | `GET /v1/accounts` | `quota.read`；HTTP/HUD 只返回已选账号，设置页可列出全部 |
| `accounts.selection.update` | 设置 | 不开放 | 完整替换 HUD 显示选择、顺序与别名；不切换上游账号 |
| `quota.snapshot.get` | HUD / 设置 | `GET /v1/snapshot` | `quota.read`；获取缓存状态，不触发网络 |
| `recommendation.get` | HUD / 设置 | `GET /v1/recommendation` | `quota.read`；结果与快照一致 |
| `refresh.request` | HUD / 设置 | `POST /v1/refresh` | `quota.refresh`；只查询已选账号 |
| `refresh.status.get` | HUD / 设置 | `GET /v1/refresh/{jobId}` | `quota.refresh`；HTTP 仅可读本 token 创建/加入的任务 |
| `settings.get` | HUD / 设置 | `GET /v1/settings` | `settings.read`；仅公开安全字段 |
| `settings.update` | 设置 | `PATCH /v1/settings` | `settings.write`；仅类型白名单内字段 |
| `themes.list` | HUD / 设置 | `GET /v1/themes` | `themes.read`；只返回安装元数据 |
| `themes.validate` | 设置 | 不开放 | 校验 JSON，无副作用 |
| `themes.preview` | 设置 | 不开放 | 虚构数据预览，不永久修改设置 |
| `themes.import` | 设置 | 不开放 | 打开宿主文件选择器，无路径字符串参数 |
| `themes.select` | 设置 | `PUT /v1/theme` | `themes.write`；只能选择已安装 ID |
| `window.control` | HUD / 设置 | `POST /v1/window` | `window.control`；show/hide/restore_position/open_settings |
| `diagnostics.get` | 设置 | `GET /v1/diagnostics` | `diagnostics.read`；只有脱敏状态摘要 |
| 事件订阅 | HUD / 设置 | `GET /v1/events` | `events.read`；需要配合相应读取 scope 使用 |

后台能力均固定白名单。自动启动设置、Cockpit 根目录选择、本机接口启停/授权、日志导出、退出应用属于设置页受信任宿主动作，另设受限内部命令，不是外部接口。表中 IPC 权限须逐项在 capability 中声明。

## 3. 返回格式与版本

统一 envelope：`apiVersion`、`instanceId`、`revision`、`requestId`、`generatedAt`、`ok`，成功带 `data`，失败带 `error`。`revision` 是进程内单调递增安全整数；`instanceId` 在重启后变化，旧 revision 不可跨进程比较。UTC 时间一律 RFC 3339；倒计时属于显示值，不写进权威数据。

```json
{
  "apiVersion": "1.0",
  "instanceId": "hud-session-example",
  "revision": 42,
  "requestId": "req-example",
  "generatedAt": "2026-10-05T02:00:00Z",
  "ok": true,
  "data": { "jobId": "job-example", "joined": false }
}
```

读取请求返回同一事务快照。设置修改要求 `expectedRevision` 等于 `settingsRevision`；冲突返回 `CONFLICT` 和提示客户端重新读取，不盲目覆盖。账号选择和主题切换也增加 settingsRevision。

所有 HTTP 响应设置 `Cache-Control: no-store`，不允许代理/浏览器缓存账户快照或错误；快照缓存仅指 HUD 的内部内存仓库。

V1 增加可选字段必须保持旧字段含义，客户端容忍响应中的新增字段。请求拒绝未知字段、重复键、非有限数、越界数字与不合法字符串。新增枚举值或改变必填语义需新主版本或先能力协商；不依赖客户端猜测。协议版本、应用版本、主题 Schema 版本彼此独立。

## 4. 配额与刷新语义

快照只含已选账号，每个账号有独立的成功/尝试时间、窗口列表、可信度、错误和可用性。未知字段用 `null` 与明确状态表达，不用 0 或 100 占位。窗口的基础/功能范围、完整性和 `blockingReason` 由 Provider 归一化，推荐逻辑以 [DOMAIN](DOMAIN.md) 为准。

`refresh.request` 的 `accountIds` 缺省表示所有已选账号；显式空数组、重复 ID、未选或不存在 ID 返回 `INVALID_ARGUMENT`，零已选账号也返回可解释空目标错误。每批上限 100，实际支持上限由 capabilities 返回；设置页亦限制最多选择 100 个。外部调用方不能传 URL、token、文件路径、`force` 或伪造内部刷新原因。

接受任务时 HTTP 返回 202，立即返回 jobId。任务可查询、按账号增量完成，并有 completed / partial / failed / cancelled 终态。相同集合且在途时加入原任务；部分重合共享子任务。退避中的账号可保留 queued，但若整个请求都受冷却阻挡，应返回 `RATE_LIMITED + retryAt`，不排无限队列。任务排队最长 60 秒，未启动项终结为失败并注明 retryAt；长 Retry-After 交给之后的刷新请求。

作业最多保留 100 个且终态保留 30 分钟，先淘汰终态，不淘汰在途；达到容量拒绝新任务。外部 `Idempotency-Key` 可选，按 token + key 保存 10 分钟，同 key 不同请求体返回 409，防止断线重试重复调度。无论幂等记录是否过期，每账号在途去重始终生效。

失败不清空旧百分比，但错误必须可见。刷新重试开始时保留上次失败信息，直到成功才清除；不能通过把 status 改为 refreshing 让失败账号重获推荐资格。

## 5. 事件与断线恢复

事件有独立 `sequence`、`instanceId`、状态 `revision`、UTC 时间和类型。事件是变更提示，权威数据通过 GET / command 获取，避免主题或客户端把零散事件拼成不一致状态。

| 事件 | 含义 |
| --- | --- |
| `snapshot.changed` | 配额、推荐或时间可信度变化，读取最新完整快照 |
| `refresh.progress` / `refresh.completed` | 刷新阶段变化，可带 jobId；HTTP 只推送本 token 的作业 ID |
| `source.changed` | 数据源健康或账号目录变化 |
| `settings.changed` / `theme.changed` | 对应设置提交成功 |
| `resync.required` | 新订阅、重连或队列丢失，需要全量重读 |

IPC 采用定向窗口事件 `hud://v1/event`，不广播原始账号信息。HTTP 使用 SSE，事件 `id` 为 `instanceId:sequence`。每个订阅有独立递增 sequence，按 scope 过滤，不从间断序号推断其他授权流量。

V1 不承诺事件重放：每次 SSE 重连或 IPC 重订阅立即发 `resync.required`，包括带 Last-Event-ID 的请求。SDK 先注册监听并缓冲通知，再读快照；若缓冲中最高 revision 更新则重读，直到应用的数据不落后。忽略旧 instance 的通知与低 revision 数据。队列上限 128，合并重复快照提示；溢出发 resync.required，慢客户端仍阻塞则断开。SSE 每 15 秒 heartbeat，不包含配额。

快照的 freshness 到期或 reset 到点由 Rust 时钟任务更新并发事件，即使没有网络返回也会改变推荐。UI 每秒只更新倒计时；发现跨界但尚未收到通知时读取快照，后端读取前同步评估时间有效性。

## 6. 本机接口授权

- 默认关闭，只在设置页主动启用；仅绑定 `127.0.0.1` 随机可用端口，不绑定局域网或 `0.0.0.0`。实际地址在设置页显示，不扫描其他服务端口。
- 每个集成单独创建至少 256 位随机 bearer token，只显示一次，存摘要并支持撤销和过期；与 Cockpit access token 完全独立。默认授予 quota.read + events.read，刷新和写操作单独勾选。
- 所有 HTTP 路径包括能力、诊断和事件均需 `Authorization: Bearer …`；token 不放 URL、查询参数、日志、localStorage、事件或文件名。比较摘要采用恒定时间方法。
- V1 外部客户端为本机脚本/工具。拒绝所有带 Origin 的 HTTP 请求，不开放浏览器 CORS；严格检查 Host 为实际 loopback 地址和端口，防止把回环绑定当成完整防护。HTTP 不使用 Cookie。浏览器原生 EventSource 无法方便携带 bearer，调用者应使用支持 header 的 SSE 客户端。
- 读取限速每 token 60 次/分钟，刷新每 token 6 次/分钟；底层每账号冷却仍生效。每 token 最多 2 个 SSE 连接、全局最多 8 个；请求体上限 64 KiB。限流也返回 retryAt。
- token 不存在/失效返回 401，缺 scope 返回 403。停用接口或撤销 token 时立即断开相关 SSE，并取消尚未执行的专属任务引用；共享子任务可为其他授权调用方继续。

V1 的 HTTP 设计只保证应用边界内的访问控制，不声称能隔离同一系统用户下所有恶意进程。Provider 网络请求仍只发往代码内固定 HTTPS 目标，默认禁重定向；无法由主题或 API 修改域名、头或代理脚本。

## 7. 错误与客户端示例

错误 code 是稳定接口，message 是脱敏提示。HTTP 使用 400（参数）、401（API 授权）、403（scope）、404、409、429、500 等；上游查询失败通常体现在已接受任务的账号结果里，不能把上游 401 混成调用 HUD API 的 401。

存储缺 key 用 `STORAGE_KEY_UNAVAILABLE`，认证标签/解密失败用 `STORAGE_DECRYPT_FAILED`，未知格式用 `SOURCE_UNSUPPORTED`。原始路径和解密材料不放进错误 message。`retryable` 表示在条件满足后可重试，需结合 retryAt，不表示立刻循环请求。

以下为使用形状；v0.1.0 可从 `src/api.ts` 导入 `hud`，前端已实现订阅和快照恢复：

```ts
const subscription = await hud.subscribe(event => {
  if (event.type === "snapshot.changed" || event.type === "resync.required") {
    // Real SDK coalesces reads and guards instanceId/revision.
    void hud.call("quota.snapshot.get", {}).then(renderSnapshot);
  }
});
const initial = await hud.call("quota.snapshot.get", {});
renderSnapshot(initial);
await hud.call("refresh.request", {});
// On unmount: subscription();
```

真正 SDK 应内置订阅/快照竞态处理，示例仅说明接口形状。错误 envelope 必须先判断 `ok`，不得把 error 当成空配额成功值。

## 0.1.2 兼容新增字段

`AccountQuota.resetCreditsAvailable?: number | null`：usage 返回的可用主动重置次数（非负安全整数）。缺失或无效时为 null；0 是有效值。该字段不改变可用性判断。`accounts.list` 现在仅返回已识别的标准 OAuth 登录账号，API 模式在来源扫描阶段排除。`display.showHoverDetails` 保留读写兼容，但不再控制 HUD 悬浮提示。

## 0.1.3 推荐门槛

AccountAvailability/Recommendation 的 reason 增加 `below_threshold`。5h < 5% 或 7d < 2% 不属于当前可推荐账号；有可靠 reset 时 waiting，否则 unknown。边界值可用，原始配额与 exhausted 字段不受本地推荐门槛影响。
