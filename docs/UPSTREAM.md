# Cockpit 兼容、源码复用与同步

核对日期：2026-10-05。目标上游为 [jlcodes99/cockpit-tools](https://github.com/jlcodes99/cockpit-tools)，不是同名 Linux Cockpit 项目。固定参考提交为 `2d0f13f9e2b1c7a2b30bab08b3805829808aa85f`，包版本 `1.3.65`，提交时间为 2026-10-01 17:54:42 UTC。

本次只审阅公开源码，未读取本机真实账号、未向配额服务发送认证请求，也未复制上游实现。以下是实现依据和待验证边界，不是已通过的兼容声明。机器可读记录见 [upstream-baseline.json](upstream-baseline.json)。

## 1. 能否直接使用源码

当前上游 README 声明默认采用 **CC BY-NC-SA 4.0**，并明确商业用途需作者书面授权；`cockpit-core` 清单也使用此许可证。复制/改编需要满足署名、非商业、相同方式共享等适用条件，不能把它当作 MIT/Apache 库纳入任意发布方式。[上游声明](https://github.com/jlcodes99/cockpit-tools/blob/2d0f13f9e2b1c7a2b30bab08b3805829808aa85f/README.md#许可证)、[core 清单](https://github.com/jlcodes99/cockpit-tools/blob/2d0f13f9e2b1c7a2b30bab08b3805829808aa85f/crates/cockpit-core/Cargo.toml)、[CC BY-NC-SA 4.0 条款概览](https://creativecommons.org/licenses/by-nc-sa/4.0/)

本项目尚未选择发布许可证或商业模式。本次先制定自己的接口和领域设计，不导入上游代码。若以后采用其代码，应保存作者/来源/提交/许可和修改记录，按拟发布方式确认授权；若许可不匹配，寻求额外授权或采用独立实现路线。独立实现也不能据此推定完全免除协议、平台条款及其他权利问题。

“源码可见”“有可调用函数”“允许本项目按预期方式分发”是三个不同问题。即使许可满足，也还要检查依赖与副作用。

## 2. 复用边界

| 路线 | 评价 | 决定 |
| --- | --- | --- |
| 直接依赖整个 Cockpit / cockpit-core | 耦合账号管理与平台模块，且 core 与桌面实现不同步 | V1 不采用 |
| 复制完整配额刷新模块 | 包含 token 维护、订阅查询、存储等非 HUD 职责 | 不采用 |
| 许可允许后抽取最小查询/解析代码 | 可维护，需保留来源并单独验证 | 可选路线，不在本次导入 |
| 独立只读存储适配器 + 查询适配器 | 明确边界，靠合成 fixture 和协议证据验证 | 当前设计采用的实现路线 |
| Cockpit 提供脱敏快照 + 受控刷新桥接 | 耦合更少，由 Cockpit 管理凭据 | 未来协作方向，当前未发现可直接使用的 Codex 配额桥接 |

固定提交中，`refresh_account_quota` 会进入凭据准备/同步、可能的刷新、账号保存等流程；core 解析对缺失字段的处理也不等同于桌面解析。不能只按函数名判断它“只查询”，也不能仅同步 core 文件就宣称与桌面行为一致。[桌面查询模块](https://github.com/jlcodes99/cockpit-tools/blob/2d0f13f9e2b1c7a2b30bab08b3805829808aa85f/src-tauri/src/modules/codex_quota.rs)、[core 查询模块](https://github.com/jlcodes99/cockpit-tools/blob/2d0f13f9e2b1c7a2b30bab08b3805829808aa85f/crates/cockpit-core/src/modules/codex_quota.rs)

上游 token 维护还涉及运行中客户端持有旧凭据时的协调，HUD 不参与这套生命周期。文件 helper 也可能触发迁移或重新保存，因此 HUD 应只使用自己的只读实现。[token 维护](https://github.com/jlcodes99/cockpit-tools/blob/2d0f13f9e2b1c7a2b30bab08b3805829808aa85f/src-tauri/src/modules/codex_account_token_refresh.rs)、[账号读取与索引](https://github.com/jlcodes99/cockpit-tools/blob/2d0f13f9e2b1c7a2b30bab08b3805829808aa85f/src-tauri/src/modules/codex_account_index.rs)、[core 目录处理](https://github.com/jlcodes99/cockpit-tools/blob/2d0f13f9e2b1c7a2b30bab08b3805829808aa85f/crates/cockpit-core/src/modules/codex_account_core_provider.rs)

## 3. 存储兼容：原 README 的假设需要修正

固定基线的默认根目录为 `~/.cockpit_tools`，开发环境另用 `.cockpit_tools_dev`，支持 `COCKPIT_TOOLS_DATA_DIR`；旧 `.antigravity_cockpit` 路径可能通过兼容入口继续存在。HUD 只能发现和读取，不调用上游目录迁移逻辑。[数据目录说明](https://github.com/jlcodes99/cockpit-tools/blob/2d0f13f9e2b1c7a2b30bab08b3805829808aa85f/README.md#安全性与隐私简明版)

`codex_accounts.json` 是索引，账号详情位于 `codex_accounts/{id}.json`；模型已有 `detail_schema_version`，不能把索引摘要当作完整配额或凭据。HUD 需使用自己识别的格式指纹与支持矩阵，拒绝未知必需结构。[账号模型](https://github.com/jlcodes99/cockpit-tools/blob/2d0f13f9e2b1c7a2b30bab08b3805829808aa85f/src-tauri/src/models/codex.rs)

**当前详情可以是加密 envelope，而非明文账号 JSON。** 固定提交使用 AES-256-GCM，已有专门的 `read_account_file_readonly(path, key_path)` 路径；它直接读取已有文件/key 并解密，不走创建 key 的 helper。该函数是 crate 内接口，不是稳定对外 SDK。[加密存储实现](https://github.com/jlcodes99/cockpit-tools/blob/2d0f13f9e2b1c7a2b30bab08b3805829808aa85f/src-tauri/src/modules/secure_account_storage.rs)

独立只读适配器的验证目标：

- 明文详情：严格按已支持模型读取，忽略非必需机密字段，不保存副本。
- 加密详情：识别 envelope v1、算法与类型；只读同一根目录中的 `secure-account-storage.key`。key 为标准 Base64 编码的 32 字节，nonce 为 Base64 编码的 12 字节；ciphertext 按 AES-GCM 认证解密，解析最少必要字段。基线 kind 为 `codex`、key_id 为 `local-secure-account-storage-v1`；未知 kind / key_id / 版本不得猜测接受。
- 文件权限不足或 key 缺失：返回 `STORAGE_KEY_UNAVAILABLE`；不得新建或寻找其他用户的 key。
- 密文损坏、认证失败或 key/详情读取竞争：最多受控重读一致快照，仍失败返回 `STORAGE_DECRYPT_FAILED`；不得还原备份覆盖原文件。
- 未知版本：返回 `SOURCE_UNSUPPORTED`，提示适配器更新。不能将已识别的损坏 envelope 再当明文解析。

加密读取从协议上可行，但仍须在合成数据与目标 Windows 环境验证。当前格式未使用 DPAPI/Keychain 绑定；同一系统用户的文件权限决定能否读取 key 与详情，不代表所有未来版本都相同。实际实现不能直接复用任何含有建 key、目录迁移、轮换、恢复备份或保存行为的高层入口。

V1 主路径是 HUD 独立查询，不依赖 Cockpit 常驻。可选的 Cockpit 历史 quota 只作标明来源的旧值，不作为新查询或推荐依据。未来桥接若实现，则单独声明进程依赖与数据来源，不悄悄改变主路径。

## 4. 查询协议的参考范围

固定桌面实现使用 `GET https://chatgpt.com/backend-api/wham/usage`，OAuth 分支包含 bearer 与账号标识请求头，还处理客户端标识。这是被观察到的内部服务用法，不是本项目获得的稳定公共接口承诺。[固定查询源码](https://github.com/jlcodes99/cockpit-tools/blob/2d0f13f9e2b1c7a2b30bab08b3805829808aa85f/src-tauri/src/modules/codex_quota.rs)

HUD 第一版只做标准 OAuth 用途：

| 上游字段 | HUD 映射与约束 |
| --- | --- |
| `rate_limit.primary_window` / `secondary_window` | 基础 windows；缺失是否代表不适用需明确证据，否则 unknown |
| `used_percent` | 验证范围后计算 `remainingPercent = 100 - used_percent`，不以缺字段默认满额 |
| `limit_window_seconds` | 真实时长；缺失不自动补成 5h/7d |
| `reset_at` | 合法 Unix 秒优先转 UTC；不把毫秒误当秒 |
| `reset_after_seconds` | 仅绝对时间缺失且相对值合法时，以该响应采样时刻换算 |
| `allowed` / `limit_reached` | 结合窗口校验；矛盾或未知阻断不作肯定推荐 |
| 代码审查或其他功能窗口 | 独立 feature scope，不能误阻断全部基础配额 |

请求目标与必要头只由适配器控制；不把上游代理、reset-credit 消费、账号切换或订阅修改功能引进 HUD。HTTP 错误 body 只作白名单分类，不复制上游含原始响应文本的日志方式。

401 只重新读取已被 Cockpit 更新的 token，最多重试一次；绝不兑换 refresh token。429 遵守服务端退避。字段/headers 的兼容变化均作为应用适配器更新处理。

## 5. 当前并无可直接替代的公开桥接

已查到的 CLI `quota` 分支尚未实现；现有 WebSocket 的账号消息使用其他账号模型，不能据此假定存在 Codex 配额协议。未来若上游增加明确的只读导出与刷新 API，再用 `CockpitBridgeSource` 对接并验证其权限/版本。[CLI](https://github.com/jlcodes99/cockpit-tools/blob/2d0f13f9e2b1c7a2b30bab08b3805829808aa85f/crates/cockpit-cli/src/main.rs)、[WebSocket](https://github.com/jlcodes99/cockpit-tools/blob/2d0f13f9e2b1c7a2b30bab08b3805829808aa85f/src-tauri/src/modules/websocket.rs)

本项目自己的开放 HTTP / SSE 接口与 Cockpit 的接口无关；前者由 HUD 实现，不能解决未实现的上游桥接问题。

## 6. 如何持续跟进上游

下面是仓库 CI 的拟定工作流，本次没有创建计划任务或自动化：

1. 固定已审阅 SHA、文件清单、许可、适配器版本和测试状态；构建不依赖浮动 main。
2. 每周或开发者手动检查相关路径变化，生成差异报告或候选 PR，不自动合并；路径被移动/删除也要报警。
3. 先核对许可，再审查存储、网络头、字段、错误、token 生命周期和写入副作用；桌面与 core 两条实现都对比。
4. 用独立编写或获准使用的合成 fixture 更新适配器。禁止提交真实账号、key、token、原始抓包或自动从开发者数据目录取样。
5. 运行兼容/领域/只读/HTTP 错误测试；在隔离临时数据根上检查内容哈希及是否新增/删除文件，证明没有写回。
6. 评审通过后更新 reference baseline 与支持矩阵，记录可兼容 Cockpit 版本和限制，随经过验证的应用包发布；保留上一兼容版本的回归样本与回滚包。

用户端不 `git pull` 上游、不动态执行下载脚本、不自动加载未知 native 库。可以自动发现“需更新适配器”，不能自动把未审核方法投入运行。标准 fixture 验证不需要真实登录；真实联调作为最后的本机验收步骤，明确范围且不采集机密。

## 7. 发布前兼容矩阵

| 场景 | 当前结论 | 发布前要求 |
| --- | --- | --- |
| 历史明文账号 | 协议参考已审阅；HUD 未实现 | 合成文件、账号类型和路径边界验证 |
| 固定提交的加密详情 v1 | 只读兼容可行；HUD 未实现 | 加密 round-trip、缺 key、坏标签、并发写入、零写回验证 |
| 标准 OAuth 配额请求 | 请求/字段已查证；未实测 | mock HTTP 契约、受控本机联调 |
| API Key / Web Session / Agent Identity | V1 不支持 | 明确显示 unsupported，不乱发 OAuth 请求 |
| 未知未来文件/响应格式 | 不承诺兼容 | 失败可解释，旧数据标记失效，不显示正常满额 |
| 上游 Codex 桥接 | 未发现可用公开契约 | 等明确协议与版本，再单独评估 |

## 0.1.2 补充核对

同一固定基线的 [配额解析](https://github.com/jlcodes99/cockpit-tools/blob/2d0f13f9e2b1c7a2b30bab08b3805829808aa85f/src-tauri/src/modules/codex_quota.rs#L1017) 从 usage 的 `rate_limit_reset_credits.available_count` 读取可用主动重置次数。本项目独立实现严格数字校验并映射为 `resetCreditsAvailable`；无需新增查询端点。账号索引摘要没有 auth_mode，因此过滤时读取详情判定认证类型，不能靠名称猜测。

## 当前与上游的关系

本项目是独立 Tauri 应用，通过自己的存储适配器读取 Cockpit 账号，再通过自己的配额适配器请求和解析 usage。未复制上游实现，没有把 cockpit-core 编译或动态加载为依赖，因此不属于直接 fork 后同步源码的产品。

上游更新时，可参考其新的请求字段、认证约定和响应结构，修改 `adapters/cockpit.rs` / `adapters/usage.rs`，补合成样本和回归检查，再发布 HUD 更新。查询层更新与主题、窗口、5% / 2% 本地推荐规则分离。不能直接替换整个上游模块：它可能同时刷新 token、写账号文件、管理订阅，且依赖上游内部类型。若直接复制代码，仍须遵守对应版本的许可要求。当前没有自动跟踪、下载或执行上游代码。
