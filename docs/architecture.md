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
| 用户 | 方法论与复盘 | 用户明确表达的方法论、手工投资规则、规则修订与文字复盘 |
| 市场 | 公共市场数据 | 证券基础信息、价格、汇率、公司分红事实与已披露计划，以及来源、缓存、采集和质量状态 |
| AI | 分析与记忆 | 授权上下文、分析预览与冻结、问答与评议、流式输出、分析历史、长期记忆 |
| AI | 模型连接与执行 | 模型配置、供应商适配、模型目录、本机模型授权与桥接 |

业务归属与数据可见范围分别判断。共享观察候选目录属于用户域，不因存储在公共 schema 就成为市场事实；AI 历史按账号隔离，但其行为规则仍属于 AI 域。证券基础目录属于市场域。

公司分红事实属于市场域，个人实际到账属于用户账本。公共报价属于市场域，用户采用的估值、人工汇率配置与相应会计结果属于用户域。长期记忆属于 AI 域，不能替代用户填写的财务事实和原则。

## 2. 主要依赖与操作边界

箭头表示使用能力的方向：

- **用户 → 市场**：取得证券、日期、价格和分红等数据，由用户域完成个人估值与观察计算。
- **AI → 用户、市场的授权数据接口**：取得允许参与分析的输入。模型执行阶段只使用冻结资料，不自行读取账号数据库。
- **支撑部分 → 各域入口**：组合页面、装配服务、执行认证与启动后台任务；领域业务规则仍归所属域。

市场服务不依赖个人账本、观察偏好或 AI 判断，也不直接更新个人业务数据。AI 不直接改写账本、观察规则、研究资料或方法论。后台个人估值由用户域用例更新，市场服务提供行情。

跨域操作通过所属域提供的能力完成，调用者不直接操作其他域的数据表。个人导入导出可以搬运不同域的账号数据，但须协调各域校验与整体事务；搬运职责不转移数据的业务所有权。

AI 用例负责授权、冻结输入及保存分析结果；模型执行器负责执行冻结任务。模型配置与执行能力不决定资料的业务授权。网站身份登录属于用户域，本机模型授权属于 AI 域。

这些是已认可的职责边界。研究资料、分析历史与方法论已按职责拆分入口，集中契约与统一 Database 仍待继续修复，不能将原则描述为已完全强制执行。

## 3. 运行角色与支撑部分

当前产品由 React Web、Rust API 和 PostgreSQL 组成。Rust 启动入口见 [runtime](../server/src/runtime.rs)。API 负责账号上下文和业务请求；AI 可在 API 进程内执行，也可由独立 [Agent](../server/src/agent.rs)执行。独立 Agent 接收冻结任务，不读取账号数据库。

本机桥接 worker 使用本机模型执行能力，网站登录会话与桥接凭据用途不同。模型连接归 AI 域；进程启动、认证传输和服务装配属于支撑部分。

| 支撑部分 | 职责与现有入口 |
| --- | --- |
| 前端应用壳 | [app](../client/src/app/)负责导航、跨模块组合、账号切换和数据生命周期；共享展示与传输分别在 [components](../client/src/components/)和 [lib](../client/src/lib/) |
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
| 用户方法论与复盘 | [methodology](../client/src/features/methodology/)、[reviews](../client/src/features/reviews/) | [方法论仓储](../server/src/db/methodology.rs)、[复盘仓储](../server/src/db/reviews.rs) |
| 市场公共数据 | 由用户页面和运营看板消费 | [数据来源适配](../server/src/market_data.rs)、[公共价格服务](../server/src/application/public_prices.rs)、[共享分红查询](../server/src/application/watch_market.rs)、[公共价格仓储](../server/src/db/public_prices.rs)、[公共分红仓储](../server/src/db/public_dividends.rs) |
| AI 分析与记忆 | [问答](../client/src/features/advisor/Advisor.tsx)、[记忆](../client/src/features/advisor/MemoryPanel.tsx) | [分析用例](../server/src/application/analysis.rs)、[上下文](../server/src/context.rs)、[模型工作流](../server/src/ai/)、[记忆用例](../server/src/application/memory.rs)、[分析历史仓储](../server/src/db/analysis_history.rs) |
| AI 模型连接 | [settings](../client/src/features/settings/)中的模型配置与桥接 | [模型配置用例](../server/src/application/model_settings.rs)、[模型管理端口](../server/src/ports/model.rs)、[本机授权](../server/src/chatgpt_auth.rs)、[worker](../server/src/codex_bridge_worker.rs) |

## 5. 已落实的职责拆分

- **用户研究资料**：前端页面为 `features/evidence`，调用 `api/evidence.ts`；后端适配为 `http/evidence.rs`，原文、标签及附件保存在 `db/evidence.rs`。
- **AI 问答与历史**：前端为 `features/advisor`，分析调用 `api/analysis.ts`；后端为 `http/analysis.rs` 与 `db/analysis_history.rs`。历史网页摘录复用留在 AI 历史仓储，不写入用户资料表。回答引用用户附件时通过用户资料 API 读取。
- **用户方法论与 AI 设置**：方法论有独立 `api/methodology.ts`、`http/methodology.rs`、`db/methodology.rs`；模型配置与问答预设由 `http/model_settings.rs`、`db/model_settings.rs` 管理。`db/settings.rs` 仅保留共用键值存取原语。数据库格式和现有 HTTP 路径保持兼容。
- **市场分红计划与用户日历**：[计划类型](../server/src/models/market/dividend_plans.rs)、[公共计划仓储](../server/src/db/public_dividend_plans.rs)与[采集服务](../server/src/application/dividend_plans.rs)属于市场域，只处理证券代码和公共计划。仓储继续使用既有 `mario_market.dividend_plan_snapshots` 表，不初始化用户候选目录。用户 [日历用例](../server/src/application/dividend_calendar.rs)按账号自选和共享候选目录确定展示范围，仅持有 `DividendPlanReader` 缓存读端口。后台装配协调刷新范围，采集服务不读取账号或用户目录。
- **依赖检查**：前端页面和 API 客户端显式登记业务归属；用户页面不能直接导入 AI API，跨页分析通过应用回调组合。后端对计划采集、仓储和用户日历检查导入的具体符号，禁止以模块别名绕开窄接口。技术模块、循环与隔离检查继续执行，见 [lint 文档](lint.md)。

## 6. 待继续修复的架构差异

1. **类型和仓储集中。** [后端契约](../server/src/models.rs)、[前端类型](../client/src/types.ts)及统一 [Database](../server/src/db.rs)汇集多个域。已拆分文件中的仓储方法仍挂在统一 Database 上；AI 用例仍能取得过宽的账号存取能力，尚需收窄到所属职责和授权数据接口。
2. **业务域检查未全覆盖。** 现有登记和检查强制了部分技术层与前端业务方向，未覆盖后端全部业务调用和读写权限；检查通过不能证明三个功能域已完全隔离。
3. **AI 输入与搜索默认值待调整。** 已采纳默认开启网页搜索、以简洁文本提供可用的当前持仓状态；当前实现仍默认关闭搜索，并将含交易明细与数据缺口的 JSON 事实卡加入模型输入。差异见 [AI 域 spec](specs/ai.md#8-现有实现差异)。

后续修复继续以三域 spec 和上述边界为依据，同步调整依赖约束和受影响的测试；已拆分文件不代表跨域授权与存储能力已全部收窄。
