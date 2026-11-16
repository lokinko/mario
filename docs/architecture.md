# 代码边界与架构

本文件按本次讨论重新建立架构边界，描述已认可的职责划分及现有代码映射。一级功能域为用户、市场、AI；应用装配与公共技术能力另列为支撑部分。模块边界不意味着拆成独立服务，也不意味着已经完成目录和依赖重组。

业务行为由各域 spec 说明；本文件不重复维护业务阈值或 HTTP 字段。当前已确认的[用户域 spec](specs/user.md)独立维护用户行为。其他域 spec、lint 细则和 API 文档在分别讨论确认后建立。

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

这些是已认可的职责边界。现有实现仍存在集中仓储和跨域混放，不能将上述原则描述为已经由模块接口完全强制执行。

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
| 用户研究资料 | [资料工作台](../client/src/features/research/EvidenceWorkbench.tsx) | [研究仓储的资料方法](../server/src/db/research.rs)、[附件处理](../server/src/attachments.rs)、[证据检索](../server/src/evidence.rs) |
| 用户方法论与复盘 | [methodology](../client/src/features/methodology/)、[reviews](../client/src/features/reviews/) | [设置仓储的方法论方法](../server/src/db/settings.rs)、[复盘仓储](../server/src/db/reviews.rs) |
| 市场公共数据 | 由用户页面和运营看板消费 | [数据来源适配](../server/src/market_data.rs)、[公共价格服务](../server/src/application/public_prices.rs)、[共享分红查询](../server/src/application/watch_market.rs)、[公共价格仓储](../server/src/db/public_prices.rs)、[公共分红仓储](../server/src/db/public_dividends.rs) |
| AI 分析与记忆 | [问答](../client/src/features/research/Advisor.tsx)、[记忆](../client/src/features/research/MemoryPanel.tsx) | [分析用例](../server/src/application/analysis.rs)、[上下文](../server/src/context.rs)、[模型工作流](../server/src/ai/)、[记忆用例](../server/src/application/memory.rs)、[研究仓储的分析历史方法](../server/src/db/research.rs) |
| AI 模型连接 | [settings](../client/src/features/settings/)中的模型配置与桥接 | [模型配置用例](../server/src/application/model_settings.rs)、[模型管理端口](../server/src/ports/model.rs)、[本机授权](../server/src/chatgpt_auth.rs)、[worker](../server/src/codex_bridge_worker.rs) |

## 5. 当前混合职责与检查边界

1. **研究资料和问答混放。** 前端 `features/research`、API 客户端、后端 HTTP 和 `db/research.rs` 同时包含用户资料和 AI 分析历史。历史问答网页摘录的复用属于 AI 历史资料复用，不自动写入用户研究资料表。
2. **配置混放。** 方法论属于用户域，模型配置和问答预设属于 AI 域，目前共用设置文件。
3. **分红日历跨域。** [日历用例](../server/src/application/dividend_calendar.rs)同时包含公共分红计划采集缓存和按用户观察范围生成日历的逻辑。前者属于市场域，后者属于用户域；[公共观察仓储](../server/src/db/public_watch.rs)也混合了候选目录和分红计划存储。
4. **类型和仓储集中。** [后端契约](../server/src/models.rs)、[前端类型](../client/src/types.ts)及统一 [Database](../server/src/db.rs)汇集多个域。物理存储位置和技术层目录不能替代业务归属。
5. **检查覆盖尚未对应业务域。** 现有架构检查主要验证技术层依赖、循环和部分隔离规则，不能证明三个功能域已严格隔离。

本轮确定边界与现状映射，不执行目录搬迁、仓储拆分或接口迁移。后续重组须单独确定方案；现有混合位置不因写入文档就视为已解决。
