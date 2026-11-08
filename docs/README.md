# 当前文档与模块 spec 索引

本索引连接当前规范、实现边界与验证入口。这里按业务或运行边界划分模块，内部文件职责见[架构地图](architecture.md)；沿用现有领域文档作为 spec，不另维护内容重复的需求目录。

## 先确定文档职责

| 文档层级 | 当前来源 | 用途 |
| --- | --- | --- |
| 产品目标 | [产品主线](product-vision.md) | 决定优先级与范围，不从历史计划推导新需求 |
| 模块 spec | 下方各领域文档 | 说明已采纳的行为、契约、数据归属、失败处理和恢复边界 |
| 全局架构 | [架构](architecture.md) | 说明系统结构、依赖方向、状态与事务归属，以及已知架构债 |
| 协作与验证 | [项目规范](../AGENTS.md)、[模块边界](module-boundaries.md) | 规定分工、修改方式、验证与 Git 交付 |
| 运维操作 | 下方部署、构建与迁移文档 | 说明当前脚本的使用方式；不证明线上已运行该版本 |
| 历史与审计 | [审计目录](audits/) | 保存特定时间的实验、验收与未采纳建议，不作为当前待办 |

产品目标决定取舍；实现与测试用于核实当前事实。若两者冲突，须说明是文档过时、实现缺陷还是未采纳设计，不能只把其中一侧改写成“已经一致”。已知实现偏离集中见[架构债](architecture.md#10-容量边界与已知架构债)。

## 在线产品模块

| 模块与职责 | 当前 spec | 主要实现边界 | 现有验证依据 |
| --- | --- | --- | --- |
| 买点观察、自选、公共目录、账号规则与提醒 | [买点观察](buy-watch.md) | [观察页面](../client/src/features/watch/)、[观察 HTTP](../server/src/http/watch.rs)、[领域计算](../server/src/models/watch.rs)、[账号仓储](../server/src/db/watch.rs)、[公共仓储](../server/src/db/public_watch.rs) | [页面回归](../client/src/features/watch/BuyWatch.test.tsx)、[规则编辑](../client/src/features/watch/WatchRuleStrip.test.tsx)、[账号规则回归](../server/src/db/watch/rules.rs)、[功能 API](../scripts/test-functional-api.mjs) |
| 分红历史、日历与股数网格 | [分红历史](buy-watch.md#分红事件存储与股息率重算)、[分红日历](buy-watch.md#分红日历)、[网格测算](buy-watch.md#股息网格与价差测算) | [公开来源](../server/src/market_data/watch/)、[日历编排](../server/src/application/dividend_calendar.rs)、[日历缓存](../server/src/db/public_watch/calendar.rs)、[网格计算](../server/src/models/watch/grid.rs) | [日历](../client/src/features/watch/DividendCalendar.test.tsx)、[分红展示](../client/src/features/watch/WatchDividends.test.tsx)、[网格](../client/src/features/watch/WatchGrid.test.tsx)、`npm run test:watch-data` |
| 持仓、现金、真实成交、期权与财务审计 | [持仓与交易](holding-trades.md) | [资产页面](../client/src/features/portfolio/)、[财务仓储](../server/src/db/financial.rs)、[成交](../server/src/db/trades.rs)、[期权](../server/src/db/options.rs)、[精确会计](../server/src/db/amounts.rs) | [财务回归](../server/src/db/financial_tests.rs)、[期权交互](../client/src/features/portfolio/StockOptions.test.tsx)、[真实托管 HTTP](../scripts/test-hosted.mjs)、`npm run test:financial` |
| 每日资产与趋势 | [每日资产](daily-assets.md) | [每日仓储](../server/src/db/daily.rs)、[追踪组件](../client/src/features/portfolio/DailyTracking.tsx)、[历史状态](../client/src/features/portfolio/useAssetHistory.ts) | [追踪](../client/src/features/portfolio/DailyTracking.test.tsx)、[资产历史](../client/src/features/portfolio/AssetHistory.test.tsx)、[财务验收](../scripts/test-financial.mjs) |
| 行情、汇率、估值与确定性风险规划 | [市场数据](market-data.md)、[规划模型](planning-model.md) | [市场端口](../server/src/ports/market.rs)、[行情用例](../server/src/application/portfolio.rs)、[汇率用例](../server/src/application/fx.rs)、[估值](../server/src/valuation.rs)、[风险](../server/src/risk.rs)、[规划](../server/src/planning.rs) | [汇率组件](../client/src/features/portfolio/ExchangeRates.test.tsx)、[证券预览锁回归](../server/src/application/portfolio_quote_tests.rs)、[报价认证 HTTP](../server/src/hosted_quote_tests.rs)、[财务回归](../server/src/db/financial_tests.rs)、各 Rust 模块内测试与[功能 API](../scripts/test-functional-api.mjs) |
| 账号、同步、草稿保护与个人 JSON 备份 | [账号与同步](cloud-sync.md)、[信任边界](threat-model.md) | [账号](../server/src/accounts.rs)、[认证适配](../server/src/hosted.rs)、[同步仓储](../server/src/db/sync_store.rs)、[传输](../client/src/lib/httpClient.ts)、[应用数据生命周期](../client/src/app/useApplicationData.ts) | [传输回归](../client/src/api.hosted.test.ts)、[应用导航](../client/src/App.test.tsx)、[账号交互](../client/src/features/account/WebAccess.test.tsx)、[托管隔离](../scripts/test-hosted.mjs) |
| 投资方法论与问答预设 | [方法论](methodology.md)、[授权边界](ai-data-boundary.md) | [方法论编辑](../client/src/features/methodology/InvestmentMethodology.tsx)、[设置页面](../client/src/features/settings/)、[设置仓储](../server/src/db/settings.rs)、[方法论契约](../server/src/models/methodology.rs) | [方法论](../client/src/features/methodology/InvestmentMethodology.test.tsx)、[问答预设](../client/src/features/settings/AnalysisDefaults.test.tsx)、[模型设置](../client/src/features/settings/ModelSettings.test.tsx) |
| 个人文字复盘与手工规则 | [复盘与规则](review-and-rules.md) | [复盘页面](../client/src/features/reviews/ReviewCenter.tsx)、[HTTP](../server/src/http/reviews.rs)、[仓储](../server/src/db/reviews.rs) | [复盘交互](../client/src/features/reviews/ReviewCenter.test.tsx)、[文字保存 HTTP](../scripts/test-functional-api.mjs) |
| 研究证据、原文与附件 | [研究证据](research-evidence.md) | [证据工作台](../client/src/features/research/EvidenceWorkbench.tsx)、[检索](../server/src/evidence.rs)、[附件](../server/src/attachments.rs)、[研究仓储](../server/src/db/research.rs) | [工作台](../client/src/features/research/EvidenceWorkbench.test.tsx)、[引用展示](../client/src/features/research/ReportSources.test.tsx)、[附件 HTTP](../scripts/test-functional-api.mjs) |
| AI 冻结、授权、生成与输出校验 | [工作流契约](ai-workflow.md)、[数据边界](ai-data-boundary.md) | [分析用例](../server/src/application/analysis.rs)、[上下文](../server/src/context.rs)、[事实卡](../server/src/context/fact_sheet.rs)、[执行端口](../server/src/ports/analysis.rs)、[AI 实现](../server/src/ai/) | 对应 Rust 模块内的授权、指纹与输出回归，[托管分析隔离](../scripts/test-hosted.mjs)、[问答回归](../client/src/features/research/Advisor.test.tsx)、[真实模型质量评测及数据边界](../server/tests/fixtures/qa_eval/README.md) |
| 问答流式交互与分析档案 | [流式回复](ai-streaming.md)、[分析历史](analysis-history.md) | [问答页面](../client/src/features/research/Advisor.tsx)、[流式协议](../server/src/streaming.rs)、[历史契约](../server/src/models/analysis_history.rs) | [问答请求竞争](../client/src/features/research/Advisor.test.tsx)、[报告读取](../client/src/features/research/ModelReport.test.tsx)、[真实 HTTP](../scripts/test-hosted.mjs) |
| 本机模型执行与桥接 | [Codex／ChatGPT 桥接](codex-bridge.md) | [桥接用例](../server/src/application/codex_bridge.rs)、[worker](../server/src/codex_bridge_worker.rs)、[本机授权](../server/src/chatgpt_auth.rs)、[启动脚本](../scripts/codex-bridge.mjs) | [桥接界面](../client/src/features/settings/CodexBridge.test.tsx)、[启动测试](../scripts/codex-bridge.test.mjs)、`npm run test:codex-bridge`、`npm run test:chatgpt` |
| 可选网站 ChatGPT 身份登录 | [网站登录](chatgpt-sign-in.md)、[信任边界](threat-model.md) | [OIDC 校验](../server/src/openai_login.rs)、[网站适配](../server/src/hosted/openai.rs)、[账号映射](../server/src/accounts/openai.rs) | [登录界面](../client/src/features/account/OpenAiSignIn.test.tsx)、对应 Rust 模块内测试与[托管验收](../scripts/test-hosted.mjs) |
| 前端应用壳、共享展示与会话生命周期 | [架构前端地图](architecture.md#5-前端模块地图)、[模块边界](module-boundaries.md#前端)、[Web 构建](client-builds.md) | [应用壳](../client/src/App.tsx)、[应用层](../client/src/app/)、[共享组件](../client/src/components/)、[传输基础](../client/src/lib/)、[主题](../client/src/styles/) | [应用回归](../client/src/App.test.tsx)、[生命周期](../client/src/app/useApplicationData.test.tsx)、[架构检查](../scripts/check-architecture.mjs)、前端构建与测试 |

## 运行模块

| 模块 | 当前规范或操作指南 | 实现与验证入口 |
| --- | --- | --- |
| 宿主机启动、部署与 Web 发布 | [自建服务](self-hosting.md)、[部署验收](deploy-madeagents.md)、[Web 构建](client-builds.md) | [部署入口](../deploy.sh)、[主运行器](../scripts/host-runtime.mjs)、[Web 构建脚本](../scripts/build-web.mjs)、[部署测试](../scripts/test-deploy.mjs) |
| 整站加密备份、下载与恢复 | [数据迁移](data-migration.md)、[信任边界](threat-model.md) | [备份](../scripts/backup.mjs)、[下载](../scripts/download-backup.mjs)、[恢复](../scripts/restore-backup.mjs)、[独立加密备份验收](../scripts/test-backup.mjs)、[财务原生恢复验收](../scripts/test-financial.mjs) |

## 当前规范与历史记录的边界

当前观察规则按账号保存；默认值见[观察规则](buy-watch.md#明确规则)。个人备份版本与升级要求见[账号与同步](cloud-sync.md#备份版本)及[观察升级](buy-watch.md#数据升级与验证)，全局索引不重复维护版本数字。

已退役的 SQLite 运行模式、长期记忆写入和旧多阶段报告生成不恢复为当前入口；必要旧数据适配见[架构迁移边界](architecture.md#9-保留的迁移适配)。产品取舍以[最新讨论复核](audits/product-decisions-20261108.md)说明当前能力、候选方案与代价；[旧审查](audits/team-review-20261107.md#待讨论的产品取舍)保留当时依据。所有未获用户选择的建议仍未采纳，不作为当前 spec 或实施待办。

## 多人协作与验证

1. 从索引确定模块 spec、实现与测试，再说明本次增量。新模块或边界变化同次更新索引；调整已有模块不另建一份互相竞争的规范。
2. 并行工作按文件划分修改范围；共享契约、架构与跨模块文字由主负责人整合。修改前重新读取当前文件，保护其他在途工作。
3. 行为变更同步所属 spec；取消要求从当前文档删除，必要证据保留为历史。实现仍有偏离时明确记录，不通过改文案声称缺陷已解决。
4. 验证命令以根目录与前端的 [package.json](../package.json)、[client/package.json](../client/package.json)和[完整门禁](../scripts/test.sh)为准，具体要求见[项目规范](../AGENTS.md#4-验证要求)。构建产物的定位与完整门禁绑定见[模块边界](module-boundaries.md#开发流程)，实现及回归为 [server-artifacts](../scripts/server-artifacts.mjs) 与[子进程测试](../scripts/server-artifacts.test.mjs)。上表列的是现有验证依据，不代表每次文档整理都重新运行了全部测试。
5. 纯文档改动检查事实、相对路径与锚点、过时文案及 `git diff --check`；只有运行代码变化才增加对应业务验证。真实 OAuth、付费模型、联网来源、协议夹具与加密备份独立验收是否执行，须在交付中分别说明。

仓库 `main` 与远端同步不代表运行中的网站已部署该版本；本机其他 worktree 也可能是历史开发分支，不能当作当前规范来源。最新状态以交付提交及对应验证记录为准。
