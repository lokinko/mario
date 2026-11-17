# API 调用契约与现有接口

本文从当前代码重新记录可调用接口，按用户、市场、AI 三域及支撑能力组织，供开发和查询使用。业务行为分别引用[用户 spec](specs/user.md)、[市场 spec](specs/market.md)、[AI spec](specs/ai.md)，代码归属见[架构](architecture.md)。文件名或 URL 前缀不改变功能归属。

这里记录实际实现，不将已认可但未实现的行为写成可用能力。HTTP 传输契约与传给模型的上下文格式是两个层次；当前 HTTP 使用 JSON，模型输入的精简要求及实现差异见 [AI spec](specs/ai.md#8-现有实现差异)。本文不恢复旧文档，也不建立第二套手工维护的 OpenAPI。

## 1. 阅读方式与公共约定

- 下表路径是后端路由，包含 `/api`。部署可能在反向代理下增加网站前缀；客户端基址由[服务地址配置](../client/src/lib/service.ts)决定，不能无条件拼接第二个 `/api`。Agent 使用自己的地址和无 `/api` 前缀的路径。
- `B` 表示 JSON 请求体，`Q` 表示查询参数，`—` 表示无需请求体。路径中的 `{id}`、`{symbol}`、`{attachment_id}`、`{mode}` 都是必填路径参数，调用者须 URL 编码；响应不统一包裹 `data`。
- 字段目录中的 `?` 表示可省略，`T/null` 表示可空；未注明可选的输入字段须提供。Rust `Option` 输入通常允许省略或 `null`，但业务校验可能要求特定组合。默认值、空字符串和 `null` 不能互相替代。
- 对外字段通常为 camelCase；枚举按各类型的实际序列化值传输。内部 Agent 的 `api_key`、`payload_bytes` 等保留 snake_case，不套用外部命名规则。
- 金额、股数、价格的精确字段使用十进制字符串；旧展示投影仍有 JSON number，不能笼统把所有金额都当 number 或 string。金额以对象的 `currency` 计价，汇总以 `baseCurrency` 计价；`Pct` 是百分数，`Bps` 是基点。成交数量是证券单位，期权数量是合约数，合约另有 `multiplier`。
- 日历日期使用 `YYYY-MM-DD`；时间戳按类型区分日期时间字符串和 Unix 秒。`requestedOn` 是请求日期，`observedOn` 是数据实际日期，不保证相同。
- 普通成功响应为 HTTP 200 JSON；记忆删除为 204 空响应，OAuth 回调为重定向，流式分析为 SSE。JSON 请求须带 `Content-Type: application/json`。
- 请求体上限：托管接口总体 64 MiB，登录/OAuth/活跃度/看板/服务信息所在路由组 16 KiB，研究资料上传与桥接 Worker 为 8 MiB，独立 Agent 为 8 MiB。上限是传输层限制，不代替附件和字段自身校验。来源：[托管装配](../server/src/hosted.rs)、[研究 HTTP](../server/src/http/evidence.rs)、[桥接 HTTP](../server/src/http/codex_bridge.rs)、[Agent](../server/src/agent.rs)。

### 1.1 权限与版本记号

| 记号 | 调用要求 |
| --- | --- |
| 匿名 | 不要求账号 token；OAuth 仍可能要求 Origin、Cookie 与一次性状态 |
| 账号 | `Authorization: Bearer <session.token>`，仅访问当前账号的数据 |
| 管理 | 账号 token，且数据库中具有超级账户权限 |
| 桥接 | 桥接配对凭据，只能访问该桥接的任务与结果通道 |
| Agent | 服务间 `Authorization: Bearer <MARIO_AGENT_TOKEN>` |
| R | 必须提供账号当前 `If-Match`，另有实体版本时也须同时提供 |
| C | 请求体携带公共目录 `revision`，使用独立目录版本，不要求账号 `If-Match` |
| — | 不要求账号 `If-Match`；并不保证无缓存、认证或外部调用副作用 |

账号版本从 `GET /api/sync/version` 的 `revision` 或账号路由响应头 `x-data-revision` 取得，按原字符串发送 `If-Match`。不匹配返回 409，响应体为 `{error:string, revision:string}`；此提前返回分支不保证附带版本响应头。普通账号处理器返回后会附带 `x-data-revision`，包括部分业务失败。客户端不能假设每种错误都有该响应头。

接口表逐项标明当前版本要求。特别是成交修复预览虽不落账，当前仍要求 R；分析预览、模型测试、财务结果查询、公司解析、网格预览不要求 R。公共目录刷新要求 R，但不因此修改个人账本。依据：[版本策略](../server/src/http/policy.rs)、[账号中间件](../server/src/hosted.rs)、[客户端传输](../client/src/lib/httpClient.ts)。

### 1.2 错误与重试

业务 `AppError` 通常返回 `{error:string}`，状态含义见下表；[错误映射](../server/src/http/error.rs)会隐藏 500 的内部诊断。框架参数解析、请求体超限、路由未找到等响应不保证使用此 JSON 格式，应先检查状态与内容类型。

| 状态 | 当前含义与处理 |
| --- | --- |
| 400 | 输入或业务校验失败，修正字段后提交 |
| 401 / 403 | 登录失效或权限不足；重新取得授权，不能当网络错误反复重试 |
| 404 | 资源或能力不存在；未知 API 路径也可能为空 404 |
| 409 | 账号/实体/目录版本冲突或状态冲突；刷新核对，保留用户输入，不自动覆盖 |
| 429 / 503 | 登录限流、服务繁忙或容量不足，可稍后重试；写入前仍需确认结果 |
| 502 | 模型、行情、网络或桥接上游失败 |
| 500 | 内部服务错误，细节只在服务端日志 |

财务写入超时、连接失败或 5xx 不等于未提交。使用 `POST /api/financial-operations/resolve`，传入原 `method`、完整后端 `path`（例如 `/api/holding-trades`）与原 `input`。返回 `{committed:boolean, snapshot:Snapshot/null}`；`false` 只表示未查到匹配回执，不保证此前请求不会稍后完成。保留原请求编号与内容，核对历史后按同一请求重试，不能创建另一笔交易。

支持回执的操作包括成交新增/修改/删除、持仓删除、余额与期权价格确认、成交修复及公司行动处理。部分操作有 `requestId`，其他操作根据路径、实体版本及输入核对，不要求调用者凭空新增字段。实体 `expectedRevision` 与账号 `If-Match` 不是同一个版本。账号版本过期时，中间件会先尝试返回这些操作已提交的结果。来源：[财务回执](../server/src/db/financial.rs)、[客户端恢复](../client/src/api/portfolio/financialRequest.ts)。其他接口没有全局幂等键或自动重试保证。

## 2. 用户域接口

### 2.1 账号与数据

实现：[托管 HTTP](../server/src/hosted.rs)、[OAuth HTTP](../server/src/hosted/openai.rs)、[账号契约](../server/src/accounts.rs)、[备份格式](../server/src/db/sync_format.rs)。

| 方法 | 路径 | 权限 / 版本 | 输入 → 响应 | 作用与限制 |
| --- | --- | --- | --- | --- |
| POST | `/api/auth/register` | 匿名 / — | B `Credentials` → `Session` | 创建账号与会话 |
| POST | `/api/auth/login` | 匿名 / — | B `Credentials` → `Session` | 创建会话 |
| POST | `/api/auth/logout` | 账号 / — | — → `{ok:true}` | 撤销当前会话，不经账号版本中间件 |
| GET | `/api/auth/openai/config` | 匿名 / — | — → `{enabled:boolean}` | 网站登录可用性，与模型调用授权分开 |
| POST | `/api/auth/openai/start` | 条件授权 / — | B `{link:boolean}` → `{url:string}` | 校验网站 Origin；`link:true` 另需账号 token；设置登录 Cookie |
| GET | `/api/auth/openai/callback` | OAuth 状态 / — | Q `state, code`，可有 `iss/error` → 303 跳转 | 校验浏览器 Cookie 与一次性状态，结果通过网站 fragment 和 Cookie 交接 |
| POST | `/api/auth/openai/complete` | OAuth Cookie / — | — → `{linked:boolean, session:Session/null}` | 校验 Origin，领取一次性结果并清 Cookie |
| GET | `/api/sync/version` | 账号 / — | — → `{revision:string}` | 当前账号版本 |
| GET | `/api/data/export` | 账号 / — | — → `SyncDataset` | 个人业务备份，不是整站数据库备份 |
| POST | `/api/data/import` | 账号 / R | Q `replace?:boolean=false`；B 备份或支持的外部自选格式 → `{ok:true}` | 默认只导入空账号；替换须显式 `replace=true`，不是合并账本 |

OAuth 的 `url` 用于浏览器跳转，不是模型 API endpoint；回调后的 `session:null` 表示身份绑定流程，不是返回一个空登录会话。备份替换前下载原备份是客户端流程，API 不会替调用者保存一份下载文件。支持的导入转换见[账号导入](../server/src/db/sync_store.rs)、[外部自选转换](../server/src/db/watchlist_import.rs)。

### 2.2 财务事实、持仓、成交与估值

下表全部为账号权限；实现与输入解析见[财务 HTTP](../server/src/http/portfolio.rs)，前端调用见[财务客户端](../client/src/api/portfolio.ts)。

| 方法 | 路径 | 版本 | 输入 → 响应 | 作用与限制 |
| --- | --- | --- | --- | --- |
| GET | `/api/snapshot` | — | — → `Snapshot` | 账号财务总览、持仓、账本投影、风险与规划 |
| PUT | `/api/profile` | R | B `FinancialProfile` → `Snapshot` | 保存财务档案 |
| POST | `/api/holdings` | R | B `HoldingInput` → `Snapshot` | 当前只允许新增现金持仓；投资持仓通过新增成交建立 |
| PUT | `/api/holdings/{id}` | R | B `HoldingInput` → `Snapshot` | 更新非期权持仓，可补取报价；不能据此绕过账本业务约束 |
| DELETE | `/api/holdings/{id}` | R | B `{expectedRevision?:string}` → `Snapshot` | 删除持仓；传输层允许空体，实体校验仍可能拒绝 |
| PUT | `/api/holdings/{id}/amount` | R | B `AmountInput` → `Snapshot` | 确认现金余额，生成财务记录 |
| PUT | `/api/holdings/{id}/option-price` | R | B `OptionPriceInput` → `Snapshot` | 确认期权估价 |
| PUT | `/api/holdings/{id}/verified-valuation` | R | B `{symbol:string,quantity:number,onDate:string}` → `Snapshot` | 从市场数据核验并保存估值 |
| POST | `/api/holding-trades` | R | B `HoldingTradeInput` → `Snapshot` | 新增成交，按结算模式更新账本 |
| PUT | `/api/holding-trades/{id}` | R | B `HoldingTradeUpdateInput` → `Snapshot` | 更正成交 |
| DELETE | `/api/holding-trades/{id}` | R | B `{expectedRevision:string}` → `Snapshot` | 删除成交并按账本规则重算 |
| GET | `/api/holdings/{id}/financial-history` | — | — → `FinancialHistory` | 更正审计与隔离记录 |
| POST | `/api/holdings/{id}/trade-repair/preview` | R | B `TradeRepairInput` → `HoldingTradeSummary` | 预览修复，不落账；当前仍须账号版本 |
| POST | `/api/holdings/{id}/trade-repair` | R | B `TradeRepairInput` → `Snapshot` | 提交修复与原因 |
| POST | `/api/holdings/{id}/corporate-actions` | R | B `ManualCorporateActionInput` → `Snapshot` | 手工声明拆并股事件 |
| POST | `/api/corporate-actions/{id}/{mode}` | R | B `{expectedRevision:string,reason:string}` → `Snapshot` | `mode` 为 `confirm/reject/revert`，分别确认、拒绝、撤销已应用事件 |
| POST | `/api/financial-operations/resolve` | — | B `{method:string,path:string,input:object}` → `{committed:boolean,snapshot:Snapshot/null}` | 查询原请求回执，不创建新交易 |
| GET | `/api/daily-assets` | — | Q `from?,to?,before?:string; limit?:integer` → `DailyHistory` | 日期分页，默认 30 条，limit 限制在 1–400，下一页用 `nextBefore` |
| POST | `/api/daily-assets/ensure` | R | B `{timezone:string}` → `DailyHistory` | 确保每日记录，首次保存 IANA 时区，之后不因设备时区变化自动切换 |
| GET | `/api/daily-assets/compare` | — | Q `from:string,to:string` → `DailyComparison` | 比较两个日期的资产状态 |
| GET | `/api/portfolio/allocation` | — | — → `Allocation` | 按资产类别与行业汇总 |
| GET | `/api/portfolio/dividends` | — | — → `DividendIncome` | 持仓分红收入投影，可能读取市场来源 |
| GET | `/api/fx-settings` | — | — → `FxBook` | 账号汇率设置及现有报价 |
| PUT | `/api/fx-settings` | R | B `{currency:string,rate?:number/null,exactRate?:string/null}` → `FxBook` | 保存手工汇率；删除时 rate 为空且不得附精确值；提供 exactRate 时也须提供 rate，非法数值会报错，见[汇率仓储](../server/src/db/fx.rs) |
| POST | `/api/fx-settings/refresh` | R | — → `FxBook` | 刷新账号使用的汇率数据 |

### 2.3 观察列表、公共目录与日历

实现：[观察 HTTP](../server/src/http/watch.rs)。以下均需账号；仅公共目录保存和公司解析额外要求管理权限。公共目录、分红日历与观察计算仍是用户功能，所消费的公共行情由市场域提供。

| 方法 | 路径 | 版本 | 输入 → 响应 | 作用与限制 |
| --- | --- | --- | --- | --- |
| GET | `/api/watch` | — | — → `WatchBoard` | 个人观察列表 |
| PUT | `/api/watch` | R | B `{symbol:string,name:string,thesis?:string="",ifAbsent?:boolean=false}` → `WatchEntry` | 新增或保存，`ifAbsent` 防止覆盖已有条目 |
| DELETE | `/api/watch/{symbol}` | R | — → `{ok:true}` | 删除个人条目 |
| GET | `/api/watch/{symbol}/observation` | — | — → `WatchRow` | 个人标的详情 |
| POST | `/api/watch/{symbol}/read` | R | — → `{ok:true}` | 标记提醒已读 |
| PUT | `/api/watch/{symbol}/pin` | R | B `{pinned:boolean}` → `WatchEntry` | 设置置顶 |
| PUT | `/api/watch/{symbol}/grid` | R | B `GridPlan/null` → `WatchBoard` | 保存或清除网格方案，不创建成交 |
| POST | `/api/watch/grid-preview` | — | B `GridPlan` → `GridProjection` | 计算网格，不保存 |
| POST | `/api/watch/refresh` | R | — → `WatchBoard` | 刷新个人列表使用的观察数据 |
| PUT | `/api/watch/rules` | R | B `{rules:WatchRules,expectedRules:WatchRules}` → `WatchBoard` | 保存规则，同时比对旧规则 |
| GET | `/api/watch/catalog` | — | — → `CatalogBoard` | 公共目录，叠加当前账号规则与 `canEdit` |
| PUT | `/api/watch/catalog` | C；管理 | B `{catalog:Catalog,revision:integer}` → `CatalogBoard` | 用读取到的目录版本保存 |
| GET | `/api/watch/catalog/{symbol}/observation` | — | — → `CatalogRow` | 公共公司详情，按账号规则展示 |
| POST | `/api/watch/catalog/refresh` | R | B `{industryId:string}` → `CatalogBoard` | 刷新指定行业公共数据，不复制成个人历史 |
| POST | `/api/watch/catalog/resolve` | —；管理 | B `{query:string}` → `Company` | 解析公司信息，不自动加入目录 |
| GET | `/api/watch/dividend-calendar` | — | Q `scope?:all/favorites/public=all; from?:date` → `CalendarBoard` | 默认从服务的当前日期查看，从市场计划缓存只读生成账号日历与覆盖情况，不触发采集 |

### 2.4 研究资料、方法论、规则与复盘

下表均需账号。来源：[研究 HTTP](../server/src/http/evidence.rs)、[方法论 HTTP](../server/src/http/methodology.rs)、[复盘 HTTP](../server/src/http/reviews.rs)。

| 方法 | 路径 | 版本 | 输入 → 响应 | 作用与限制 |
| --- | --- | --- | --- | --- |
| GET | `/api/research-evidence` | — | — → `ResearchEvidence[]` | 查询独立研究资料库 |
| POST | `/api/research-evidence` | R | B `ResearchEvidenceInput` → `ResearchEvidence` | 保存原文、标签及附件 |
| GET | `/api/research-evidence/{id}/attachments/{attachment_id}` | — | — → `{name:string,dataBase64:string}` | JSON 中返回附件内容，不是原始二进制下载 |
| PUT | `/api/research-evidence/{id}/status` | R | B `{active:boolean}` → `ResearchEvidence` | 启用或归档 |
| PUT | `/api/research-evidence/{id}/tags` | R | B `{assetName?:string,category?:string}` → `ResearchEvidence` | 标签字段默认空字符串，按保存语义提交，不当作局部 PATCH |
| GET | `/api/investment-methodology` | — | — → `InvestmentMethodology` | 当前方法论、默认文本及长度限制 |
| PUT | `/api/investment-methodology` | R | B `{prompt:string}` → `InvestmentMethodology` | 保存方法论 |
| GET | `/api/investment-rules` | — | — → `InvestmentRule[]` | 手工规则 |
| POST | `/api/investment-rules` | R | B `InvestmentRuleInput` → `InvestmentRule` | 新增规则 |
| PUT | `/api/investment-rules/{id}` | R | B `InvestmentRuleInput` → `InvestmentRule` | 更新并留修订记录 |
| GET | `/api/investment-rules/{id}/history` | — | — → `InvestmentRuleRevision[]` | 规则修订历史 |
| GET | `/api/system-reviews` | — | — → `SystemReviewRecord[]` | 复盘记录，含现有兼容字段 |
| POST | `/api/system-reviews` | R | B `{content:string}` → `SystemReviewRecord` | 保存文字复盘；旧记录字段不等于当前输入字段 |
| GET | `/api/review-reminders` | — | — → `{periodicReviewDue:boolean,checkedOn:string}` | 复盘提醒 |

## 3. 市场域接口

这些入口位于[财务 HTTP](../server/src/http/portfolio.rs)，由[市场用例](../server/src/application/portfolio.rs)提供能力。公共价格在底层复用，但 HTTP 入口均需账号 token。

| 方法 | 路径 | 版本 | 输入 → 响应 | 作用与限制 |
| --- | --- | --- | --- | --- |
| GET | `/api/market-data/security-price` | — | Q `{symbol:string,onDate:date,actionFrom?:date}` → `SecurityPriceQuote` | 查询报价，可取源并填充缓存；不是证券搜索接口 |
| GET | `/api/market-data/fx-rate` | — | Q `{fromCurrency:string,toCurrency:string,onDate:date}` → `FxRateQuote` | 查询换算报价，账号手工汇率可能参与结果 |
| POST | `/api/market-data/security/refresh` | R | — → `{snapshot:Snapshot,updatedCount:integer,errors:string[]}` | 以市场报价刷新当前账号持仓估值，兼有用户域写入作用；部分失败查看 `errors` |

证券代码使用[当前规范化实现](../server/src/models/market.rs)支持的格式，例如 `AAPL`、`0700.HK`、`600519.SS`；已支持的别名会规范化，同公司不同交易标的仍分别报价。当前报价含复权口径，不得用它改写真实成交价格；口径与来源看 `priceBasis`、`providerCode`、`observedOn`、`sourceUrl`。不存在独立公开的批量历史价格下载 API，观察详情中的历史字段按其响应契约使用。

## 4. AI 域接口

### 4.1 模型、分析与记忆

以下均需账号 token。来源：[设置 HTTP](../server/src/http/model_settings.rs)、[分析 HTTP](../server/src/http/analysis.rs)、[记忆 HTTP](../server/src/http/memory.rs)、[桥接管理 HTTP](../server/src/http/codex_bridge.rs)。

| 方法 | 路径 | 版本 | 输入 → 响应 | 作用与限制 |
| --- | --- | --- | --- | --- |
| GET | `/api/model-config` | — | — → `ModelConfig` | 当前配置，不返回明文模型密钥 |
| PUT | `/api/model-config` | R | B `ModelConfigInput` → `ModelConfig` | 保存个人配置或选择站点配置 |
| POST | `/api/model-config/test` | — | — → `{ok:boolean,model:string,latencyMs:number}` | 测试已保存配置，会调用模型 |
| GET | `/api/model-config/models` | — | — → `{models:string[],defaultModel:string}` | 站点模型目录 |
| DELETE | `/api/model-key` | R | — → `ModelConfig` | 移除个人密钥；站点密钥不能由个人移除 |
| GET | `/api/analysis-settings` | — | — → `AnalysisSettings` | 已保存的问答预设 |
| PUT | `/api/analysis-settings` | R | B `AnalysisSettings` → `AnalysisSettings` | 保存预设，不执行分析 |
| POST | `/api/analysis/preview` | — | B `AnalysisRequest` → `AnalysisPreview` | 准备授权上下文与指纹，不执行完整模型问答 |
| POST | `/api/analysis` | R | B `AnalysisRequest` → `AnalysisResult` | 执行并保存完整结果，无通用幂等重放保证 |
| POST | `/api/analysis/stream` | R | B `AnalysisRequest` → SSE | 流式执行，终态见下节 |
| GET | `/api/analyses` | — | — → `AnalysisHistoryItem[]` | 分析历史摘要 |
| GET | `/api/analyses/{id}` | — | — → `StoredAnalysis` | 已保存的回答与审计信息 |
| GET | `/api/memory` | — | — → `MemoryOverview` | 当前记忆与整理状态 |
| POST | `/api/memory/dream` | R | — → `MemoryOverview` | 执行记忆整理，可能调用模型并写入记忆 |
| PUT | `/api/memory/{id}/status` | R | B `{active:boolean}` → `MemoryNote` | 启用或停用记忆 |
| DELETE | `/api/memory/{id}` | R | — → 204 空响应 | 删除记忆 |
| GET | `/api/codex-bridge` | — | — → `CodexBridgeStatus` | 配对及在线状态 |
| POST | `/api/codex-bridge` | R | — → `{pairingCode:string,expiresInSeconds:600}` | 创建短期配对凭据 |
| DELETE | `/api/codex-bridge` | R | — → `{ok:true}` | 撤销桥接 |
| POST | `/api/codex-bridge/enable` | R | — → `ModelConfig` | 启用配对桥接作为模型配置 |

预览返回 `contextRevision`，执行请求必须用 `previewRevision` 携带匹配指纹；字段在反序列化层可省略，但执行时缺失或不匹配会失败。它表示授权输入与模型目标的准备状态，不替代 `If-Match`。客户端可自动准备再执行，不要求额外弹窗确认。分析执行与校验见[分析用例](../server/src/application/analysis.rs)。当前省略 `webSearch` 的运行默认值仍为 false；“默认开启网页搜索”已采纳但尚未实现，不能根据产品 spec 推断当前省略字段会搜索。当前也未提供通用 CSV 模型附件接口。

### 4.2 流式协议

SSE 内容类型为 `text/event-stream`，JSON 放在 `data:` 行中，以对象的 `type` 分派；没有为每种类型设置 SSE `event:` 名称。可以有保活注释，不是每个分片都含完整 JSON。来源：[事件类型](../server/src/models/inference.rs)、[SSE 适配](../server/src/streaming.rs)。

| type | 其他字段 | 含义 |
| --- | --- | --- |
| `status` | `message:string` | 执行进度 |
| `draft` | `proposal:string, answer:string` | 某提案的未验证即时文本，不能当最终结论 |
| `complete` | `result:AnalysisResult` | 完整结果，替换草稿展示 |
| `error` | `message:string` | 流中失败终态，不生成完整成功结果 |

进入流前的认证、版本、请求校验或容量失败使用普通 HTTP 错误；进入流后 HTTP 200 不代表成功，必须读取终态。断开连接会取消仍在执行的工作，但结果若已保存而终态传输丢失，不能声称保存已回滚；先检查历史再决定是否重试。流起始响应头的版本不能视为完成后的最终版本，可重新查询账号版本。

最小请求示例（虚构问题；实际使用读取到的版本与预览指纹）：

```http
POST /api/analysis/stream
Authorization: Bearer <session-token>
If-Match: <account-revision>
Content-Type: application/json

{"question":"讨论当前持仓结构","workflow":"council","webSearch":true,"contextSelection":{"includeProfile":true,"includeHoldings":true,"includeEvidence":true,"includeMemory":true},"previewRevision":"<context-revision>"}
```

```text
data: {"type":"status","message":"正在分析"}

data: {"type":"draft","proposal":"P1","answer":"初步分析文本"}

```

上例仅展示事件帧格式，不是一次已完成的分析记录；成功必须另收到含完整 `AnalysisResult` 的 `complete`。

## 5. 支撑与内部接口

### 5.1 服务与运维

实现：[托管 HTTP](../server/src/hosted.rs)、[健康路由](../server/src/http/mod.rs)、[活跃度和看板](../server/src/hosted/activity.rs)。

| 方法 | 路径 | 权限 / 版本 | 输入 → 响应 | 作用 |
| --- | --- | --- | --- | --- |
| GET | `/api/server` | 匿名 / — | — → `ServerInfo` | 托管协议与能力标记 |
| GET | `/api/health` | 账号 / — | — → `{status:"ok",version:string}` | 当前位于账号路由内，不能当匿名健康探针 |
| POST | `/api/activity` | 账号 / — | B `{page:Page}` → `{ok:true}` | 记录页面活跃元数据，不写个人账本 |
| GET | `/api/dashboard` | 管理 / — | Q `days?:integer=30` → `Dashboard` | 账号活跃度与公共价格采集状态，权限由账号数据库判定 |

`Page` 为 `watch/calendar/facts/evidence/review/advisor/methodology/settings`。看板数据类型见[活跃度类型](../server/src/accounts/activity.rs)及[价格监控类型](../server/src/models/market/monitor.rs)。网站 `/dashboard`、`/dashboard/` 是静态页面入口，不是数据 API。`/api/{*path}` 是未知路径的 404 兜底，也不是通用调用接口。

### 5.2 桥接 Worker

以下均用桥接凭据，不使用账号 token 或 `If-Match`，长轮询不持有账号数据库锁。来源：[Worker HTTP](../server/src/http/codex_bridge.rs)、[桥接任务与事件](../server/src/ai/bridge.rs)、[模型消息与完成类型](../server/src/ai/provider.rs)。

| 方法 | 路径 | 输入 → 响应 | 作用 |
| --- | --- | --- | --- |
| POST | `/api/codex-bridge/worker/poll` | B `{runner:string,model:string,models?:CodexModel[]}` → `BridgeJob/null` | 宣告能力并领取任务，暂无任务可返回 null |
| POST | `/api/codex-bridge/worker/event` | B `{runner:string,jobId:string,event:BridgeEvent}` → `{active:boolean}` | 上报输出/心跳；active 表示任务仍有效 |

`BridgeJob` 为 `{id,model,messages,search}`，其中 `messages` 是模型消息数组，`search` 是布尔值。`BridgeEvent` 为带 `type` 的联合类型：`delta{text}`、`complete{result:SearchCompletion}`、`error`、`heartbeat`、`status{message}`、`failure{reason:CodexFailure}`。这与浏览器分析 SSE 是不同协议，不能混用 `complete` 的字段。`CodexFailure` 的枚举见[推理契约](../server/src/models/inference.rs)。

### 5.3 独立 Agent

以下使用独立 Agent 地址，全部要求 Agent token，包括健康检查；不使用账号 `If-Match`。来源：[Agent 实现](../server/src/agent.rs)。

| 方法 | 路径 | 输入 → 响应 | 作用 |
| --- | --- | --- | --- |
| GET | `/health` | — → `{status:"ok"}` | Agent 健康状态 |
| POST | `/execute` | B `AgentJob` → `AnalysisResult` | 执行已冻结任务，不直接保存账号数据 |
| POST | `/execute/stream` | B `AgentJob` → SSE `AnalysisEvent` | 流式执行已冻结任务 |
| POST | `/dream` | B `DreamJob` → `DreamOutcome` | 生成记忆整理结果，账号写入由主服务负责 |

`AgentJob` 是 `{request:AnalysisRequest,context:BuiltContext,config:ModelConfig,api_key:string}`；`DreamJob` 是 `{input:DreamInput,config:ModelConfig,api_key:string}`。`BuiltContext` 的完整字段为 `payload,groups,payload_bytes,revision,evidence_items,valuation_evidence,evidence_catalog`，见[冻结上下文](../server/src/context.rs)；后两个集合可省略并默认空。`DreamInput` 是 `{analyses:DreamSourceAnalysis[],memories:MemoryNote[]}`，结果见[记忆类型](../server/src/models/memory.rs)。这些请求包含冻结凭据，只用于服务间调用，不作为浏览器公开执行入口。

## 6. 共用字段与类型查询

以下记录输入字段及响应导航，避免每个接口复制同一结构。大型嵌套响应通过具名类型和源码链接继续查询，字段存在性、序列化规则以相应类型及契约测试核对；并非所有 Rust 内部结构都是可发送的请求。前端公共类型另见 [types.ts](../client/src/types.ts)，调用模块见 [api](../client/src/api/)。

### 6.1 账号与财务输入

主要定义：[核心模型](../server/src/models.rs)、[精确会计类型](../server/src/models/accounting.rs)、[每日资产类型](../server/src/models/daily.rs)、[期权类型](../server/src/models/options.rs)。

| 类型 | 字段与传输语义 |
| --- | --- |
| Credentials / Session | 输入 `email,password:string`；输出 `token,userId,email:string; expiresAt:integer`（Unix 秒） |
| FinancialProfile | `monthlyIncome,monthlyExpense,emergencyFund,liabilities,maxDrawdownPct:number; horizonYears:integer; baseCurrency?:string=CNY; providedFields?:ProfileField[]/null; exactAmounts?:ProfileAmounts/null`；providedFields 标明已填写项，不能从数值 0 推断缺失 |
| ProfileAmounts | `monthlyIncome,monthlyExpense,emergencyFund,liabilities:string`，为精确金额 |
| HoldingInput | `symbol,name,assetClass,currency:string; marketValue,costBasis,targetPct:number; costPrice?,quantity?,currentPrice?,fxRateToBase?:number/null; valuationDate?,fxRateSource?,fxRateObservedOn?:string`（默认空）；`exactValues?:HoldingAmounts/null; cashAmount?:string/null` |
| HoldingAmounts | `marketValue,costBasis:string; quantity?,costPrice?,currentPrice?:string/null; priceStatus?:string=unconfirmed` |
| AmountInput | `amount:string; expectedRevision,requestId:string`；amount 当前也兼容 JSON number，由十进制转换器处理，精确调用优先字符串 |
| OptionPriceInput | `requestId,expectedRevision,price,observedOn:string`；price 是每单位报价，observedOn 是日期 |
| TradeSecurityInput | `name,symbol,assetClass,currency:string; option?:OptionContract/null` |
| OptionContract | `underlyingId,account,strike,expiresOn,multiplier:string; optionType:call/put; side:long/short`；strike、multiplier 为十进制字符串 |
| HoldingTradeInput | `requestId:string; holdingId?:string=""; expectedRevision?:string/null; newHolding?:TradeSecurityInput/null; expectedCashBaseline?:string=""`，加下行成交字段；已有持仓与新证券按业务校验选用 |
| 成交字段 | `kind:buy/sell/dividend/expire; occurredOn:string; sequence:integer; quantity,price,grossAmount,fees,taxes,account:string; externalId?,note?:string=""; settlementMode:history/settle`；history 记录历史，settle 按现金基线结算；金额与数量均为十进制字符串 |
| HoldingTradeUpdateInput | `expectedRevision:string` 加成交字段；不接收新增成交的 requestId/newHolding/holdingId |
| HoldingTrade | 返回记录含 `id,holdingId,kind,occurredOn,sequence,quantity,price,grossAmount,fees,taxes,currency,account,externalId,note,createdAt,updatedAt`；不含 settlementMode |
| TradeRepairInput | `expectedRevision:string; trades:HoldingTrade[]; reason:string`；提供完整修复集合 |
| SplitEvent / ManualCorporateActionInput | SplitEvent 为 `effectiveOn,numerator,denominator:string`；手工输入为 `requestId,expectedRevision,reason:string; split:SplitEvent` |
| InvestmentRuleInput | `category,statement,trigger,rationale:string; active:boolean; sourceReviewId?:string/null` |
| SyncDataset | `schemaVersion:integer; exportedAt:string; tables:SyncTable[]`；每表 `{name,columns,rows}`，rows 为单元格二维数组；SyncValue 使用带类型值：`"Null"` 或 `{Integer:number}`、`{Real:number}`、`{Text:string}`，不是普通扁平对象数组 |

### 6.2 观察与研究输入

| 类型 | 字段与定义来源 |
| --- | --- |
| WatchRules | `minYieldBps:integer; daily,weekly:BandRule`；BandRule 为 `reference:lower/middle/upper; mode:near/at_or_below/at_or_above; offsetBps:integer`。默认业务规则引用用户 spec，校验见[规则类型](../server/src/models/watch/rules.rs) |
| GridPlan | `budget,basePrice:string; sizeUnit:amount/budgetPct; levels:{price:string,size:string}[]`，金额/价格均为十进制字符串，size 按 sizeUnit 解释。投影与合法范围见[网格类型](../server/src/models/watch/grid.rs) |
| Catalog / Company | Catalog 为 `version:integer; verifiedOn:string; industries:{id:string,name:string,companies:Company[]}[]`；Company 为 `symbol,name:string; sources?:string[]; sourcesCheckedOn?:string/null`，见[公共目录类型](../server/src/models/watch/catalog.rs) |
| ResearchEvidenceInput | `content?,assetName?,category?:string=""; attachments?:ResearchAttachmentInput[]=[]`；资料内容与附件还须通过业务校验，空默认不是“空资料一定可保存” |
| ResearchAttachmentInput | `name:string,dataBase64:string`；传输是 JSON base64，不使用 multipart。响应附件元数据为 `id,name,byteSize,extractionNote`，提取的内部文本不序列化返回 |

### 6.3 AI 输入与共用输出

定义：[核心模型](../server/src/models.rs)、[供应商枚举](../server/src/models/provider.rs)、[记忆类型](../server/src/models/memory.rs)。

| 类型 | 字段与语义 |
| --- | --- |
| ModelConfigInput | `provider:openai-responses/openai-chat/anthropic/codex-bridge; baseUrl,model:string; apiKey?:string/null; source?:account/server=account` |
| ModelConfig | `provider,baseUrl,model,hasApiKey,source`，可有 `serverModel`；不返回 apiKey |
| CodexModel / CodexBridgeStatus | CodexModel 为 `model,displayName:string; description?:string=""; isDefault?:boolean=false`；状态为 `paired,online:boolean; model:string/null; models:CodexModel[]` |
| ContextSelection | `includeProfile?,includeHoldings?,includeEvidence?,includeMemory?:boolean`，当前各字段默认 true |
| AnalysisRequest | `question:string; workflow:"council"; webSearch?:boolean=false; previousTurn?:PreviousTurn/null; contextSelection?:ContextSelection; previewRevision?:string/null`；拒绝未知字段，不接收持仓账本 JSON 作为独立请求字段 |
| PreviousTurn | `question,answer:string; suggestions:string[]; earlierQuestions?:string[]=[]; pendingQuestion?:string/null`；主摘要总长不超过 800 字符，建议最多 3 条；早期问题最多 5 条，每条不超过 300 字符，追问不超过 200 字符 |
| AnalysisSettings | `workflow?:"council"; webSearch?:boolean=false; contextSelection?:ContextSelection`；按整组设置保存，缺省应用当前默认值 |
| AnalysisPreview | `provider,model,workflow,groups,evidenceCandidates,payload,payloadBytes,contextRevision,systemPolicy,localOnly,generatedAt`；payload 是当前内部准备数据，不代表模型输入已按新 spec 精简 |
| AnalysisResult | `id,answer,stages,transparency,workflowTrace,createdAt,disclaimer`；transparency 含授权组、模型、指纹、调用与 token 统计；workflowTrace 含研究、提案、投票、引用及输出校验记录，嵌套类型见核心模型与[评议类型](../server/src/models/council.rs) |
| AnalysisHistoryItem / StoredAnalysis | 列表项含 `id,question,createdAt,transparency,groundingStatus,workflowVersion,verdict`；详情含 `id,question,answer,createdAt,transparency,workflowTrace`。兼容字段可空，不据旧档案推断当前工作流 |
| MemoryNote | `id,kind,origin,statement,rationale,sourceAnalysisIds,quote,active,createdAt,updatedAt`；kind 为 `preference/viewpoint/context`，origin 为 `userStated/modelInferred` |
| MemoryOverview | `items:MemoryNote[]; dream:DreamState; pendingAnalyses:integer; dreamDue,enabled:boolean`；DreamState 含可空 `dreamedThrough,lastRunAt,lastResult`，lastResult 汇总 analyses/added/updated/archived/rejected |

### 6.4 大型响应查询地图

这些响应包含嵌套投影或兼容字段，调用者应从类型定义查询完整字段与可空性；这里只记录查询入口，不复制一份全量源码。

| 响应 | 主要内容 | 完整字段来源 |
| --- | --- | --- |
| Snapshot | profile、holdings、accounting、cashBalances、holdingTrades、holdingTradeSummaries、holdingRevisions、corporateActions、optionContracts、stockOptionTotals、holdingValuations、findings、valuationStatus、plan、汇总与时间 | [核心模型](../server/src/models.rs)、[精确会计](../server/src/models/accounting.rs)、[期权](../server/src/models/options.rs) |
| FinancialHistory / HoldingTradeSummary | records 中的更正前后值、cashDelta、operationId、reason、时间和可空 quarantine；成交汇总中的数量、成本、已实现收益、分红、费用、税、现金流 | [客户端返回类型](../client/src/api/portfolio.ts)、[财务仓储](../server/src/db/financial.rs)、[核心模型](../server/src/models.rs) |
| DailyHistory / DailyComparison | timezone、today、records、nextBefore；比较结果 from/to、amountChange、assets、allocationChanges | [每日资产类型](../server/src/models/daily.rs) |
| Allocation / DividendIncome | baseCurrency、byClass/byIndustry、missing；分红 since/value/assetsPct/missing，精确金额与比例为字符串 | [会计类型](../server/src/models/accounting.rs) |
| FxBook / FxRateQuote | FxBook 含 baseCurrency、checkedAt、requestedOn、quotes、manual、error；报价含 fromCurrency/toCurrency、rate/exactRate、requestedOn/observedOn、stalenessDays、来源与说明 | [汇率类型](../server/src/models/market.rs)、[核心模型](../server/src/models.rs) |
| SecurityPriceQuote | symbol、currency、close/exactClose、splits、requestedOn/observedOn、stalenessDays、providerCode/providerName、exchange/micCode/instrumentType、priceBasis、来源与说明 | [核心模型](../server/src/models.rs) |
| WatchEntry / WatchRow / WatchBoard | 个人条目、最新观察、历史、提醒及账号规则；日期与行情缺失状态保留其业务含义 | [观察类型](../server/src/models/watch.rs) |
| CatalogBoard / CatalogRow / CalendarBoard | 目录版本、编辑权限、行业/公司观察；分红计划日历及覆盖状态 | [目录](../server/src/models/watch/catalog.rs)、[日历](../server/src/models/watch/calendar.rs)、[公共计划](../server/src/models/market/dividend_plans.rs) |
| GridProjection | 网格档位与预算、买卖情景计算，不是成交记录 | [网格类型](../server/src/models/watch/grid.rs) |
| ResearchEvidence / InvestmentRule / InvestmentRuleRevision / SystemReviewRecord | 原文和附件元数据、标签与归档；手工规则及修订；文字复盘、当时快照及兼容字段 | [核心模型](../server/src/models.rs) |
| InvestmentMethodology | prompt、defaultPrompt、maxLength | [方法论类型](../server/src/models/methodology.rs) |
| ServerInfo / Dashboard | mode/database/protocol/codexBridge/analysisTimeoutSeconds/codexMaxAttempts/publicDividendData；活跃统计与 marketPrices | [托管类型](../server/src/hosted.rs)、[看板](../server/src/hosted/activity.rs)、[价格监控](../server/src/models/market/monitor.rs) |

## 7. 维护与验证

AI 账号访问的实现边界见[能力端口](../server/src/ports/ai_account.rs)与[存储适配](../server/src/db/ai_access.rs)。端口拆分保留既有 HTTP 字段、版本校验、凭据冻结与流式行为；这些内部接口不作为新的公开 HTTP API。

新增或变更接口时，同次更新本文的方法、路径、权限、版本、输入输出及副作用；业务规则变更更新所属 spec。现有类型与路由说明当前行为，并不自动授权其所有历史行为继续成为产品要求；已采纳行为与代码差异应明确记录并修复。

现有[契约检查](../scripts/check-contracts.mjs)覆盖部分 Rust/TypeScript 字段与请求策略，[传输回归](../client/src/api.hosted.test.ts)、[托管验收](../scripts/test-hosted.mjs)、[功能 API 验收](../scripts/test-functional-api.mjs)及各 HTTP 模块内测试提供进一步依据。本文建立时核对路由清单、字段来源、链接和差异，不代表这些完整测试已重新执行。

`npm run check:docs` 持续核对本文表格与 `hosted.rs`、`agent.rs`、`http/` 路由的方法和路径，检查遗漏、过期或重复条目，已接入完整门禁。它不校验全部字段、权限与副作用，也不生成全量 OpenAPI schema；这些内容仍须结合类型和契约测试核实。范围与限制见 [lint 文档](lint.md)，其他已确认文档见[文档索引](README.md)。
