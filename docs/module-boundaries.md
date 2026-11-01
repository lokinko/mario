# 模块边界与开发约束

## 设计选择

采用模块化单体，保持 React Web、Rust API、PostgreSQL 和无状态 Agent 的运行方式。业务边界通过明确的接口、依赖方向和状态所有权约束；独立模块可以单独测试，涉及多个领域的流程由应用层组合。当前规模下，增加服务进程、通用事件总线或每张表一个仓储接口只会增加维护成本。

## 依赖方向

```mermaid
flowchart TD
    App[前端应用壳] --> Features[功能页面]
    App --> Lifecycle[应用数据生命周期]
    Features --> API[领域 API 客户端]
    Lifecycle --> API
    API --> Transport[认证与 HTTP 传输]
    HTTP[多用户 HTTP 适配器] --> UseCases[应用服务]
    UseCases --> Repository[Database 领域仓储]
    UseCases --> Ports[AI 与行情执行接口]
    Repository --> Storage[PostgreSQL 存储适配器]
    Repository --> Domain[共享契约与确定性规则]
    Ports --> Domain
    UseCases --> Domain
```

| 层 | 拥有的职责 | 扩展约束 |
| --- | --- | --- |
| `client/src/features/` | 表单、页面请求状态、领域交互 | 不引用其他功能的页面实现；通过共享组件或应用壳组合 |
| `client/src/api/` | 某领域的 URL、请求和返回类型 | 不访问 DOM，不引用其他领域客户端，不自行处理账号与同步 |
| `client/src/lib/httpClient.ts` | 令牌、请求时限、写入前同步、响应版本与账号校验 | 不引用功能页面或领域客户端 |
| `client/src/app/` | 启动、全局同步、跨页组合与刷新 | 页面草稿保留在页面，应用层只记录是否需要保护 |
| `server/src/runtime.rs` | 环境与参数、维护命令、运行角色、退出信号 | 不放业务用例或 HTTP handler |
| `server/src/http/`、`hosted.rs` | 路由、认证、租户选择、版本检查、输入输出转换 | 多步业务流程交给应用服务；简单仓储查询和单次 CRUD 可直接调用 Database |
| `server/src/application/` | 冻结与保存分析、模型设置、行情辅助的组合写入 | 不依赖 Axum、启动入口或 Agent 进程适配器 |
| `server/src/models/` 与确定性计算模块 | 序列化契约、值校验、风险、规划、估值、上下文构建 | 不依赖仓储、网络刷新、工作流实现或 HTTP |
| `server/src/db/` | 领域事务、历史不可变性、同步导入和校验 | 不依赖应用流程、AI、HTTP 或行情网络服务 |
| `server/src/ai/` | 冻结输入上的工作流、模型 Provider、输出验证、执行接口 | 不读数据库或账号状态 |
| `server/src/agent.rs` | AI 执行接口的本地/远程适配和无状态 Agent HTTP 服务 | 无账户数据库、业务写入或用户会话依赖 |

`Database` 是领域仓储门面。表的内部协作、组合快照以及持仓成交和每日资产之间的事务是一致性边界，不能为了分文件而拆开事务。`models.rs` 和前端 `types.ts` 是共享契约入口，不复制跨域公共类型；只有具有明确所属职责的契约放入子模块。

`AppError` 负责序列化与底层错误转换；错误如何变成 HTTP 状态码、以及如何隐藏内部诊断，由 `http/error.rs` 决定。

## 状态与生命周期

### 服务端

- 启动配置属于进程。行情 Provider 和 AI 执行器属于进程依赖，可以替换；它们不持有账号数据库。
- 多用户请求先认证，再按服务端用户 ID 选择数据库连接。`RequestContext` 只组合该请求的账号数据库与共享服务依赖；客户端不能提供租户 ID。
- 公开行情缓存有界、共享并在失败时退避。用户汇率覆盖、持仓、密钥和分析历史归账号数据库，不进入跨账号缓存。
- 多用户请求在数据库账户锁内检查 `If-Match`。分析预览和模型连接测试不要求写入版本；CSV 预览会缓存汇率，所以仍按写入处理。新增操作默认要求版本，不能因为名字含 `preview` 或 `test` 就自动豁免。
- 分析执行在锁内冻结上下文、模型配置、密钥目的地和指纹；模型执行期间释放账户锁，执行结束重新取锁后保存。失败不产生分析历史；请求退出会关闭连接并释放残留的数据库会话锁。

- 组合快照先读取 `db/snapshot::SnapshotData`，`amounts::build` 生成精确投影，再调用一次 `build_snapshot`。AI 上下文消费结构化问题及规划范围，`structured_output` 校验结构与来源，不按固定中文动作判定有效性。数据问题只由 `valuation::issues` 判定，问题码、持仓 ID 和币种是公开契约；不得按文案筛选风险项或在仓储中事后清空规划。`planning` 拥有再平衡范围与精确金额计算，未平仓期权及标的只排除自身动作，分母仍用完整组合。

### 前端

- 功能页面拥有未保存输入和单页请求状态。`useApplicationData` 拥有已显示快照、可选模型配置、远程更新提示、页面刷新版本和同步控制器。
- 核心快照与模型配置分别加载，模型读取失败不阻止财务功能。刷新时可保留已可用的模型配置，同时记录其读取错误。
- 每次整体加载或刷新使用递增请求代次，较旧响应与卸载后的响应不得更新 UI 或发布数据更新事件。
- 自动刷新在请求前和返回后都检查草稿，避免“开始刷新之后才输入”的内容被覆盖。用户接受新资料后再清除草稿与写入保护；刷新失败继续保留输入。
- `httpClient` 捕获请求的账号与服务地址，发送前、收响应和采纳版本时都校验当前账号。金额快速编辑队列属于组合 API，排队任务保留原账号身份，不会在切换账号后写入新账号。
- 版本轮询和长时间分析返回的版本，不直接代替页面已核对的资料版本。普通写入、快照和汇率读取按请求策略采纳版本。
- 功能代码与测试直接引用领域 API 模块。财务回执未确认时 `financialRequest` 抛出 `FinancialRequestError`（`kind: "pending"`），页面经类型守卫保留原请求和输入，不匹配错误文案。
- `MyFacts` 组合问题导航和修复入口，`AssetAmountRow` 只显示属于该持仓（包括关联期权）的 `issues`；日期、参考价与来源使用中性样式。汇率修复复用 `ExchangeRates`，拆合股和交易链修复复用已展开的 `FinancialHistory`，不引入第二套修复状态。不可用会计投影的派生数量、价格和盈亏汇总隐藏，原始成交与审计记录仍保留；概览和趋势不推测缺失原因，具体原因归行内问题。
- `app/useDisplayPreferences` 管理本地涨跌色与金额隐藏偏好。功能页通过属性接收值和回调，共享 `DisplayPreferences` 表单不导入应用壳；显示控件标注 `data-presentation`，不将本地偏好视为业务草稿。
- `features/portfolio/useAssetHistory` 是资产页历史查询的唯一状态来源，总览与完整趋势共用请求、区间与分页；不得分别发起相同查询，或从持仓投影重算服务端总资产。
- 共享界面能力放在 `components/`，无领域 API 依赖。新增提示使用 `Callout`，状态使用 `Badge`，时点使用 `DataStamp`；行尾操作使用支持方向键、Esc 和焦点恢复的 `Menu`。抽屉内 Esc 先关闭菜单，再关闭抽屉。
- 新增金额输出使用 `data-money`，输入使用 `data-money-input`，数量与百分比保持可见；金额格式化仅作用于输出，原生数值输入保留源精度。颜色与字号只在 `styles/tokens.css` 定义。
- 新增页面沿用 1200/900/600px 断点，校验桌面、平板和手机；不得用全局最小宽度掩盖溢出。

## 简化原则

- 持仓、现金和收支只在“我的情况”编辑，风险偏好与基准币种放在收支区域的可选详情中。
- 问答和历史对话使用同一份报告展示与原始引用，不维护第二套报告、审计面板或无入口组件。
- 本地证据检索直接调用 `search_evidence`，只有一个实现，不预留动态接口。模型、远程 Agent 和行情有多实现接口。
- TypeScript 编译检查未使用的变量与参数；删页面时同时删专用组件、测试和失效样式。动态状态类需结合实际页面核对。
- 汇率入口属于侧栏状态区，打开共享抽屉；抽屉显示当前资产涉及币种及手动覆盖，编辑控件和其他币种按需展开。架构检查同时阻止没有任何生产入口的前端模块。

## 领域扩展约束

### 执行接口与适配器

- `ports/` 是外部执行契约层，只依赖领域契约、冻结上下文和错误类型；流式事件结构归 `models/inference.rs`。
- `application/portfolio.rs`、`fx.rs`、`prices.rs` 只使用行情端口；`composition.rs` 负责实际供应商装配与生命周期；`market_data.rs`、`prices.rs` 不读写账号数据，也不持有静态可变资源。
- 账号凭据读写归 `db/secrets.rs`，`secrets.rs` 不依赖仓储。
- 功能页不引用 `app/`（包括导航类型），通过回调通知应用壳：参考资料与复盘使用 `onAsk(question)`，问答使用 `onOpenSettings()`。
- `api/<领域>.ts` 可引用 `api/<同领域>/` 内部实现，禁止跨领域 API 互相引用。财务回执恢复只属于组合 API，`lib/httpClient.ts` 不解释财务回执或组合快照。
- `storage.rs` 是唯一使用数据库驱动的模块，提供自有的 `Value`、`ToSql`/`FromSql`、`Row` 与错误类型；仓储 SQL 使用原生 PostgreSQL 写法。账号锁、版本检查和财务回执只有多用户一条路径，本地开发 `npm run dev` 走同一认证与租户流程。
- Rust 检查解析相对 `super::` / `self::` 路径，防止以相对导入隐藏反向依赖；分析生命周期不得导入 AI 工作流实现。

### 期权

期权契约、合约计价及多空头重放归 `models/options.rs`；`db/options.rs` 管理标的关联、独立权利金报价与标的汇总投影，不联网。合约复用持仓、成交、现金基准及财务回执；`db/trades.rs` 验证公共成交形状后，经 `db/corporate.rs` 按合约类型分派到股票或期权计算，不把股票拆股重放套用到期权。股票股数和成本仍只由股票成交维护。

前端 `features/portfolio/StockOptions.tsx` 拥有合约与报价草稿，复用 `HoldingTrades` 的成交提交与未知结果核对。`HoldingDetail` 协调股票、期权表单的互斥编辑，应用壳继续管理快照。`stockOptionTotals` 只供标的分组展示，不回写股票估值或再次累加组合资产。新增财务入口仍受现有全局 `If-Match` 保护，不增加豁免。

新增或修改期权行为需验证：多空头现金符号、非 100 乘数、分次开平仓成本、到期零权利金、同账户标的关联、禁止超额平仓和跨合约混算、报价回执、审计失败回滚、v23 备份与早期备份恢复、含空头的每日资产，以及股票列表分组不重复计数。完整 `npm test` 已包含真实 HTTP 期权流程和原生 PostgreSQL 备份恢复入口。

### 投资方法论

- `features/methodology` 只依赖设置 API、共享契约与展示组件；应用壳提供左侧导航和保存后的草稿清理回调，不引用问答页内部状态。编辑时沿用应用层远程更新保护，未保存内容不进入问答。
- `models/methodology.rs` 定义初始范本、正文契约与 4000 字符上限。`db/settings.rs` 按当前账号保存，HTTP 使用认证、账号锁及 `If-Match`，没有绕过版本检查的例外。
- `application/analysis.rs` 读取方法论，`context.rs` 将正文纳入冻结上下文与指纹；`ai/workflow.rs` 只消费冻结值，格式修复只消费原输出与校验错误。固定系统约束只负责数据解释、来源与输出契约，投资原则在可编辑的初始范本中。
- 账号备份沿用 settings 表结构，`sync_validation.rs` 校验设置白名单与方法论正文，`fx.rs` 只验证汇率设置。备份没有方法论时使用初始范本；导入失败整次回滚，不覆盖现有正文。

### 问答报告、事实卡与追问

- `models::StructuredAnalysis` 是生成契约；`models/analysis_history` 的独立类型只读历史报告，仓储以 `headline()` 获取两类报告的列表摘要，不转换历史正文。
- `ai/structured_output` 校验字段、数量、字数、事实索引和引用，拒绝缺少原值引用的个人事实；路径校验保证存在与授权，不替代语义核对。`ai/streaming` 只投影报告的用户可见字段。`features/research/ModelReport` 共享来源抽屉，按报告形状读取历史或显示新回答。
- `context/fact_sheet` 是领域层确定性投影：精确金额只输出一次，结余、占比、偏差和浮盈使用 Decimal；组合合计缺失时保留空值与结构化影响范围，其他持仓及现金继续计算。不重算或写入财务事实。`application/analysis` 不查询个人规则、复盘和每日资产。
- `BuiltContext` 区分模型 payload 与授权估值审计，二者共同冻结指纹；模型只消费紧凑 payload，估值链进入轨迹。预览的事实卡与模型使用的卡一致。
- `features/research/previousTurn` 按 Unicode 字符截取上一轮问题、结构化结论和最多 3 条动作（额度 200/300/每条 100）；纯文本历史记录最多取 300 字。`models::PreviousTurn` 校验总计 800 字符、非空正文及最多 3 条建议；`ContextBuilder` 单独冻结它，应用层保存当前 question，不把历史模型内容拼进问题。输出引用只允许当前问题与已授权事实卡。新对话清空摘要。
- `evidence::search_evidence` 取前 5 条，`relevant_excerpt` 返回最多 2000 字连续原文；`application` 读取并授权候选，`context` 冻结原文与片段，AI 不读仓储、不额外筛选。网页搜索由用户主动开关授权，默认关闭，原生搜索结果直接进入生成。

## 开发流程

离线研究工具位于 `benchmark/`，契约与入口见 [benchmark](benchmark.md)。免费来源覆盖探测归 `public_sources.py`，授权来源与原件下载归 `sources.py`，确定性选样/时点规则归 `dataset.py`，指标归 `evaluate.py`，发布格式归 `release.py`，股票池共用的按时点市值口径、剔除原因与行情/股本缓存归 `market_cap.py`，科技股池候选归 `tech_universe.py`，红利股池成分重建归 `dividend_universe.py`（两个池子互不引用，新增股票池同样只依赖 `market_cap.py` 并产出同构的 `universe/<池>-<年>.json`，再在 `pool_release.POOLS` 登记），旧版 Excel 解析归 `xls.py`，多池切片构建归 `pool_release.py`，主指标（收益得分与胜率）打分、卖出规则与网格执行归 `scoring.py`（不访问网络），策略发现、静态检查、子进程生成仓位与结果缓存归 `strategy_runner.py`，策略本身是 `strategies/<名字>/` 下的 `strategy.md` 与只实现 `decide(sample)` 的 `strategy.py`（新增策略只加目录，不改其他模块；策略代码只能导入纯计算标准库），策略得分看板与本机看板服务归 `dashboard.py`（只读 `bench-run` 输出的成绩与说明，不读输入与标签），Hugging Face 上传归 `hub.py`，CLI 只装配流程；这些模块只依赖 Python 标准库和本包，不引用在线业务实现或账号数据。第三方依赖只允许出现在单一适配器中：`baostock_market.py` 是唯一导入 `baostock` 的模块，`hub.py` 是唯一导入 `huggingface_hub` 的模块，依赖分别在 `pyproject.toml` 的 `market`、`publish` 依赖组声明，其他模块不得直接导入；新增此类适配器需同步更新边界测试的白名单。`npm run test:benchmark` 验证该隔离边界与数据行为，并纳入 `scripts/test.sh`。新增来源不能把未经证实的披露时间或供应商缺失自动升级成已核对事实；发布目录不可覆盖，原始数据修正需生成新版本，未来接入在线问答需走现有冻结和授权接口。

批量免费补采由 `collect.py` 拥有任务进度、固定请求与断点恢复，调用 `public_sources` 的目录和行情能力；`files.symbol` 明确支持沪深北代码，不按首位数字或默认深市猜测交易所。`universes/` 的当前成分清单只用于候选数据预采，不替代 `dataset` 的历史选样。价格口径不可隐式拼接，日历闭市与真实交易日缺口分别计数；测试覆盖节假日排除、范围截断、标题简写、来源隔离和恢复校验。

新增功能时先确定数据与事务的所属领域，再放置共享契约和仓储操作。需要调用市场接口、检索多类资料或执行模型时，在 `application` 增加用例，并从 HTTP 装配执行依赖。HTTP handler 只解析请求、调用该用例并返回结果。

前端在对应 `api` 模块增加接口，功能页面维护自身交互。多个功能需要同屏出现时由应用壳组合；仅供显示、无功能生命周期的组件可以进入 `components`。已有明确边界的单步操作不必再增加一层空壳服务。

开发与测试需要本机 PostgreSQL 工具（从 `MARIO_TEST_PG_BIN` 或 `pg_config --bindir` 查找）。Rust 测试与 HTTP 脚本只连接 `MARIO_TEST_DATABASE_URL` 指向的一次性数据库；`scripts/postgres-cluster.mjs <命令…>` 创建临时集群、设置该变量并在命令结束后删除集群。

提交前执行：

```bash
npm run check:architecture
npm run build --prefix client
npm run format:check --prefix client
npm test --prefix client
cargo fmt --manifest-path server/Cargo.toml -- --check
npm run test:server
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
cargo build --manifest-path server/Cargo.toml
```

跨前后端、数据格式、财务逻辑或公共契约的修改运行完整 `npm test`（[`scripts/test.sh`](../scripts/test.sh)）：上述检查之后，在同一个临时集群中依次运行 Rust 测试、`test-functional-api`、`test-web`、`test-codex-bridge`，再运行独立建集群的 `npm run test:financial`。

依赖检查解析 TypeScript 导入/导出，包括类型导入与动态导入；Rust 检查去除字符串、注释与测试专属项后提取显式 crate 依赖。它阻止上层反向引用、跨功能 UI 引用、领域客户端之间的引用以及依赖循环。DB 内部实现仍允许使用父模块门面的共享导入。检查属于静态约束，与编译及行为测试互补，不代表所有宏展开或运行时隔离已经被证明。
