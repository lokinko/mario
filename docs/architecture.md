# 代码边界与架构

本文件按本次讨论重新建立架构边界，描述已认可的职责划分及现有代码映射。一级功能域为用户、市场、AI；应用装配与公共技术能力另列为支撑部分。模块边界不意味着拆成独立服务；已完成和待完成的拆分见文末。

业务行为由各域 spec 说明；本文件不重复维护业务阈值或 HTTP 字段。当前已确认[用户域 spec](specs/user.md)、[市场域 spec](specs/market.md)、[AI 域 spec](specs/ai.md)、[lint 范围](lint.md)及 [API 文档](api.md)。

## 1. 功能域与数据归属

| 功能域 | 子模块与职责 | 拥有的数据与结果 |
| --- | --- | --- |
| 用户 | 账号与会话、个人数据管理 | 身份、会话、账号隔离、数据版本；协调个人备份与导入 |
| 用户 | 财务事实、账本、个人资产估值 | 收支、负债、期限、持仓、现金、真实成交、期权、每日资产、会计与风险规划结果 |
| 用户 | 买点观察 | 个人自选、人工维护的共享候选目录、观察规则、历史结果、提醒、个人分红日历、网格测算 |
| 用户 | 研究资料 | 用户保存的原文、附件、标签、归档状态与检索能力 |
| 用户 | 方法论 | 用户明确表达的投资方法论与原则 |
| 市场 | 公共市场数据 | 证券基础信息、价格、汇率、公司分红事实与已披露计划、A 股法定官方公告，以及来源、缓存、采集和质量状态 |
| AI | 分析、每日复盘与记忆 | 授权上下文、分析预览与冻结、问答与评议、流式输出、分析历史、每日自动复盘信息流与调度配置、长期记忆 |
| AI | 模型连接与执行 | 模型配置、供应商适配、模型目录、本机模型授权与桥接 |

业务归属与数据可见范围分别判断。共享观察候选目录属于用户域，不因存储在公共 schema 就成为市场事实；AI 历史按账号隔离，但其行为规则仍属于 AI 域。证券基础目录属于市场域。共享候选目录由用户域的[目录升级逻辑](../server/src/models/watch/catalog.rs)按版本采用经确认的指数成分清单，由[共享目录仓储](../server/src/db/public_watch.rs)在启动事务中写入；成分身份和来源与市场行情、分红及核查状态分别管理。

公司分红事实属于市场域，个人实际到账属于用户账本。公共报价属于市场域，用户采用的估值、人工汇率配置与相应会计结果属于用户域。长期记忆属于 AI 域，不能替代用户填写的财务事实和原则。

## 2. 主要依赖与操作边界

箭头表示使用能力的方向：

- **用户 → 市场**：取得证券、日期、价格和分红等数据，由用户域完成个人估值与观察计算。
- **AI → 用户、市场的授权数据接口**：取得允许参与分析的输入。模型执行阶段只使用冻结资料，不自行读取账号数据库。
- **支撑部分 → 各域入口**：组合页面、装配服务、执行认证与启动后台任务；领域业务规则仍归所属域。

市场服务不依赖个人账本、观察偏好或 AI 判断，也不直接更新个人业务数据。AI 不直接改写账本、观察规则、研究资料或方法论。后台个人估值由用户域用例更新，市场服务提供行情。

跨域操作通过所属域提供的能力完成，调用者不直接操作其他域的数据表。个人导入导出可以搬运不同域的账号数据，但须协调各域校验与整体事务；搬运职责不转移数据的业务所有权。

AI 用例负责授权、冻结输入及保存分析结果；模型执行器负责执行冻结任务。模型配置与执行能力不决定资料的业务授权。网站身份登录属于用户域，本机模型授权属于 AI 域。

这些是已认可的职责边界。研究资料、分析历史与方法论已按职责拆分入口，契约按业务归属显式导入，请求装配与存储适配保留统一账号事务，仓储方法、表与设置键分别按归属检查，不能将原则描述为已完全强制执行。

## 3. 运行角色与支撑部分

当前产品由 React Web、Rust API 和 PostgreSQL 组成。Rust 启动入口见 [runtime](../server/src/runtime.rs)。API 负责账号上下文和业务请求；AI 可在 API 进程内执行，也可由独立 [Agent](../server/src/agent.rs)执行。独立 Agent 接收冻结任务，不读取账号数据库。

本机桥接 worker 使用本机模型执行能力，网站登录会话与桥接凭据用途不同。模型设置页将发出配对请求的 API 基址传给本机 worker，worker 仅在该地址下访问桥接路由。模型连接归 AI 域，进程启动、认证传输和服务装配属于支撑部分。

| 支撑部分 | 职责与现有入口 |
| --- | --- |
| 前端应用壳 | [app](../client/src/app/)负责导航、跨模块组合、账号切换和数据生命周期。[工作台](../client/src/App.tsx)精简为资产（我的情况）、决策（每日复盘、买点观察/内嵌分红日历、问答）、研究（研究资料、方法论）和底部设置；桌面与移动端共用位置栏、全局金额隐藏切换和异常态同步提示，移动导航通过「资产」入口进入持仓与存款页面。共享展示与传输分别在 [components](../client/src/components/)和 [lib](../client/src/lib/) |
| HTTP 与进程装配 | [hosted](../server/src/hosted.rs)、[http](../server/src/http/)、[composition](../server/src/composition.rs)负责协议、认证上下文和依赖装配；路由中的业务职责仍按所属域识别 |
| 存储与安全基础 | [storage](../server/src/storage.rs)封装数据库驱动；[secrets](../server/src/secrets.rs)封装凭据加密；公共技术能力不拥有业务规则 |
| 站点运营 | 汇总使用统计和市场采集状态，权限由账号能力检查；不作为任意跨账号业务访问入口 |
| 构建与运维 | [scripts](../scripts/)与 [deploy.sh](../deploy.sh)负责开发、构建、部署、整站备份恢复和验证 |

## 4. 现有代码映射

当前目录主要按技术层组织，以下列的是职责入口，不表示目录内所有代码都只属于一个域。

| 业务职责 | 前端入口 | 后端主要入口 |
| --- | --- | --- |
| 用户账号与数据管理 | [account](../client/src/features/account/) | [accounts](../server/src/accounts.rs)、[hosted](../server/src/hosted.rs)、[同步仓储](../server/src/db/sync_store.rs) |
| 用户财务账本与估值 | [portfolio](../client/src/features/portfolio/) | [portfolio 用例](../server/src/application/portfolio.rs)、[个人价格更新](../server/src/application/prices.rs)、[个人汇率用例](../server/src/application/fx.rs)、[财务仓储](../server/src/db/financial.rs)、[成交仓储](../server/src/db/trades.rs)、[估值](../server/src/valuation.rs)、[风险](../server/src/risk.rs)、[规划](../server/src/planning.rs) |
| 用户买点观察 | [watch](../client/src/features/watch/) | [观察用例](../server/src/application/watch.rs)、[候选目录用例](../server/src/application/watch_catalog.rs)、[观察仓储](../server/src/db/watch.rs)、[观察规则与计算](../server/src/models/watch.rs) |
| 用户研究资料 | [资料工作台](../client/src/features/evidence/EvidenceWorkbench.tsx) | [研究资料仓储](../server/src/db/evidence.rs)、[附件处理](../server/src/attachments.rs)、[证据检索](../server/src/evidence.rs) |
| 用户方法论 | [methodology](../client/src/features/methodology/) | [方法论仓储](../server/src/db/methodology.rs) |
| 市场公共数据 | 由用户页面、[公开红利观察](../client/src/features/watch/dividend-preview/DividendPreview.tsx)、每日复盘和运营看板消费。公开红利观察读取内置历史样本。[分红卡片模型](../client/src/features/watch/dividend-preview/card.ts)将逐股解读和[归档行情指标](../client/src/features/watch/dividend-preview/returns.ts)组装成页面与分享长图共用的内容 | [数据来源适配](../server/src/market_data.rs)、[巨潮公告适配](../server/src/market_data/announcements.rs)、[公共价格服务](../server/src/application/public_prices.rs)、[公共公告服务](../server/src/application/announcements.rs)、[共享分红查询](../server/src/application/watch_market.rs)、[公共价格仓储](../server/src/db/public_prices.rs)、[公共公告仓储](../server/src/db/public_announcements.rs)、[公共分红仓储](../server/src/db/public_dividends.rs) |
| AI 分析、每日复盘与记忆 | [问答](../client/src/features/advisor/Advisor.tsx)、[每日复盘](../client/src/features/dailyReview/DailyReviewPage.tsx)、[红利样本解读](../client/src/features/watch/dividend-preview/briefs.ts)、[记忆](../client/src/features/advisor/MemoryPanel.tsx) | [分析用例](../server/src/application/analysis.rs)、[每日复盘用例](../server/src/application/daily_review.rs)、[上下文](../server/src/context.rs)、[模型工作流](../server/src/ai/)、[记忆用例](../server/src/application/memory.rs)、[分析历史仓储](../server/src/db/analysis_history.rs)、[每日复盘仓储](../server/src/db/daily_review.rs) |
| AI 模型连接 | [settings](../client/src/features/settings/)中的模型配置与桥接 | [模型配置用例](../server/src/application/model_settings.rs)、[模型管理端口](../server/src/ports/model.rs)、[本机授权](../server/src/chatgpt_auth.rs)、[系统加密](../server/src/chatgpt_auth/crypto.rs)、[worker](../server/src/codex_bridge_worker.rs) |

## 5. 已落实的职责拆分

- **用户研究资料**：前端页面为 `features/evidence`，调用 `api/evidence.ts`；后端适配为 `http/evidence.rs`，原文、标签及附件保存在 `db/evidence.rs`。
- **AI 问答与历史**：前端为 `features/advisor`，分析调用 `api/analysis.ts`；后端为 `http/analysis.rs` 与 `db/analysis_history.rs`。历史网页摘录复用留在 AI 历史仓储，不写入用户资料表。回答引用用户附件时通过用户资料 API 读取。
- **用户方法论与 AI 设置**：方法论有独立 `api/methodology.ts`、`http/methodology.rs`、`db/methodology.rs`；模型配置与问答预设由 `http/model_settings.rs`、`db/model_settings.rs` 管理。`db/settings.rs` 仅保留共用键值存取原语。数据库格式和现有 HTTP 路径保持兼容。
- **AI 账号访问能力**：[账号端口](../server/src/ports/ai_account.rs)区分只读模型配置、可写模型设置、用户只读分析来源、分析历史保存、记忆管理和账号锁协调；[存储适配](../server/src/db/ai_access.rs)复用现有事务与账号隔离。分析、记忆、模型配置与桥接用例按泛型端口编译，不能取得具体 Database、连接、任意配置键或用户账本写入方法。模型密钥绑定是[模型规则](../server/src/models/provider.rs)，移除密钥的业务校验由模型设置用例负责。用户资料通过[授权投影](../server/src/models/accounting/analysis.rs)提供，AI 接口只取得当前可用状态和授权组信息，不再接收完整 Snapshot。
- **用户账号访问能力**：[用户端口](../server/src/ports/user_account.rs)分别约束组合、估值、汇率和个人观察用例；它们不再依赖具体 Database，也不能调用模型配置、密钥、AI 历史或任意设置键。汇率旧缓存由存储适配通过具名报价读取。账号锁协调移至公共[账号端口](../server/src/ports/account.rs)和[锁适配](../server/src/db/account_access.rs)，不再属于 AI 专用能力；[用户适配](../server/src/db/user_access.rs)继续调用原仓储方法，沿用事务、版本校验和现金一致性规则。
- **市场分红计划与用户日历**：[计划类型](../server/src/models/market/dividend_plans.rs)、[公共计划仓储](../server/src/db/public_dividend_plans.rs)与[采集服务](../server/src/application/dividend_plans.rs)属于市场域，只处理证券代码和公共计划。仓储继续使用既有 `mario_market.dividend_plan_snapshots` 表，不初始化用户候选目录。用户 [日历用例](../server/src/application/dividend_calendar.rs)按账号自选和共享候选目录确定展示范围，仅持有 `DividendPlanReader` 缓存读端口。后台装配协调刷新范围，采集服务不读取账号或用户目录。
- **HTTP 账号能力**：认证层在[请求装配](../server/src/http/context.rs)中选择账号并持有具体存储；财务、观察、资料、方法论、每日复盘、分析、记忆、模型设置和桥接路由分别提取自己的上下文。上下文内部字段私有，账号访问通过不暴露具体 Database 的能力接口返回。用户记录能力见 [user_records](../server/src/ports/user_records.rs)，AI 历史与预设见 [ai_records](../server/src/ports/ai_records.rs)，分别由所属存储适配委托原事务方法。备份上下文只协调版本、导出和导入；认证中间件仍在同一账号连接上持锁，处理全局版本与财务回执后再执行业务路由。
- **仓储内部边界**：[仓储归属登记](../scripts/storage-boundaries.mjs)覆盖生产仓储模块、账号方法和当前表；业务仓储禁止跨域调用或直接操作其他域表。AI 授权适配只允许读取明列的用户来源，迁移和备份负责共享事务协调。生产文件显式导入所属类型与辅助方法，db 根模块不再提供混合业务类型总入口。用户设置与 AI 设置分别通过 [UserSetting](../server/src/db/settings/user.rs)、[AiSetting](../server/src/db/settings/ai.rs)构造，原字符串键读写仅供测试夹具；模型凭据固定为原有 model 槽位。备份 SQL 筛选与输入校验共同引用设置类型，沿用原有键名、加密账号范围及导入格式。
- **公共事实与观察规则**：公共分红历史、覆盖日期、每股 TTM 计算在 [市场分红契约](../server/src/models/market/dividends.rs)，价格条目与统一行情输入在 [行情序列](../server/src/models/market/series.rs)。官方证券资料通过 [SecuritySources](../server/src/models/market/securities.rs)返回，用户候选目录显式转换为自有 Company，市场适配不再借用用户 WatchInput 校验。内置资料包只负责公共事实；CSV 的观察参考价由[用户导出用例](../server/src/application/watch_export.rs)生成，运行入口协调加载与不覆盖文件的写入。
- **精简模型输入**：用户授权当前状态在 `models/accounting/analysis.rs` 生成，交易明细、缺口码和审计材料不进入模型资料；`context/render.rs` 将业务输入渲染为 Markdown，内部引用结构与历史审计独立保存。新工作流版本与搜索默认值见 [AI spec](specs/ai.md#9-现有实现差异)。
- **契约归属**：后端 [models](../server/src/models.rs)只声明模块，不再提供跨域类型总入口；用户财务、资料、方法论和个人汇率分别在 `portfolio`、`research`、`methodology`、`fx`，公共查询与报价在 [market/quotes](../server/src/models/market/quotes.rs)，AI 请求、每日复盘和模型连接分别在 [analysis](../server/src/models/analysis.rs)、[daily_review](../server/src/models/daily_review.rs)、[connection](../server/src/models/connection.rs)。前端通过 [用户类型](../client/src/types/user.ts)、[市场类型](../client/src/types/market.ts)、[AI 类型](../client/src/types/ai.ts)显式导入。用户授权投影只接收用户资料筛选参数，AI 授权类型由适配层转换，用户契约不依赖 AI。
- **依赖检查**：前端页面和 API 客户端显式登记业务归属；用户页面不能直接导入 AI API，跨页分析通过应用回调组合。后端显式登记用例与端口的业务归属；市场来源、公共市场仓储和市场用例禁止依赖用户或 AI 契约与账号端口，用户用例禁止反向依赖 AI，AI 通过授权来源读取用户资料。计划采集、仓储和用户日历还检查导入的具体符号，禁止以模块别名绕开窄接口。技术模块、循环与隔离检查继续执行，见 [lint 文档](lint.md)。

## 6. 保留的协调与验证范围

统一 [Database](../server/src/db.rs)保留连接、账号锁和事务协调，同一笔财务变更及备份导入继续整体提交或回滚。职责边界通过所属契约、能力端口、请求上下文，以及仓储方法和表归属共同落实；共享一个账号事务不改变用户记录、AI 记录与公共行情的所有权。

初始化与迁移可以按顺序创建和升级各域数据，备份协调可以按已验证清单搬运多个域的个人数据。动态业务表名只保留在迁移和备份协调范围；用户、AI 和市场业务仓储各自访问所属表。AI 输入适配的只读用户来源是明列的交接接口，不能扩展为用户记录写入。个人备份只搬运认可的设置子集，保留接收账号的模型连接、问答预设及密钥。

架构检查沿 Rust 生产模块声明树解析实际源码，排除测试模块；声明、方法和字面量 SQL 分别核对归属。SQL 检查处理当前表名、连接、LATERAL、JSON 函数与注释，不能当作任意动态程序的数据流或数据库权限证明。新增模块解析或 SQL 形式需要扩展相应检查和回归；当前独立测试夹具及历史格式兼容不属于生产业务能力。

工程门禁与领域测试共同验证现有结构和行为；真实模型回复质量、外部市场来源的数据时效和部署状态仍按各自验收范围核对，不由架构检查推断。
