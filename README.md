# mario

mario 是一个支持自建服务器、多用户账号和跨设备访问的 AI 投资决策辅助软件。它不以行情、K 线或荐股作为产品中心，而是通过财务底座、目标配置、风险检查、决策日志和复盘，帮助用户形成可验证的投资方法。

> 当前版本用于投资教育与决策支持，不承诺提高收益，也不替代持牌专业人士针对个人情况提供的建议。

## 自建部署

当前采用 **Web / Tauri 客户端 → Rust API → PostgreSQL**，AI Agent 独立部署。账号、会话与数据由自己管理，无第三方账户或云存储依赖。

两种启动方式统一读取根目录 `.env`，所有配置项和中文说明见 [`.env.example`](.env.example)。可先运行 `node scripts/generate-env.mjs` 生成带注释和随机密钥的配置，再修改域名；原生部署还可在文件中设置 PostgreSQL 路径、端口和 HTTPS 开关。

需要频繁搬迁时，运行 `npm run data:download -- --host user@server --remote-dir /srv/mario --mode docker`，可从本机一键在服务器打包并下载加密迁移包。目标恢复、反复覆盖和回退见 [数据迁移指南](docs/data-migration.md)。

```bash
# Docker：生成配置、构建并后台启动全部服务
bash scripts/start-docker.sh --domain mario.example.com

# 无 Docker：自动初始化本地 PostgreSQL，构建并前台启动
bash scripts/start-native.sh
# 已安装 Caddy 且配置好域名时，可加 --domain mario.example.com 启用 HTTPS
```

域名指向服务器并开放 80/443 后，网页使用 HTTPS 访问，客户端填写同一服务器地址登录。管理员提供邀请码创建独立账号。部署参数、旧数据迁移、备份与恢复详见 [自建部署指南](docs/self-hosting.md)。

**当前是在线服务模式**：多设备直接读写同一数据库，版本校验防止旧数据覆盖；离线不能保存。服务器可读取业务明文，供应商密钥按账号加密保存。

## 问答优先的使用方式

打开应用即可提问，也可以选择一个示例问题开始。发送前只需确认模型与资料范围，具体数据和记忆授权按需展开。回答优先展示结论和一项待补充信息，依据、行动建议与分析过程折叠保留。

在当前页面继续追问时，会附上上一轮问题和回答摘要（最多 6,000 字符）；选择“换个话题”可不携带这段上下文。页面明确显示是否参考上一轮，不把模型回答当作已核实事实。切换资料页面保留当前问答与输入，刷新后可从“以前的问答”重开已保存分析。

日常只保留“问答”和“我的情况”两个入口。“我的情况”只需填写月收入、月支出、存款和持仓；负债、备用金等按需展开。不要求新人填写风险倾向、目标权重、收益预测或维护研究资料。存款保存为现金类持仓，计入资产一次；备用金是存款中预留的部分，不再次计入总资产。

mario 在问答时使用供应商原生网页搜索，自动保存回答和来源，并从最近 30 天、最多 50 次分析中检索相关搜索资料（去重后最多 200 条候选，再按问题选取 12 条）。历史摘要保留其摘要属性和原始采集时间，不提升为已核验事实，也不把历史模型结论当成资料来源；时效信息需重新联网核对。关闭“研究证据”授权后，自动资料同样不会发送。

原有资料、目标、决策和复盘没有删除，可在“模型与隐私 → 查看已有的资料与记录”访问。需要补充个人事实时在问答中逐项追问；模型不会擅自改写资产、收支、目标、规则或执行交易。

## 当前能力

- macOS / Windows 桌面客户端：Tauri + React + TypeScript
- Web：HTTPS 同源 API、独立用户账号，详见 [自建部署](docs/self-hosting.md)
- Rust + Axum 后端：自建账号、会话、租户隔离与业务 API；Tauri 仅提供客户端外壳
- PostgreSQL：财务档案、目标、持仓、决策与分析历史；SQLite 仅用于旧数据迁移及本地开发
- 每日资产自动追踪：列表内改金额即保存，同日合并、跨日沿用，查看总资产与净资产趋势、逐项变化和区间分析；详细规则见 [每日资产追踪](docs/daily-assets.md)
- 服务器按账号加密保存模型与行情 API Key，不进入数据导出和分析上下文
- 确定性风险规则：应急资金、负债压力、集中度、期限错配
- 确定性规划：目标路径模拟、月度投入缺口、风险预算与再平衡偏差
- OpenAI Responses、Anthropic Messages 独立 Agent 适配器；Codex 本机登录仅限本地开发
- AI 深度工作流：研究计划 → 两轮结构化长期记忆检索 → 两个独立候选方案 → 独立反思 → 最终裁决
- 结构化输出校验：事实、推断、未知、方案、行动与复盘条件必须通过机器契约，非法引用自动拦截并允许一次修复
- 可解释长期记忆：区分已复盘决策、未验证判断与历史 AI 回答，支持时间衰减和反证信号
- 用户可管理的长期记忆：把相关经验标为长期保留、写适用范围注释或永久屏蔽；原始决策与复盘不被改写
- 逐条记忆授权：发送前可单独排除候选记录，选择变化后必须重新生成一致性指纹
- AI 工作流轨迹：逐阶段保存研究产物、模型耗时与 Provider 返回的 token 用量
- 可审计分析档案：重开历史结构化报告和证据，从决策追溯原分析，或用当前数据重新分析旧问题
- AI 快速工作流：用于低成本的单轮结构化分析
- AI 数据预览：逐组选择上下文、冻结候选记忆、确认指纹并保存本地审计
- 分析到决策闭环：用户选择一条 AI 行动后生成可编辑草稿，高风险数字必须人工填写，冻结时保留原分析来源
- 决策复盘：原始判断不可变，复盘单独记录结果、过程评分与经验修正
- 周期系统复盘：冻结当时组合、目标、风险、决策完成度与纪律执行记录
- 可选桌面复盘提醒：启动时检查到期决策与周期复盘，相同状态每天至多通知一次，不在锁屏暴露资产名称
- 个人投资规则：从复盘沉淀触发条件与行动，任何修订都保留历史版本
- 决策前规则检查与有效性追踪：冻结当时规则版本、遵守或偏离状态，用复盘后的过程评分寻找需要保留或修订的规则
- 不可变组合流水：按日期记录入金、出金、分红、利息、费用、税费与买卖额，冻结原币、汇率和基准币种
- 安全 CSV 导入：写入前逐行预览，使用来源交易编号幂等去重；编号冲突或错误行会阻止整批写入
- 可审计流水冲正：当前冻结周期内追加等额负向事件，保留原记录和修正原因，阻止重复冲正并随加密快照同步
- 组合变化归因：周期性冻结持仓，自动汇总区间流水，把组合总变化、外部现金流与估值/数据残差分开观察，并提供明确标注的 Modified Dietz 近似回报
- 估值口径护栏：支持基准币种、外币折算汇率和持仓估值日；缺失汇率或日期错位时停止伪精确汇总与归因
- 可追溯参考汇率：按估值日查询 ECB 日度序列，周末使用最近共同工作日并明确标注；来源与观察日期随记录冻结
- 可审计证券估值：用户主动调用 Twelve Data 日线，以估值日数量 × 未复权收盘价推导市值，并冻结币种、交易所、MIC、来源和实际观察日；人工改值会撤销核验标记
- 研究证据账本：记录来源层级、HTTPS 链接、资料日期、支持/反驳关系与限制
- 证据检索与引用：按问题和组合筛选，发送前冻结，要求 AI 只引用实际进入载荷的来源
- 简化概率校准：使用历史置信度与逻辑结果训练概率意识，不用单笔盈亏评价能力
- 多账号与版本同步：前台低频检查版本、变化后刷新；跨 API 实例的写入冲突保护

## 仓库结构

```text
client/                 React 响应式界面与 Tauri 桌面/Android 外壳
  src/                  页面、类型和本地 API 客户端
  src-tauri/            轻量原生外壳，连接远程 API
server/                 多用户 HTTP API 与独立 AI Agent 运行入口
  src/ai/               可替换工作流、执行器与模型 Provider
  src/db.rs             领域数据仓储；storage.rs 对接 PostgreSQL / 旧 SQLite
  src/accounts.rs       自建账号与会话
  src/hosted.rs         租户认证、版本校验与数据迁移
  src/agent.rs          无状态 Agent HTTP 服务
  src/memory.rs         可替换、可解释的结构化记忆检索接口
  src/evidence.rs       可替换的研究证据检索接口
  src/valuation.rs      基准币种折算、估值完整性与资产类别变化
  src/market_data.rs    可替换市场数据 Provider、ECB 汇率与 Twelve Data 证券价格适配器
  src/performance.rs    流水分类汇总与现金流调整后期间回报
  src/event_import.rs   CSV 解析、字段映射与不可信输入边界
  src/risk.rs           不依赖大模型的风险规则
docs/                   架构与投资方法论
deploy/                 HTTPS 入口配置
compose.yaml            Web / API / PostgreSQL / Agent 四服务部署
scripts/                构建、迁移与集成测试
```

产品为什么存在、长期不应偏离什么，见 [产品目标与长期原则](docs/product-vision.md)。详细设计见 [架构说明](docs/architecture.md)、[自建服务信任边界](docs/threat-model.md)、[Android 调试构建](docs/android.md)、[AI 工作流契约](docs/ai-workflow.md)、[可审计的 AI 分析档案](docs/analysis-history.md)、[从 AI 分析到用户决策](docs/analysis-to-decision.md)、[长期记忆与多轮检索](docs/long-term-memory.md)、[投资方法论](docs/methodology.md)、[研究证据与引用](docs/research-evidence.md)、[复盘与规则闭环](docs/review-and-rules.md)、[组合变化归因](docs/portfolio-attribution.md)、[组合流水 CSV 导入](docs/portfolio-event-import.md)、[可追溯汇率数据](docs/market-data.md)、[确定性规划模型](docs/planning-model.md)、[AI 数据边界](docs/ai-data-boundary.md) 与 [账号与数据同步](docs/cloud-sync.md)。

## 开发与验证

生产部署和客户端构建见 [自建部署指南](docs/self-hosting.md)。只调试原有领域功能时仍可使用隔离的本地 SQLite 开发服务：

```bash
npm run install:all
npm run dev
```

多用户开发请配置 PostgreSQL、`MARIO_MASTER_KEY`、`MARIO_REGISTRATION_KEY`；前端设置 `VITE_API_URL=http://127.0.0.1:4217/api`。独立 Agent 使用 `MARIO_ROLE=agent`，后端配置 `MARIO_AGENT_URL` 与共享内部令牌。Compose 默认采用独立 Agent，单进程开发可以不设置 Agent URL。

```bash
npm run build --prefix client
npm test --prefix client
cargo test --manifest-path server/Cargo.toml
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
cargo build --manifest-path server/Cargo.toml
# 设置 MARIO_TEST_DATABASE_URL 为临时 PostgreSQL 后运行：
node scripts/test-hosted.mjs
```

## 模型与数据

登录后在“模型与隐私”配置自己的模型、接口地址和 API Key。服务器只允许管理员配置的 HTTPS 模型主机。AI 分析前展示资料范围并冻结上下文，独立 Agent 完成工作流，API 保存可审计结果。Agent 不读取数据库，也不修改资产或执行交易。

旧 SQLite 数据可通过服务端 `--export-data` 命令导出，然后在新账号的“账号与数据”导入；密钥和旧账号会话不会导入。迁移步骤、隐私边界、PostgreSQL 备份和密码重置详见 [自建部署指南](docs/self-hosting.md)。
