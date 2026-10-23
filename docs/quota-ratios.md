# 额度显示与估算比例

核查日期：2026-10-23。

双周期账号的 5h 显示值使用 `min(原始 5h 百分比, 剩余周百分比 ÷ 比例)`，表示该 5h 窗口实际可用的百分比。两项满额时显示 100%；周额度为 0 时 5h 显示 0。例：ChatGPT 周额度剩余 4%、比例 15%，5h 最多显示约 27%。原始数据不修改。圆形悬浮窗和四平台总量继续由 `quota_total::calculate` 按周额度单位计算；账号行不直接展示折算后的周占比。

| 平台／池 | 默认 5h 占周额度 | 依据与边界 |
| --- | --- | --- |
| ChatGPT | 15% | 用户指定，沿用项目估算；不宣称官方固定比例 |
| Antigravity Claude/GPT 池 | 50% | 用户实测指定；与独立 Claude 平台分开 |
| Antigravity Gemini 3 池 | 25% | 沿用项目暂定估算；未查到官方固定比例，不能据此保证真实容量 |
| 独立 Claude Pro / Max 5× / Max 20× | 15% / 10% / 20% | 沿用项目订阅档位估算；官方未公布固定比例 |
| Grok | 不折算 | 官方共享周额度池；兼容接口返回的旧月周期，不构造 5h 窗口 |
| Gemini 4 Argon | 不折算 | 当前适配器仅接收其周窗口，不构造 5h 窗口 |

用户的自定义比例优先于默认值；Antigravity 的命名订阅档位使用上述池级比例，自定义设置适用于该物理账号的所有池。

官方资料：

- [Google Antigravity plans](https://antigravity.google/docs/plans)：5 小时与周限制同时存在，配额随容量与工作量变化，没有提供固定换算率。
- [Google Antigravity models](https://antigravity.google/docs/models)：分别展示 Gemini 与 Claude/GPT 的五小时、周额度。
- [Claude usage limits](https://support.claude.com/en/articles/11647753-how-do-usage-and-length-limits-work)：区分五小时会话与周额度，使用量受模型、任务等影响，没有提供固定换算率。
- [Grok usage FAQ](https://docs.x.ai/grok/faq)：付费产品共用一个周额度池，产品消耗占比不是独立额度。

取不到新数据时，沿用最近成功取得的窗口值与原始重置时刻；窗口携带原采样时间，不能因其他窗口刷新而冒充新值。桌面、独立监视屏与汇总数值的历史数据显示为 45% 不透明度；新测量恢复后正常显示。没有任何历史测量的窗口仍显示未知。历史测量不恢复账号的当前可用推荐资格。

本地 Antigravity 协议的账号字段核对参考：[ai-usagebar 的 GetUserStatus 响应解析](https://docs.rs/ai-usagebar/latest/src/ai_usagebar/antigravity/fetch.rs.html)。本项目独立实现查询前后邮箱核对，不缓存账号身份；不可验证时使用目标账号自己的认证查询。

Google 邮箱别名或邮箱变更：保存邮箱与本地探针不一致时，使用该账号自己的访问凭据请求固定的 Google UserInfo 端点，仅接受有 `sub` 和已验证邮箱的响应，再按当前邮箱重做查询前后校验。身份核验结果不跨查询缓存；失败时仍回退远程额度查询。参见 [Google OpenID Connect](https://developers.google.com/identity/openid-connect/reference)。
