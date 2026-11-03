# 架构

本文是全局代码设计的总体说明：系统由哪些运行单元组成、源码如何分层、依赖朝哪个方向、状态和事务归谁、关键数据怎样流动，以及哪些不变量不能被局部改动破坏。修改跨模块结构前先读本文；具体模块的扩展步骤与开发流程见 [模块边界与开发约束](module-boundaries.md)，部署与运维见 [自建宿主机服务](self-hosting.md)。

本文只描述当前生产路径。长期有效的架构变化必须在同次提交中更新本文与 [模块边界](module-boundaries.md)；一次性的验收数据不写入本文。

## 1. 系统形态

项目是模块化单体：一个 Rust 二进制按启动参数承担不同角色，一个 React 单页应用，一个 PostgreSQL。没有微服务、消息总线或持久作业队列。

```mermaid
flowchart LR
    W[React Web] --> G[Nginx HTTPS]
    G --> A[Rust API<br/>认证 · 租户选择 · 业务用例 · 事务]
    A --> P[(PostgreSQL<br/>账号表 + 每账号独立 schema)]
    A -->|冻结上下文| R[无状态 AI Agent]
    R --> M[模型提供方]
    A -->|公开行情| Q[ECB / 免费行情 / Twelve Data]
    B[个人电脑桥接进程] -->|HTTPS 长轮询| A
    B --> C[本机 Codex App Server]
    B -->|本机 OAuth 令牌| O[官方 OpenAI Responses API]
    L[系统浏览器] -->|OAuth 回环回调| B
```

同一个 `mario-server` 二进制由 [`runtime.rs`](../server/src/runtime.rs) 选择运行角色：

| 角色 | 触发条件 | 入口 | 数据 |
| --- | --- | --- | --- |
| 多用户 API（默认） | 必须设置 `DATABASE_URL` | `hosted::serve` | PostgreSQL，按账号 schema 隔离 |
| 无状态 Agent | `MARIO_ROLE=agent` | `agent::serve` | 无数据库，只接收 API 发来的冻结任务 |
| Codex 桥接 worker | `--codex-bridge <站点>` | `codex_bridge_worker::run` | 用户电脑执行，不监听桥接端口，不读 `auth.json` |
| ChatGPT 官方桥接 worker | `--chatgpt-bridge <站点>` | `codex_bridge_worker::run_chatgpt` | 用户电脑执行；OAuth 令牌留在本机，直连官方 Responses |
| 本机 ChatGPT 账号管理 | `--chatgpt-auth <命令>` | `chatgpt_auth::command` | 浏览器 OAuth 仅回环回调；注册、刷新和退出；不连接业务数据库 |
| 维护命令 | `--reset-password` | `runtime.rs` | 重置指定账号密码 |

PostgreSQL 是唯一的数据存储，没有单机或免登录模式。本地开发（`npm run dev`）与测试使用同一条多用户路径：[`scripts/postgres-cluster.mjs`](../scripts/postgres-cluster.mjs) 在 `.runtime/dev` 维护持久开发集群，或为测试创建一次性集群；开发者在浏览器注册账号，与生产流程一致。

生产部署由 [`deploy.sh`](../deploy.sh) 在宿主机以 systemd 启动一个主运行器（[`scripts/host-runtime.mjs`](../scripts/host-runtime.mjs)），依次拉起 PostgreSQL、Agent（仅监听本机端口）和 API；Nginx 反向代理 API 与静态网页。推送代码不等于部署。

部署中的系统依赖安装与 systemd 管理使用 root；用户工具安装、构建和主运行器统一使用非 root 服务账号。`deploy.sh` 在下载工具和停服前准备该账号的 home，自动修复默认专用 home 与 Rust/uv 工具目录的历史归属、所有者写入及遍历权限，不递归接管整个 home 或跟随工具链接修改外部文件。已有数据库继续使用原所有者，数据与事务归属不变。具体范围与异常处理见 [部署权限](deploy-madeagents.md#服务用户-home-与工具目录权限)。

## 2. 设计原则

1. **服务端是唯一事实来源。** 前端只持有已显示快照和未保存草稿；金额、会计结果、估值状态和数据版本都由服务端计算并返回。
2. **账号隔离由服务端决定。** 请求不能指定租户；认证后由服务端按用户 ID 选择数据库连接，账号数据不进入跨账号缓存。
3. **写入有版本，财务写入有回执。** 业务写入与数据版本在同一事务推进；客户端用 `If-Match` 提交，过期写入返回 409；财务写入的回执与领域变化同事务提交，丢失响应可以安全重放。
4. **精确金额。** 金额以十进制原文和精确值持久化，浮点只作展示投影；前端显示不经过二进制浮点重算。
5. **AI 只消费冻结输入。** 分析前在服务端读取、授权、冻结上下文并计算指纹；AI 模块不读数据库、不写资产、不执行交易，模型输出必须经过结构校验才成为结果。新生成的每条个人事实须有非空且授权的原值引用；这保证可回查，不代替语义核对，历史报告保留原文读取。
6. **可替换能力经端口注入。** 模型执行与行情查询通过 `ports/` 中的 trait 注入，测试可以替换；不为普通仓储查询或单步 CRUD 增加抽象层。
7. **单一生产路径。** 非兼容更新删除旧实现；只为存量数据恢复保留隔离的迁移适配，并写明移除条件（见第 9 节）。

## 3. 分层与依赖方向

```mermaid
flowchart TD
    subgraph Client[client/src]
        Shell[App.tsx · app/<br/>应用壳与数据生命周期] --> Features[features/&lt;领域&gt;/<br/>页面、表单、草稿]
        Shell --> API[api/&lt;领域&gt;<br/>请求与返回契约]
        Features --> API
        Features --> Shared[components/ · lib/ · types.ts]
        API --> Shared
    end
    subgraph Server[server/src]
        Runtime[runtime · lib · main<br/>启动与模式选择] --> Adapters[http/ · hosted · agent<br/>composition · streaming]
        Adapters --> App[application/<br/>业务用例与编排]
        Adapters --> DB
        App --> DB[db/ · storage<br/>领域仓储与事务]
        App --> Ports[ports/<br/>执行接口]
        Adapters --> AI[ai/<br/>工作流 · Provider · 输出校验]
        AI --> Ports
        DB --> Core
        App --> Core
        Ports --> Core[models/ · context · planning<br/>risk · valuation · evidence<br/>契约与确定性规则]
        AI --> Core
    end
    API -->|HTTPS JSON / SSE| Adapters
```

箭头表示"可以依赖"。底层（契约与确定性规则）不依赖任何上层；`ports/` 只依赖契约，实现放在适配器中；`application/` 不认识 Axum、Agent 进程或具体行情供应商。简单仓储查询与单次 CRUD 允许沿用 HTTP → `Database` 的直接路径。

`npm run check:architecture`（[`scripts/check-architecture.mjs`](../scripts/check-architecture.mjs)）静态强制以下规则，并检测前后端依赖循环：

| 范围 | 禁止 |
| --- | --- |
| 前端功能页 | 引用 `app/`、`App.tsx`、`main.tsx`；引用其他功能目录 |
| 前端共享层 | `lib/`、`components/`、`api/`、`types.ts` 引用功能页或应用壳；`lib/` 引用 `api/`；`api/` 跨领域互相引用 |
| 前端可达性 | 任何生产模块无法从 `main.tsx` 到达 |
| 契约与确定性规则 | `models`、`context`、`planning`、`risk`、`valuation`、`evidence` 依赖仓储、网络、传输、`application`、`ai` 或 `ports` |
| `ports` | 依赖 `models`、`context`、`error` 以外的模块 |
| `ai` | 依赖仓储、账号、行情供应商、传输适配器或 `application` |
| `db` | 依赖 `application`、`ai`、`agent`、`http`、`hosted`、`runtime` 或行情网络模块 |
| `application` | 依赖传输/进程适配器或具体行情供应商；`application/analysis.rs` 直接引用 `ai` |
| `agent` | 依赖 `db`、`storage`、`accounts`、`application`、`http`、`hosted`、`runtime` |
| 本机 OAuth | `chatgpt_auth` 依赖业务、账号或 AI 实现；在线 HTTP/Agent、领域及仓储依赖本机凭据；`ai/chatgpt.rs` 以外的 AI 模块读取本机 OAuth |
| 网站 OIDC | `openai_login` 依赖账号仓储、模型执行或本机授权；`hosted` 以外的模块依赖网站 OIDC |
| `secrets`、行情适配器 | `secrets` 依赖 `error` 以外模块；`prices`/`market_data` 依赖 `models`、`ports`、`error` 以外模块或持有静态可变资源 |
| 通用 | 非 `db` 模块使用通配导入；契约、仓储、应用层使用 `axum`；`application`/`ports` 直接使用 `reqwest`；`storage.rs` 以外直接使用数据库驱动 |

静态检查与编译、行为测试互补，不能证明宏展开或所有运行时路径的隔离。新增或调整边界时同步更新检查器及其测试，不通过扩大豁免掩盖耦合。

## 4. 后端模块地图

| 模块 | 职责 |
| --- | --- |
| `main.rs`、`lib.rs`、`runtime.rs` | 进程入口、模块声明、环境与参数解析、模式选择、维护命令、优雅退出 |
| `composition.rs` | 创建行情供应商，组装 `MarketServices`，拥有后台刷新任务并在服务退出时取消 |
| `hosted.rs` | 多用户模式：公开注册/登录路由、认证中间件、并发槽位、账号锁、`If-Match` 版本检查、财务回执重放、导出/导入、CORS、静态网页，以及只读提供研究看板文件（`MARIO_BENCH_DIR`，数据接口只验证会话，不打开账号库） |
| `http/` | 按领域注册路由（`portfolio`、`research`、`reviews`、`settings`、`codex_bridge`）并转换输入输出；`policy.rs` 定义免版本检查的显式例外；`error.rs` 把领域错误映射为 HTTP 状态 |
| `accounts.rs`、`accounts/openai.rs`、`authentication.rs` | 自建账号、Argon2 密码、随机会话（库内只存哈希）、撤销与登录限速；可选网站 OpenAI 身份映射、短期 OAuth 事务及显式关联；Agent 令牌的常量时间比较 |
| `openai_login.rs`、`hosted/openai.rs` | 网站身份登录：前者只做官方 OIDC 配置、授权码交换和签名声明校验，不访问账号或模型；后者转换 HTTP/Cookie、编排账号事务、固定站点重定向与一次性交接；与本机 `chatgpt_auth` 及订阅执行无依赖 |
| `application/` | 跨仓储与外部端口的用例：`analysis`（冻结、预览、执行、保存）、`portfolio`/`prices`/`fx`（行情刷新与估值编排）、`model_settings`（模型接口切换策略、Provider 构造与连接测试）、`codex_bridge`（账号配对） |
| `ports/` | `analysis::AnalysisExecutor` 与事件通道；`market::FxRateProvider`、`SecurityPriceProvider` |
| `agent.rs` | `AgentExecutor`：配置 `MARIO_AGENT_URL` 时把冻结任务发给独立 Agent，否则进程内执行；Codex 桥接任务始终在 API 进程经桥接中继执行。同文件提供 Agent HTTP 服务 |
| `ai/` | `orchestrator` 按 `workflow`（当前 v8）执行可选搜索与单次报告生成；格式错误只修复一次；`provider` 定义模型接口，API 侧实现为 `native_provider`（供应商 HTTP 接口）与 `bridge`（经配对桥接中继），`codex` 和 `chatgpt` 只由用户电脑上的桥接 worker 调用，分别使用 Codex 登录和 Mario 自己的官方 OAuth 注册；`structured_output` 负责确定性引用、输出校验；资料检索和连续原文摘录归领域层 `evidence`；`streaming` 解析流式草稿 |
| `codex_bridge_worker.rs` | 用户电脑上的共用桥接进程，固定注册及授权会话，长轮询 API 领取任务，选择 Codex App Server 或官方 ChatGPT Responses；模型目录及推理取令牌时在凭据锁内核对同一会话，不借用重新登录后的授权；不上传 OpenAI 凭据 |
| `chatgpt_auth.rs`、`chatgpt_auth/` | 本机官方 OAuth、签名身份和 scopes 校验、多注册选择、跨进程串行刷新、原子凭据保存及退出；仅由本机运行角色与 `ai/chatgpt.rs` 调用，不依赖业务数据库 |
| `market_data.rs`、`prices.rs` | 行情端口的网络实现与实例内有界缓存；只处理公开行情，不读写账号数据 |
| `db/`、`storage.rs` | `Database` 门面下的领域仓储（持仓、成交、期权关联与报价、现金、公司行动、金额、每日资产、汇率、研究、复盘、设置、凭据）、财务回执与日志、快照构建、同步/备份格式与校验、schema 迁移；`storage.rs` 是唯一的 PostgreSQL 驱动适配：专用客户端线程、动态类型行、事务与 `search_path` 绑定的账号 schema |
| `models.rs`、`models/` | 序列化契约、值校验、精度与日期规则、期权合约计价及多空头重放、推理事件与执行时限 |
| `context.rs` / `context/fact_sheet.rs`、`planning.rs`、`risk.rs`、`valuation.rs`、`evidence.rs` | 确定性计算：分析上下文构建与预览、再平衡规划、风险检查、估值状态、本地证据检索 |
| `attachments.rs`、`secrets.rs`、`streaming.rs`、`error.rs` | 附件解码与文本提取、主密钥加解密（AAD 绑定 schema 与用途）、共享 SSE 响应、统一错误类型 |

## 5. 前端模块地图

| 模块 | 职责 |
| --- | --- |
| `main.tsx`、`App.tsx` | 挂载、页面组合与导航；跨页动作（如 `onAsk`、`onOpenSettings`）由应用壳接收并转发 |
| `app/useApplicationData.ts` | 服务端数据生命周期：启动、快照与模型配置加载、自动同步、请求代次、草稿保护与远程更新提示 |
| `app/navigation.ts` | 视图、导航分组与标签；“研究 → 研究看板”以新标签页链接打开同源独立 dashboard，不增加 React 视图状态 |
| `features/portfolio` | 我的情况：持仓、成交、股票关联期权与报价、现金、汇率、每日资产、资产历史与财务日志 |
| `features/methodology` | 左侧独立入口“投资方法论”：编辑账号专属提示词、保存及填入初始范本；页面持有草稿，应用壳负责同步保护 |
| `features/research` | 问答（流式回复、报告、引用来源）与研究证据工作台 |
| `features/reviews`、`features/settings`、`features/account` | 复盘；模型设置、问答预设与 Codex 桥接；网页账号与服务端数据备份 |
| `api/<领域>.ts` | 领域请求与返回契约；`api/portfolio/financialRequest.ts` 负责财务回执核对与快速编辑排队 |
| `lib/httpClient.ts`、`transport.ts`、`requestPolicy.ts` | 认证、超时与只读重试、版本采纳、账号切换保护、请求类别 |
| `lib/autoSync.ts`、`syncEvents.ts`、`webSession.ts`、`useRequestGuard.ts` | 版本轮询、数据事件、网页会话与多账号、过期响应抑制 |
| `components/`、其余 `lib/`、`types.ts` | 无功能生命周期的展示组件、格式化与日期工具、共享契约类型 |

### 界面与显示约定

- `main.tsx` 先加载 `styles/tokens.css`，再加载基础样式和工作区样式。颜色、字体、字号、间距、圆角和动效使用统一变量；沿用 React、原生 CSS 和 lucide，不请求外部字体、不引入 UI 或图表框架。
- `app/useDisplayPreferences.ts` 拥有浏览器本地显示偏好，通过受控属性传给资产页和共享设置表单，再更新根节点 `data-pnl`、`data-mask-amounts`。默认红涨绿跌，可切换绿涨红跌；盈亏统一使用 `--up/--down` 并配符号，买卖方向用中性描边。偏好不进入账号数据库、备份或写入版本。
- `lib/format.ts` 直接对十进制源字符串舍入和分组，金额按币种精度补齐（通常两位，JPY 为零位），数量和价格可显式指定精度，负号使用 `−`。数字列右对齐并采用等宽数字；数值输入保留原文，不把千分位文本写进原生数值控件。
- 结构化金额使用 `data-money`，金额输入使用 `data-money-input`；隐藏金额只影响显示，原文、复制和导出仍含真实数据。它不构成数据脱敏或加密。
- `Callout`、`Badge`、`DataStamp`、`Menu`、`Skeleton`、`EmptyState` 和 `Drawer` 只提供共享展示与可访问交互。菜单支持键盘与关闭后焦点恢复；抽屉保留草稿保护、忙碌锁定、背景隔离和焦点约束。
- 同步入口集中在侧栏状态块，移动端顶部显示同一状态。汇率由侧栏入口打开抽屉。页面内容上限 1440px，阅读和表单页上限 960px。
- 数据问题只在相关持仓行显示一次，按 `IssueCode` 提供汇率、拆合股、交易链或存款核对入口；总览显示问题数量并定位到该行。日期、来源和参考价使用中性元数据，其他币种存款与常用币种共用列表。缺汇率或持仓金额不可用时不以旧历史总值替代当前总值。
- `features/portfolio/useAssetHistory` 持有单页历史请求、区间与分页状态，供 `AssetOverview` 与 `AssetHistory` 共享。资产总览读取服务端每日精确总额或会计快照，趋势默认展开；前端只投影曲线坐标，不新增会计合计、持仓占比或汇兑计算。
- 问答提供实际资料预览，已完成报告的引用在抽屉查看；流式阶段只映射服务端状态，未知阶段保留原文。来源、方法论版本等元信息仅显示契约中实际存在的字段。

## 6. 关键数据流

### 6.1 多用户请求生命周期

1. Nginx 转发到 API；公开路由包含注册、登录、登出、可选 OpenAI 网站登录配置／发起／回调／交接和服务信息，请求体上限 16 KB。OpenAI 发起和交接接口校验 Origin，关联还必须提供有效 Mario 会话；回调只信任浏览器绑定且可一次性消费的 state。
2. 私有路由先取并发槽位（业务 24、认证 2），满载返回繁忙。
3. 校验 Bearer 会话，按用户 ID 打开该账号 schema 的连接，取数据库 advisory 账号锁并读取数据版本。
4. 写入请求（由 `http/policy.rs` 判定）比较 `If-Match`；不一致时，财务写入先查回执，命中则返回原结果，否则返回 409 与当前版本。
5. 构造只含该账号连接与共享依赖的 `RequestContext`，交给领域路由；响应带 `x-data-revision` 与 `no-store`。连接关闭时释放残留的会话锁。

网站身份登录由 API 验证 OpenAI 身份后建立原有 Mario 会话。`mario_auth` 的外部身份按 issuer + client ID + subject 映射到 users；邮箱碰撞需要原账号登录后明确关联，不自动合并。10 分钟 OAuth 事务与 120 秒会话交接均由 PostgreSQL 原子消费，HttpOnly 临时 Cookie 绑定浏览器；回调整页返回固定站点，交接只返回 Mario 自有会话。OpenAI 原始令牌只在校验期间存在内存，不持久化或返回网页。流程及申请见 [网站 ChatGPT 登录](chatgpt-sign-in.md)。

### 6.2 AI 分析

```mermaid
sequenceDiagram
    participant H as http/research
    participant S as application/analysis
    participant D as Database
    participant E as AnalysisExecutor
    participant G as Agent / ai
    H->>S: 预览或执行请求（持有账号锁）
    S->>D: 读取授权资料、模型配置与密钥目的地
    S->>S: ContextBuilder 冻结上下文并计算指纹
    S-->>H: 预览返回；执行时释放账号锁
    S->>E: 冻结上下文 + 模型配置
    E->>G: 独立 Agent、进程内或 Codex 桥接
    G-->>E: 经校验的报告与轨迹（可流式）
    S->>D: 重新取锁，校验输入版本后保存分析历史
```

预览与执行共享 `AnalysisService::prepare`，避免授权与指纹出现两套实现。执行失败不写历史；外部模型运行期间不持有账号锁。AI 上下文统一使用 `facts` 事实卡，精确金额去重、比例在领域层计算，`facts.dataIssues` 合并问题码、影响持仓、计算范围与期权再平衡排除信息；原始档案、会计双份投影、规则、复盘与每日资产不进入上下文。估值计算链和研究原文分别冻结在 `BuiltContext.valuationEvidence`、`evidenceCatalog` 并写入轨迹审计；模型输入使用紧凑 JSON，研究资料以关键词取前5条、每条最多2000字连续原文，原文与模型片段共同冻结指纹；输出校验保留结构、证据 ID 与原值引用规则，不强制动作逐字匹配固定文案。追问单独冻结 previousTurn（上一轮问题、结论与动作，总计至多 800 字符），当前 question 不拼接历史；历史摘要不可作为事实引用。历史分析冻结上下文不改写。新报告使用回答、建议、事实和可选追问的最小契约；`models/analysis_history` 隔离历史报告的读取类型，生成只接受当前契约。

投资方法论通过 `GET/PUT /api/investment-methodology` 管理，正文与初始范本契约归 `models/methodology.rs`，使用账号 schema 内的 `settings[investment.methodology]` 保存。没有存量值时返回初始范本，不执行隐式写入。准备时服务端读取已保存正文并纳入冻结 payload、预览指纹和历史轨迹；分析各阶段将该正文作为方法论指令，数据解释、来源校验与输出格式仍由系统约束。更改方法论使旧预览失效，已启动分析继续使用冻结版本。

### 6.3 行情与汇率

`composition::MarketRuntime` 在进程启动时创建供应商并定时刷新公开行情；`application/prices.rs`、`fx.rs` 通过端口查询并在账号事务中写入估值与汇率来源。快照、版本与历史汇率的 GET 不触发刷新或写入；持仓编辑的外部查询在提交事务前完成，成交保存不依赖外部行情。

### 股票与关联期权

期权以 `holding_options` 绑定账号内同账户、同币种的标的股票，自身复用持仓和成交。`models/options` 按整数张数、每单位权利金及显式合约乘数独立重放：多头为权利金资产，空头为有符号负债，不混入股票股数与成本。`db/options` 维护不可变合约定义、独立人工报价及 `stockOptionTotals` 展示投影；现金、审计、回执、每日观测与数据版本沿用同一仓储事务。组合会计按底层资产计数一次，股票列表按标的汇总显示。普通股票行情与拆股计算不得作用于期权；未平仓期权及其标的从再平衡目标排除，其他持仓仍按完整组合总值计算偏差，行权、被指派、保证金及合约调整尚无自动处理路径。到期作废只接受到期日经核对后的显式记录。每日观测冻结合约元数据，备份 v23 包含关联，旧版无期权备份可事务恢复。

快照构建先读取全部 `SnapshotData`，由 `amounts` 生成精确会计投影，`valuation::issues` 唯一判定问题，再单次构建快照；`planning` 根据问题码、完整组合分母及期权排除清单计算精确再平衡。目标权重合计不为 100%（十进制容差 1e-8）只阻止再平衡，通过 `invalidTargetWeights` 和 `targetTotalPct` 表达。规划日期覆盖参与分母的全部持仓，包括只排除动作的期权及标的。不可用持仓的列表、详情和交易汇总不展示不可靠的派生数字，原始成交仍可核对。快照派生字段不持久化，不改变备份格式。

### 6.4 同步、备份与恢复

前端按版本轮询，版本变化后才拉取快照，没有离线写入队列或实时推送。投资方法论随账号备份的 settings 行导出，恢复前校验非空及 4000 字符上限；缺少该项的旧备份恢复为初始范本。沿用现有备份结构，不新增 schema 迁移；恢复失败整体回滚。账号导出/导入使用版本化的同步数据集（`db/sync_format.rs`），导入在单一事务内校验表关联、精确金额与财务一致性后整体替换，失败整次回滚。宿主机级加密备份与迁移见 [服务器间打包、下载与恢复](data-migration.md)。

## 7. 状态与事务归属

| 状态 | 所有者 | 说明 |
| --- | --- | --- |
| 启动配置、行情供应商、AI 执行器 | 进程（`runtime`、`composition`） | 可替换，不持有账号数据 |
| 公开行情缓存 | `prices`/`market_data` 实例 | 有界、失败退避，跨账号共享，只含公开数据 |
| 会话与账号 | `accounts` | 会话仅存哈希，可撤销；网站 OpenAI 身份映射、账号创建／关联／会话签发归同一事务；短期 OAuth 事务原子消费后不重放 |
| 账号业务数据与版本 | `db` 领域仓储 | 业务写入、回执、日志、投影与版本同事务提交 |
| 模型配置与凭据 | `db/settings.rs` + `db/secrets.rs` + `secrets.rs` | 接口、地址、模型名、密文与目标绑定由 `save_model_configuration` 在同一事务写入，失败保留原配置；主密钥加密，AAD 绑定账号 schema 与用途 |
| 冻结分析输入 | 单次分析请求 | 执行期间不可变，结果记录输入版本与轨迹 |
| 桥接配对 | `application/codex_bridge` + `ai/bridge` | 只存配对摘要与短期通道，不接触 OpenAI 凭据；沿用 `codex-bridge` 协议，不区分本机登录实现 |
| ChatGPT 官方注册与令牌 | 用户电脑的 `chatgpt_auth/store` | 系统用户专属配置目录；稳定 host ID、按 subject + issued client ID 分开的注册；锁内轮换和原子替换；不是业务数据库或账号备份 |
| 已显示快照、同步状态 | `app/useApplicationData` | 请求代次防止旧响应覆盖新数据 |
| 显示偏好 | `app/useDisplayPreferences` | 当前浏览器 localStorage；不触发同步、账号写入或数据迁移 |
| 未保存草稿与单页请求状态 | 各功能页 | 应用层只知道是否需要保护 |

## 8. 关键不变量

- 账号之间不共享连接、缓存中的业务数据或密钥；客户端无法选择租户。
- 数据库模块不联网，AI 模块不访问数据库，Agent 不需要数据库连接或账号服务。
- 多步财务写入是单一领域事务，不能为拆分文件而拆开。
- 只停用无法正确计算的部分。缺汇率、待确认拆合股、隔离交易使组合总值与再平衡不可用；其他持仓仍显示原币金额。存款待核对不阻止计算，日期、参考价和手工来源只作元数据。

### 财务一致性约束

| 风险 | 领域边界与必须保持的不变量 | 验证入口 |
| --- | --- | --- |
| 历史成交重复扣现金 | cash_state 基准与 trade_settlements 逐笔归属；历史补录不结算，旧更正不推翻已核对余额 | financial_tests 的现金基准测试、HTTP 流程 |
| 异常链被删除 | trading_quarantine 保留原文；隔离暂停计算，预览后显式修复，永久留痕 | trades 异常隔离/修复测试 |
| 拆股估值错配 | corporate_actions 独立于原始成交；先核对，再重放，保持总成本和现金；拒绝不可表示的零碎股 | 拆股、合股、撤回与备份回归 |
| 丢失响应重复提交 | 财务回执与领域变化同事务，先识别回执后版本检查，客户端固定原请求 | 回滚注入、丢响应、双实例 HTTP 测试 |
| 更正不可追溯 | financial_journal 前后原文、精确现金差额、原因、时间、操作编号随事务提交 | 日志写入失败整次回滚测试 |
| 浮点精度漂移 | decimal 输入、cash_state、holding_amounts、源行情/FX、accounting、每日精确原值、备份；number 仅投影 | 大额分位、行情原始十进制、还原一致性测试 |
| 不可信数据生成行动 | 结构化 issues 定位持仓；缺汇率、待确认拆合股和隔离阻止依赖总值的计算；存款核对与价格时点不阻止计算；plan 返回阻止原因与排除持仓 | valuation、planning、structured_output 测试 |

每次更正由所属领域事务负责回执、源记录、投影及历史的一致性。正式验收运行 `npm run test:financial`：门禁创建独立临时集群，运行真实迁移、托管 HTTP 流程、多实例版本/租户隔离、`pg_dump`/`pg_restore` 并逐表比较，不读取部署 `.env` 或生产数据。PostgreSQL 工具从 `MARIO_TEST_PG_BIN` 或 `pg_config --bindir` 查找；`scripts/test.sh` 包含此门禁，缺少 PostgreSQL 工具时必须失败。

功能代码不能保证行情供应商无遗漏，也不能替代券商资产核对、部署环境备份及主密钥托管。当前不提供自动换汇、转托管、零碎股现金替代、交易执行或税务账簿。

## 9. 保留的迁移适配

以下代码只服务存量数据恢复，不构成第二条生产路径：

- 旧 JSON 备份结构（同步数据集 v1 起）、旧租户 schema 补列与一次性数据迁移（`db/schema.rs`、按 `POSTGRES_SCHEMA_VERSION` 只执行一次）。
- 旧收支缺少 exactAmounts 时，将已保存数值转换为事实卡十进制字符串；不推测此前已丢失精度，待存量收支均以精确值重存后可移除。
- 历史报告与引用的读取适配，保留历史原文；v7 的研究计划、候选、风险审查及模型证据核对字段仅供读取，v8 不再产生这些内容。

- 旧 SQLite 数据：运行时只有 PostgreSQL，唯一迁移路径是用 Git 标签 `legacy-sqlite-export` 中的 `--export-data` 导出同步数据集，再由当前版本导入（见 [数据迁移](data-migration.md#旧版-sqlite-数据)）；该标签不再接收修复。
- 长期记忆退役后的升级与恢复：仓储升级时删除旧偏好表、清理预设开关和分析中的记忆元数据，历史问答正文及冻结引用保留；旧备份只校验退役表形状并跳过，导入在同一事务内清理元数据并执行财务一致性校验。待停止支持 v1–v21 备份时移除。

## 10. 容量边界与已知架构债

### 离线 benchmark 研究工具

Python 打包由 `pyproject.toml` 限定为 `benchmark` 包及其配置、策略资源，支持可编辑安装，不扫描前端和 Rust 目录。部署时由 `scripts/benchmark-deploy.mjs` 拥有看板环境准备与当前快照同步；虚拟环境、uv 管理的 Python 与缓存分别位于 `.runtime/benchmark/.venv`、`python`、`uv-cache`，均由服务用户创建。部署不复用仓库 `.venv`，不继承调用者的 Python 环境选择；两次研究 CLI 调用通过 `MARIO_BENCHMARK_PYTHON` 使用同一个部署解释器，下载仍使用服务用户保存的 HF 凭据。

[`benchmark/`](../benchmark/) 是与生产模块隔离的 Python 标准库 CLI，由 `scripts/benchmark.mjs` 启动；`public_sources` 优先探测免费来源覆盖并归档响应，`sources` 提供可选授权行情和财报原件下载，`dataset` 负责契约、完整性和历史时点投影，`evaluate` 计算预测排序指标，`release` 创建不可覆盖且带哈希清单的本地发布包。CLI 依赖这些研究模块，研究模块不导入前后端源码、不访问 PostgreSQL、账号备份或生产凭据。边界测试限制其依赖为 Python 标准库和本包，例外只有两个单一适配器：`baostock_market` 可导入 uv 依赖组 `market` 中的 `baostock`，用于已退市证券的免费日线；`hub` 可导入依赖组 `publish` 中的 `huggingface_hub`，负责上传与按固定提交下载。在线服务不调用研究 CLI。

两个股票池只在候选来源上不同，按时点市值的口径由 `market_cap` 统一：选样日未复权收盘价（经 `baostock_market`）乘以当时已公告的总股本（东方财富），连同剔除原因与缓存布局。`tech_universe` 以 `xls` 解析的申万个股行业变更历史确定科技池候选；`dividend_universe` 以被动指数基金年报全部持仓为中证红利成分代理，再按中证官方调样公告修正生效时点。两者互不引用，只产出带出处的年度候选提案，不修改 `dataset` 的正式校验规则。`pool_release` 把一个或多个股票池的提案合并为季度决策切片，时点输入、选样快照与建仓后价格路径分文件写入不可覆盖目录；`scoring` 是无网络的纯计算模块，按用户确定的主指标（平均收益得分加 2.5 × 胜率，全部现金为 0，收益得分为建仓后 0–7、7–30、30–90 自然日总资产增长按 0.3/0.3/0.4 加权）给仓位文件打分，并以同一公式计算沪深300 全收益指数与股票池等权对照；仓位可附在决策日确定的卖出规则或网格，由 `scoring` 按结果路径中的逐日开盘、收盘与可卖出、可买入状态执行，策略本身不接触未来价格。策略以 `benchmark/strategies/<名字>/` 目录提交（说明 `strategy.md` 与只含 `decide(sample)` 的 `strategy.py`），`strategy_runner` 静态检查后在隔离子进程中只喂入时点输入生成仓位（显式启用 Python UTF-8 模式，父子进程以 UTF-8 通信，避免 Windows 本地编码影响中文行业输入与错误诊断），交给 `scoring` 打分，并以代码、发布包和打分引擎的哈希缓存结果；`dashboard` 只读取 `strategy_runner` 输出的成绩与说明，生成看板的页面、脚本与数据三个文件，`bench-run` 一条命令串起这一流程；`bench-serve` 只在本机提供这三个文件，看板重新生成后已打开的页面自动载入。`hub` 校验发布包后把输入与标签分别上传到两个 Hugging Face 数据集仓库，`hub-fetch` 按标签下载并校验还原。当前输入与标签仓库按用户要求公开；适配器也支持已有私有仓库，不修改可见性。公开分仓不是访问隔离，未来标签仍禁止进入策略决策或训练输入。

每日行情由 `scripts/benchmark_update_prices.py` 编排，GitHub Actions定时执行；`index_membership.py`保留中证A50/上证50各50只当前成分原件及获取/来源日期，不伪造历史成分。`daily_prices.py`合并年度历史成员与当前完整成分，通过既有公开HTTP适配器更新供应商OHLC与独立调整收盘价，最多2个线程；每证券一个完整历史文件，首次回补或显式全区间刷新，不拼接调整尺度；22:37更新独立报价，23:37显式整段刷新历史，按月累计报价及原始元数据。来源缺失、闭市观察与日期缺口结构化保存；当前指数覆盖不足或活跃证券来源不可用时不发布，未知日历不能推断休市。最新报价及其原始元数据与日线分开保存，保留供应商时间戳；当前价格覆盖和当日日线覆盖分开计数，不用报价补造OHLC。新来源不生成交易状态、财务指标或季度评分。

原Baostock双口径 `daily-prices/`只读保留，不再由每日工作流调用该供应商。`hub.historical_prices_reference`按固定旧标签核验配置、覆盖及上市退市元数据，供新 `daily-public-prices/` 的历史引用使用；不把旧价格拼入新序列。只有经证据确认的退市证券才允许以这类历史引用保留覆盖，活跃来源失败仍失败。此适配只服务尚未迁移的历史数据；完成所有相关证券及复权口径迁移后可显式移除，旧提交保留审计。

`hub.py`固定输入标签与远端提交，恢复新归档并校验全集哈希；在临时分支启用LFS、完整分批上传和核验内容标识，再以非强制Git快进原子更新主分支，远端前进则拒绝覆盖。`rebuild`仅显式完整重采新来源，保留旧范围与历史成分，不回退日期。HF是持久状态来源，本机缓存仅支持首次恢复；状态不进入在线账号事务。供应商未对账数据标记 `release_ready=false`，不改变正式评分、策略或部署看板。见[每日行情自动更新](benchmark.md#每日行情自动更新)。

研究看板由 Git 主分支持续维护：`.github/workflows/benchmark-live.yml` 在可信 main 的策略、引擎或依赖变化后执行 `benchmark.live evaluate`，复用 `strategy_runner` 和 `scoring`；`hub` 原子发布完整 JSON 快照到现有标签仓库的 `dashboard/current.json`。运行进度附着于上次成功成绩，全部成功才替换成绩；Git 提交、评分切片和 HF 历史提供可追溯性。日线与持续评测共享HF标签仓库主分支，并发归 Actions 的同一 `benchmark-hf-main` 任务组，失败不替换成功成绩；不引入在线数据库、业务事务或消息队列。

`benchmark-deploy` 仅准备服务用户的 Python 环境并同步当前快照；`host-runtime` 管理 `benchmark.live sync --watch` 的启动和退出，每 30 秒检查远端固定提交，只读同步至 `.runtime/benchmark/current/`。远端只提供数据，页面与脚本仍由本地部署版本生成；失败保留上次文件，进程内缓存避免重复下载。API 的 `MARIO_BENCH_DIR` 指向固定目录，`/dashboard` 与 `/dashboard.js` 公开，`/api/benchmark/dashboard` 只验证会话令牌，不打开账号 schema、不加账号锁。依赖方向不变：在线 API 只读派生文件，不执行策略；研究模块不访问账号数据。页面每 10 秒携带 Mario 会话轮询，状态更新保留视图，成绩更新后按原网址状态重载。会话存储格式变更时同步修改 `dashboard.py`。旧标签目录只供研究复现，生产不再在部署时评分；部署配置使用 `MARIO_BENCH_ENABLED=true` 启用同步，旧标签配置不再读取，评分切片由 Actions 的 `BENCHMARK_SCORING_TAG` 统一确定。

策略目录按选股、仓位构建与执行机制区分；同一机制只保留当前切片总分最高的一组参数，参数更新在原目录完成，不以参数变体重复占榜。扫描记录可保留为历史审计，当前看板由去重后的目录重新生成。

富贵策略是用户明确指定保留的策略，唯一实现位于`strategies/fugui/`，即使不在Top-10也参与默认发现、评分和看板；名称标注“技术底仓试验”，不将尚未实现的基本面和机动仓算作已验证能力。

每个决策点的操作比重由策略仓位与执行规则确定：`weight` 是占期初总资产的预算，网格首买/加仓按资金份数，条件卖出的 `sell_fraction` 按该股当时持股数量。`scoring` 拥有部分卖出后余股/现金估值和逐笔 `operations`，`strategy_runner` 规范化默认全卖比例，`dashboard` 只读取记录并展示每期预算、首买、加减仓与实际成交比重，不自行模拟交易或从未来路径推断。旧成绩通过引擎指纹重新评分，不把缺少操作记录解释成没有成交；详见 [操作比重与逐笔记录](benchmark.md#操作比重与逐笔记录)。

独立研究脚本 `scripts/benchmark_long_history_study.py` 只消费已缓存公开行情与既有切片，生成截至决策日的长窗口特征和临时候选，再复用隔离执行器和评分器。扩充输入写到新的研究目录，保留原字段与结果标签；研究目录不自动替换线上切片，也不向正式策略目录安装候选。它不引用前后端模块或账号存储，来源与实验记录见研究清单和审计文档。

`scripts/benchmark_risk_study.py` 在同一边界内构造三年周波动和月内历史日收益，复用构建/扫描函数的显式特征与规则参数；组合风险在候选策略中计算。最小输入升级只携带准入策略需要的字段，`hub` 数据卡按明确的 `input_features` 版本说明截止时点和缺失语义；原始日线不随最小升级包上传。

本地main使用的研究切片`pool-scoring-v9`保留月内日收益及90日回归趋势，将`stock.fugui`替换为日/周/月下轨、月中轨回踩和月MACD负柱五个布尔信号及问题码，契约`monthly-risk-regression-fugui-v2`。富贵两类买点共用日/周下轨与月MACD柱<0；月下吸用月下轨，龙回头用月中轨回踩；收盘容差5%，DIF/DEA不要求负值。`trend_basket_risk`和`trend_regression_tilt`输入保持不变，原21项策略和结果标签不变。本次按用户选择仅本地保留，代码统一合入本地main、不推送；已发布切片与持续评测默认仍为v8，使用对应历史提交复现；需要新字段的策略不在缺失时静默换逻辑。

`scripts/benchmark_regression_study.py`复用长历史构建/扫描及月内收益计算，比较严格90日与停牌估值口径；缺行不前填，窗口不延长。`--minimal`只输出准入机制必需的信号和原v6字段，发布仍经`hub`显式执行。它只依赖标准库与离线研究模块，不进入在线状态或账号事务。

`scripts/benchmark_earnings_study.py` 消费巨潮公告和中证市场行情的独立缓存，构造截至决策日已结束的报告公告反应窗口；公告解析仍复用 `public_sources`，不解析未核验EPS或读取未来标签生成特征。组织标识和证券代码分开处理，旧代码公告不会自动归入新代码。该扩充只用于独立研究输入，不改变正式切片数据契约。

`scripts/benchmark_sue_study.py` 读取人工核验财务台账与原件哈希，按决策时已知的利润版本和全期间恒定普通股证据构造单季EPS及SUE。财务语义审阅与档案身份校验分开；缺少股数证明、不可比口径或股份尺度变化时返回问题码。它不自动解析PDF、不构造正式切片、不调用来源SDK或在线业务；来源、台账和计算输出仍归本地研究档案。

输入、请求缓存与发布包归 `.runtime/benchmark/`，是公开研究语料文件，不是第二个在线业务存储引擎。行情 token 只从终端环境读取。Hugging Face 是可选的后续分发渠道，本地构建不发布数据。完整原始归档与按披露时间截断的模型 inputs、未来 labels 分开；未核对历史成分、缺少日线或财报时不生成完成发布包。详细契约、失败恢复和研究限制见 [benchmark](benchmark.md)。

`scripts/benchmark_fugui_study.py`在同一离线边界消费后复权/日历缓存，按截止时点构造BOLL、MACD与月中轨回踩特征；`--minimal --built-on <日期>`仅从正式v8替换富贵信号并生成不可覆盖的v9。策略只存在于正式目录，旧长期腰斩/首次回踩买点已删除；旧数据和成绩只作历史审计。保留其他字段和结果标签，不推定缺失基本面通过；执行仍归隔离执行器与评分器，不新增在线状态或账号事务。[富贵策略方案](fugui-strategy.md)区分完整设计、技术底仓与未实现的滚动/基本面条件。

`collect` 编排免费批量补采：固定显式证券清单和请求快照，保存逐证券行情/财报目录、逐 PDF 哈希与可恢复进度；仅依赖公开来源适配器和文件契约，不调用在线服务。交易日历闭市日不算价格缺口，少量未补齐交易日只记覆盖状态、不阻塞其他资料下载。Yahoo 与腾讯价格口径分开归档，当前成分采集清单与历史年度前 20 名选样分开，未核对的财报与历史成分不会被采集流程升级为评测就绪。

当前实现面向小规模自建：PostgreSQL 连接按请求创建，业务并发有固定上限，快照为全量读取，同步为轮询，分析没有持久作业或跨实例取消。扩大规模时的接入点分别是连接池、分页/增量资源接口和任务队列；在需求出现前不预先引入。

已知架构债（架构检查通过不代表它们已消失）：

- **行情等待期间持有账号锁**：价格与汇率刷新在请求的账号锁内等待外部供应商，慢供应商会阻塞同账号其他请求。改造方向是锁内读取输入、锁外查询行情、重新加锁核对输入未变后再由仓储事务写入；连接池与异步 SQL 需另行设计。
- **模型管理直接依赖具体 AI 实现**：`application/model_settings.rs` 直接构造 `NativeModelProvider` 与 `ai::bridge::BridgeProvider`，`application/codex_bridge.rs` 直接操作 `ai::bridge` 的注册状态。改造方向是为模型检测、目录读取与桥接注册定义窄接口，启动时注入。
- **前端同步依赖单实例全局状态**：`lib/service.ts` 保存全局 revision，`lib/syncEvents.ts` 保存同步与写入计数，切换账号依靠重建子树保护。改造方向是由账号边界创建拥有 revision、同步屏障和生命周期的会话实例。
- **跨语言契约手动维护**：前端 `types.ts` 与 Rust `models` 各自维护，写入政策前后端各有一份；架构检查不理解宏展开与间接转发，也不证明运行时事务与租户隔离。

## 11. 修改架构时

1. 先写简明设计：目标、涉及模块与职责、接口与数据流、依赖方向、状态和事务归属、验证方式；非兼容更新说明替换关系与数据迁移。
2. 确定数据与事务的所属领域，再放置契约、仓储操作与用例；需要外部能力时增加或复用 `ports/` 接口并在适配器装配。
3. 更新本文受影响的章节与 [模块边界](module-boundaries.md)，必要时调整 `scripts/check-architecture.mjs` 及其测试。
4. 删除被替代的旧实现，运行 `npm run check:architecture` 与范围匹配的测试（入口见 [模块边界](module-boundaries.md#开发流程)）。

## 相关文档

- 产品与方法：[产品目标与长期原则](product-vision.md)、[投资方法论](methodology.md)、[确定性风险与再平衡模型](planning-model.md)
- 安全与数据：[自建服务信任边界](threat-model.md)、[账号与数据同步](cloud-sync.md)、[AI 数据边界](ai-data-boundary.md)、[可追溯市场数据](market-data.md)
- 领域行为：[真实成交与持仓](holding-trades.md)、[每日资产](daily-assets.md)、[研究证据](research-evidence.md)、[复盘与规则](review-and-rules.md)
- AI：[AI 分析工作流契约](ai-workflow.md)、[问答流式回复](ai-streaming.md)、[可审计的 AI 分析档案](analysis-history.md)、[Codex 桥接](codex-bridge.md)
- 运维：[自建宿主机服务](self-hosting.md)、[宿主机部署](deploy-madeagents.md)、[数据迁移](data-migration.md)、[Web 构建](client-builds.md)
- 离线研究：[A 股研究 benchmark](benchmark.md)、[benchmark 后续计划](benchmark-plan.md)
