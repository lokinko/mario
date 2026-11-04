# mario

mario 是给自己使用的投资辅助工具。当前优先做成一件事：**持续跟踪自己认可的公司，发现值得研究的买点，并结合已录入的持仓、现金和真实交易判断是否适合自己。**

此前的投资信息录入、AI 问答和 benchmark 都服务于这条使用主线。现阶段先把观察助手用起来，按实际遇到的问题改进，不以增加功能、模型流程或策略数量作为进展。

## 从买点观察开始

1. 在“买点观察”加入认可的沪深 A 股，已有持仓和未持有的公司都可以加入；关注理由可选填。
2. 服务端在收盘后检查：已实施现金分红的 **TTM 税前股息率 ≥5%**，且**日线与周线同时处于各自 BOLL(20,2) 下轨价格的上下 2%**。接近下轨按下轨价格计算，不按带宽计算；跌破下轨超过 2% 不满足。
3. 查看站内变化提醒、数据日期及来源；对值得关注的公司，点击“结合我的交易讨论”。问答按已保存授权使用观察资料、持仓、现金及最近真实交易，核查基本面和分红持续性。
4. 周末查看本周已保存的观察，讨论下周等待什么；需要时在“复盘记录”保存自己的判断。

“高股息让我愿意长期持有、公司需求与经营相对稳定、价格进入合意区间”是选公司的出发点。技术条件只用于提示进一步研究，不能单独证明低估、分红可持续或应该买入。近期上涨也不能证明规则已经有效。

首版只做认可公司的买点观察。陌生公司发现、盘中盯盘、站外通知、卖出与滚动仓管理、自动下单不在当前范围；不预设买入数量或新增仓位限制。完整口径、来源、失败处理和接口见[买点观察助手](docs/buy-watch.md)。

## 已有能力如何服务这件事

| 能力 | 当前用途 |
| --- | --- |
| 我的情况、持仓与交易 | 保存真实资产、现金、买卖、分红及费用，提供个人决策背景；不用虚拟成交代替实际记录 |
| AI 问答与参考资料 | 对具体公司核查证据、反方观点与未知项；按授权冻结资料，保留可追溯引用，不自动修改账本或执行交易 |
| 复盘记录 | 用自由文字记录为何等待、为何行动，以及后来判断有何变化；不要求先建立完整投资研究表单 |
| 每日资产、风险与再平衡 | 保留已有账本和计算能力，按需查看；不是使用观察助手前必须完成的流程 |
| benchmark | 独立研究工具，用于检验明确的策略疑问；成绩不直接转成个人买入建议，也不作为观察助手上线或使用的前置条件 |

问答预设在“设置”中管理，网页搜索需主动开启。观察检查本身不调用模型；讨论由用户发起。基本面尚无自动监控结论，需结合来源核查。数据缺失只影响相应计算，已有结果保留原日期。规则详情与研究版富贵策略的区别见[富贵策略研究边界](docs/fugui-strategy.md)。

## 自建部署

当前采用 **React Web → Rust API → PostgreSQL**，AI Agent 独立部署。账号、会话与数据由自己管理，无第三方账户或云存储依赖。

服务器部署只有一个入口：根目录 [deploy.sh](deploy.sh)，在服务器手动执行。产品网站仍由该脚本手动部署，推送或创建 PR 不会自动部署网站；benchmark 的已有采集与评测使用独立 GitHub Actions。公网 IP、域名和可选参数集中在脚本顶部，逐项附中文注释。默认 Ubuntu/Debian 自动安装缺少的工具、生成持久密钥、构建源码，并通过 systemd 后台运行和开机自启，不需要 Docker。

```bash
sudo bash deploy.sh --ip 47.243.99.21 --domain www.madeagents.ai
# 以后更新源码后重新部署：sudo bash deploy.sh
# 管理：bash deploy.sh start|stop|restart|status|logs|check
```

默认网页入口是 https://www.madeagents.ai/mario。首次还需将自动生成的 Nginx 片段加入已有 HTTPS 站点，保留原网站和证书，具体步骤见 [部署与验收](docs/deploy-madeagents.md)。

频繁搬迁可执行 `npm run data:download -- --host user@server --remote-dir /srv/mario`，一键打包下载加密迁移包。目标恢复、反复覆盖和回退见 [数据迁移指南](docs/data-migration.md)。

**当前是在线服务模式**：多设备直接读写同一数据库，版本校验防止旧数据覆盖；离线不能保存。服务器可读取业务明文，供应商密钥按账号加密保存。

## 文档入口

- 产品目标与取舍：[产品主线](docs/product-vision.md)、[买点观察](docs/buy-watch.md)、[讨论方法](docs/methodology.md)。
- 使用与数据：[持仓交易](docs/holding-trades.md)、[行情与汇率](docs/market-data.md)、[每日资产](docs/daily-assets.md)、[复盘记录](docs/review-and-rules.md)、[参考资料](docs/research-evidence.md)。
- AI：[数据授权边界](docs/ai-data-boundary.md)、[问答流式回复](docs/ai-streaming.md)、[工作流契约](docs/ai-workflow.md)、[分析历史](docs/analysis-history.md)、[Codex 桥接](docs/codex-bridge.md)。
- 开发与运维：[协作规范](AGENTS.md)、[架构](docs/architecture.md)、[模块边界](docs/module-boundaries.md)、[自建部署](docs/self-hosting.md)、[数据迁移](docs/data-migration.md)、[账号与同步](docs/cloud-sync.md)、[信任边界](docs/threat-model.md)。
- 独立研究：[benchmark 当前能力与成绩](docs/benchmark.md)、[已有研究维护](docs/benchmark-maintenance.md)。`docs/audits/` 与明确标记的历史研究记录只供追溯，不构成当前待办。

## 仓库结构

```text
client/src/features/    买点观察、持仓、问答等独立页面
client/src/app/         导航、跨页组合与服务端数据生命周期
client/src/api/         按领域组织的 HTTP 客户端
server/src/http/        路由、认证后的协议适配
server/src/application/ 观察、分析与行情等业务用例
server/src/models/      契约与确定性规则
server/src/ports/       行情查询与模型执行接口
server/src/db/          账号仓储、事务、备份与迁移
server/src/ai/          冻结输入上的模型工作流
benchmark/             与账号数据隔离的离线研究工具
docs/                  当前行为、架构和历史审计
deploy.sh               宿主机部署与服务管理入口
scripts/               构建、迁移与验收
```

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

早期单机版的 SQLite 数据需用 Git 标签 `legacy-sqlite-export` 导出，再在新账号的“账号与数据”导入；密钥和旧账号会话不会导入，步骤见 [数据迁移](docs/data-migration.md#旧版-sqlite-数据)。PostgreSQL 备份和密码重置详见 [自建部署指南](docs/self-hosting.md)。

已有数据的账号可在“账号与数据”勾选“覆盖当前账号已有数据”，选择导出的 JSON 并确认。覆盖前自动下载原数据备份，随后替换当前账号的业务数据；不合并交易，也不影响其他账号。备份回退及接口说明见 [账号与数据同步](docs/cloud-sync.md)；本机 Codex 见 [Codex 桥接](docs/codex-bridge.md)，问答流式输出见 [问答流式回复](docs/ai-streaming.md)。
