# mario

mario 是一个支持自建服务器、多用户账号和跨设备访问的 AI 投资决策辅助软件。它不以行情、K 线或荐股作为产品中心，而是通过财务底座、持仓管理、风险检查和复盘，帮助用户形成可验证的投资方法。

> 当前版本用于投资教育与决策支持，不承诺提高收益，也不替代持牌专业人士针对个人情况提供的建议。

## 自建部署

当前采用 **React Web → Rust API → PostgreSQL**，AI Agent 独立部署。账号、会话与数据由自己管理，无第三方账户或云存储依赖。

服务器部署只有一个入口：根目录 [deploy.sh](deploy.sh)，在服务器手动执行。仓库不配置 GitHub Actions，推送或创建 PR 不会触发自动构建、测试或部署。公网 IP、域名和可选参数集中在脚本顶部，逐项附中文注释。默认 Ubuntu/Debian 自动安装缺少的工具、生成持久密钥、构建源码，并通过 systemd 后台运行和开机自启，不需要 Docker。

```bash
sudo bash deploy.sh --ip 47.243.99.21 --domain www.madeagents.ai
# 以后更新源码后重新部署：sudo bash deploy.sh
# 管理：bash deploy.sh start|stop|restart|status|logs|check
```

默认网页入口是 https://www.madeagents.ai/mario。首次还需将自动生成的 Nginx 片段加入已有 HTTPS 站点，保留原网站和证书，具体步骤见 [部署与验收](docs/deploy-madeagents.md)。

频繁搬迁可执行 `npm run data:download -- --host user@server --remote-dir /srv/mario`，一键打包下载加密迁移包。目标恢复、反复覆盖和回退见 [数据迁移指南](docs/data-migration.md)。

**当前是在线服务模式**：多设备直接读写同一数据库，版本校验防止旧数据覆盖；离线不能保存。服务器可读取业务明文，供应商密钥按账号加密保存。

## 资产工作台与问答

默认进入“我的情况 → 投资持仓”。问答页在宽屏左侧展示历史会话，右侧为当前对话与底部输入区；窄屏通过历史按钮切换。输入问题后直接发送，正文边生成边显示，可点击“停止生成”中止并保留草稿；完成后展示经过校验的完整 report 与实际引用的信息编号，详见 [问答流式回复](docs/ai-streaming.md)。点击编号可展开本次冻结的原始资料、个人原值、历史模型回答，并查看来源网页或下载原始附件；分类、相关性评分、资料确认、工作流轨迹和审计信息不出现在问答页。

在“设置 → 问答预设”一次配置允许输入的财务档案、持仓明细、研究证据与历史搜索资料，以及网页搜索。问答固定使用深度分析，旧预设中的快速分析与已移除资料项会自动调整。预设按账号保存在服务器，之后每次提问自动使用最新资料；后台仍会冻结资料指纹并检查一致性，读取预设失败时不会发送。

在当前页面继续追问时，会附上上一轮问题和回答摘要（最多 6,000 字符）；选择“新对话”可清除本轮上下文。模型回答不作为已核实事实。切换资料页面保留当前问答与输入，刷新后可从“历史对话”重开已保存 report 并继续追问。

日常只保留“问答”和“我的情况”两个入口。“我的情况”只需填写月收入、月支出、存款和持仓；负债、备用金等按需展开。不要求新人填写风险倾向、目标权重、收益预测或维护研究资料。存款保存为现金类持仓，计入资产一次；备用金是存款中预留的部分，不再次计入总资产。

mario 在问答时使用供应商原生网页搜索，自动保存回答和来源，并从最近 30 天、最多 50 次分析中检索相关搜索资料（去重后最多 200 条历史搜索资料，与手工资料按问题召回最多 30 条；再与本次网页来源一起交给模型，判断主题、事件日期、时效、相关性与冲突，合计筛选最多 10 条用于回答）。历史摘要保留其摘要属性和原始采集时间，不提升为已核验事实，也不把历史模型结论当成资料来源；时效信息需重新联网核对。关闭“研究证据”授权后，自动资料同样不会发送。

参考资料和复盘可通过对应导航访问。需要补充个人事实时在问答中继续追问；模型不会擅自改写资产、收支、规则或执行交易。

## 当前能力

- 离线研究：[中国红利 top-20 benchmark](docs/benchmark.md)，按历史年末市值选样，构建日线/季度财报及宽指 ETF 的时点输入、预测标签和本地 Hugging Face 发布包；v2 新增按申万时点行业选样的科技股池与按时点重建成分的中证红利股池，合并为按 7/30/90 日加权总资产增长打分的季度决策切片，上传到 Hugging Face 私有仓库（[行动指南](docs/benchmark-v2-plan.md)）；真实数据采集与完整性验收独立执行

- Web：HTTPS 同源 API、独立用户账号，详见 [自建部署](docs/self-hosting.md)
- Rust + Axum 后端：自建账号、会话、租户隔离与业务 API
- PostgreSQL：账号、财务档案、持仓、复盘与分析历史的唯一数据存储，开发与测试同样使用
- 每日资产自动追踪：在资产抽屉中编辑并明确保存，同日合并、跨日沿用，查看总资产与净资产趋势、逐项变化和区间分析；详细规则见 [每日资产追踪](docs/daily-assets.md)
- 服务器按账号加密保存模型与行情 API Key，不进入数据导出和分析上下文
- 确定性风险规则：应急资金、负债压力、集中度、期限错配
- 确定性计算：风险预算与再平衡偏差
- 用户自定义第三方模型 API 地址、端口、模型名称和密钥；支持 OpenAI 兼容 Chat Completions、OpenAI Responses、Anthropic Messages；也可通过个人电脑上的 [Codex 桥接](docs/codex-bridge.md) 使用本机 Codex
- AI 深度工作流：资料筛选 → 研究计划 → 两个独立候选方案 → 独立反思 → 最终裁决
- 结构化输出校验：事实、推断、未知、方案、行动与复盘条件必须通过机器契约，非法引用自动拦截并允许一次修复
- AI 工作流轨迹：逐阶段保存研究产物、模型耗时与 Provider 返回的 token 用量
- 可审计分析档案：重开历史结构化报告和证据，用当前数据重新分析旧问题
- AI 快速工作流：用于低成本的单轮结构化分析
- AI 资料预设：在设置中逐组选择上下文，提问时自动冻结资料、复核指纹并保存审计
- 复盘记录：只需自由文字，保留完整原文与换行，并保存当时组合与风险摘要
- 页面内复盘提示：查看周期复盘事项
- 个人投资规则：从复盘沉淀触发条件与行动，任何修订都保留历史版本
- 估值口径护栏：支持基准币种、外币折算汇率和持仓估值日；缺失汇率或日期错位时提示估值口径缺口
- 统一汇率：每天自动更新 ECB 参考值，通过右侧独立浮窗勾选展示币种、手动修改与恢复自动；已有每日资产记录保留当时的来源与观察日期
- 自动证券价格：无需密钥的 Yahoo Finance 免费日线，服务进程全局共享缓存、并发合并与失败退避；根据证券代码和持仓数量自动更新价格、市值并保存来源及实际观察日，另可主动调用个人 Twelve Data 日线
- 纯文本参考资料：直接保存获取到的重要原始信息，保留全文与换行，无需逐项填写来源分类、链接或日期；支持附带文件及下载，提取文本、PDF 和 DOCX 文字用于问答
- 证据检索与引用：按问题和组合筛选，发送前冻结，要求 AI 只引用实际进入载荷的来源
- 简化概率校准：使用历史置信度与逻辑结果训练概率意识，不用单笔盈亏评价能力
- 多账号与版本同步：前台低频检查版本、变化后刷新；跨 API 实例的写入冲突保护
- 浏览器下载备份和附件、打开来源网页；Web 产物记录源码指纹与校验值，见 [Web 构建](docs/client-builds.md)

资产录入统一在“我的情况”：收支、存款与现金、投资持仓分别管理；风险偏好与基准币种在收支的可选详情中。投资规则在“复盘记录”中维护。

界面采用浅色紧凑工作台、独立概览卡和连续列表。投资概览按基础币种汇总全部非现金持仓，不随搜索与筛选变化；成本、浮盈和已结算盈亏使用同一持仓估值汇率，缺失数据显示已知部分及缺失数量，完全未知显示“—”。原币金额、汇率及日期可展开查看，折算不代表历史汇兑损益。持仓列表展示资产、数量、现价／成本价、市值、盈亏和明确操作；名称打开详情，编辑与买卖分红进入右侧抽屉，删除位于更多操作并需确认。默认按基础币种市值降序，缺汇率资产置后，已清仓置末并继续贡献已结算盈亏。每日资产记录另设区域并明确包含现金，趋势默认收起。参考资料及复盘以列表为主，详情与编辑进入抽屉；抽屉支持键盘焦点约束、Escape、草稿放弃确认及失败重试。小屏使用带字段标签的纵向记录和全屏抽屉。汇率独立悬浮，刷新默认收起，业务抽屉打开时隐藏。

## 仓库结构

```text
client/                 React 响应式 Web 界面
  src/features/         按领域隔离的功能页面
  src/api/              按领域隔离的 HTTP 客户端
  src/app/              应用组合、导航与数据生命周期
  src/lib/httpClient.ts 认证、请求政策与版本保护
server/                 多用户 HTTP API 与独立 AI Agent 运行入口
  src/runtime.rs        启动配置与维护命令
  src/http/             按功能注册的 HTTP 适配器
  src/application/      组合、行情刷新、分析与模型配置用例
  src/ports/            可替换的行情查询与分析执行契约
  src/composition.rs    供应商装配、共享实例与后台任务生命周期
  src/models/           每日资产、行情与分析轨迹共享契约
  src/ai/               可替换工作流、执行器与模型 Provider
  src/db.rs             领域数据仓储；storage.rs 是唯一的 PostgreSQL 驱动适配
  src/accounts.rs       自建账号与会话
  src/hosted.rs         租户认证、版本校验与数据迁移
  src/agent.rs          无状态 Agent HTTP 服务
  src/evidence.rs       本地研究证据检索
  src/valuation.rs      基准币种折算、估值完整性与资产类别变化
  src/market_data.rs    可替换市场数据 Provider、ECB 汇率与 Twelve Data 证券价格适配器
  src/prices.rs         免费证券日线、实例缓存及公开行情预热
  src/risk.rs           不依赖大模型的风险规则
docs/                   架构与投资方法论
deploy.sh               宿主机部署与服务管理唯一入口
scripts/                构建、迁移与集成测试
```

产品为什么存在、长期不应偏离什么，见 [产品目标与长期原则](docs/product-vision.md)。详细设计见 [架构说明](docs/architecture.md)、[自建服务信任边界](docs/threat-model.md)、[AI 工作流契约](docs/ai-workflow.md)、[可审计的 AI 分析档案](docs/analysis-history.md)、[投资方法论](docs/methodology.md)、[研究证据与引用](docs/research-evidence.md)、[复盘与规则闭环](docs/review-and-rules.md)、[可追溯汇率数据](docs/market-data.md)、[确定性规划模型](docs/planning-model.md)、[AI 数据边界](docs/ai-data-boundary.md) 与 [账号与数据同步](docs/cloud-sync.md)。

开发时的模块职责、依赖方向、状态生命周期和扩展约束见 [模块边界与开发约束](docs/module-boundaries.md)。`npm run check:architecture` 检查反向依赖和依赖循环，常规测试入口会执行。

## 开发与验证

生产部署和 Web 构建见 [自建部署指南](docs/self-hosting.md)。开发与测试需要本机安装 PostgreSQL 工具（`initdb`、`pg_ctl` 等，脚本从 `MARIO_TEST_PG_BIN` 或 `pg_config --bindir` 查找），没有 SQLite 或免登录模式：

```bash
npm run install:all
npm run dev
```

`npm run dev` 在 `.runtime/dev` 启动开发用 PostgreSQL 集群，并生成该目录专用的数据库密码与 `MARIO_MASTER_KEY`，再启动 API 与 Vite；在浏览器用任意邮箱注册开发账号，数据在重启后保留，删除 `.runtime/dev` 即重置。单进程开发不需要独立 Agent；需要时以 `MARIO_ROLE=agent` 启动，后端配置 `MARIO_AGENT_URL` 与共享内部令牌。

```bash
npm run check:architecture
npm run build --prefix client
npm test --prefix client
npm run test:server        # 在临时 PostgreSQL 集群中运行 cargo test
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
npm test                   # 完整门禁，含 HTTP 与财务集成验收
```

## 模型与数据

登录后在“设置”配置自己的模型、接口地址、API Key 和问答预设。AI 分析前自动按预设冻结上下文，独立 Agent 完成工作流，API 保存可审计结果。Agent 不读取数据库，也不修改资产或执行交易。

旧单机版的 SQLite 数据需用 Git 标签 `legacy-sqlite-export` 导出，再在新账号的“账号与数据”导入；密钥和旧账号会话不会导入，步骤见 [数据迁移](docs/data-migration.md#旧版-sqlite-数据)。PostgreSQL 备份和密码重置详见 [自建部署指南](docs/self-hosting.md)。

已有数据的账号可在“账号与数据”勾选“覆盖当前账号已有数据”，选择导出的 JSON 并确认。覆盖前自动下载原数据备份，随后替换当前账号的业务数据；不合并交易，也不影响其他账号。备份回退及接口说明见 [账号与数据同步](docs/cloud-sync.md)。
