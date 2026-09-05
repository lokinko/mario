# 知衡（Compass Invest）

知衡是一个本地优先、AI-native 的投资决策辅助软件。它不以行情、K 线或荐股作为产品中心，而是通过财务底座、目标配置、风险检查、决策日志和复盘，帮助用户形成可验证的投资方法。

> 当前版本用于投资教育与决策支持，不承诺提高收益，也不替代持牌专业人士针对个人情况提供的建议。

## 当前能力

- macOS 桌面客户端：Tauri + React + TypeScript
- 独立本地服务端：Rust + Axum，仅监听 `127.0.0.1:4217`
- 本地 SQLite：财务档案、目标、可编辑持仓、决策与分析历史
- 系统钥匙串：模型 API Key 不写入数据库
- 确定性风险规则：应急资金、负债压力、集中度、期限错配
- 确定性规划：目标路径模拟、月度投入缺口、风险预算与再平衡偏差
- OpenAI-compatible 模型适配器
- AI 深度工作流：研究计划 → 两轮结构化长期记忆检索 → 两个独立候选方案 → 独立反思 → 最终裁决
- 结构化输出校验：事实、推断、未知、方案、行动与复盘条件必须通过机器契约，非法引用自动拦截并允许一次修复
- 可解释长期记忆：区分已复盘决策、未验证判断与历史 AI 回答，支持时间衰减和反证信号
- 逐条记忆授权：发送前可单独排除候选记录，选择变化后必须重新生成一致性指纹
- AI 工作流轨迹：逐阶段保存研究产物、模型耗时与 Provider 返回的 token 用量
- AI 快速工作流：用于低成本的单轮结构化分析
- AI 数据预览：逐组选择上下文、冻结候选记忆、确认指纹并保存本地审计
- 决策复盘：原始判断不可变，复盘单独记录结果、过程评分与经验修正
- 周期系统复盘：冻结当时组合、目标、风险、决策完成度与纪律执行记录
- 个人投资规则：从复盘沉淀触发条件与行动，任何修订都保留历史版本
- 研究证据账本：记录来源层级、HTTPS 链接、资料日期、支持/反驳关系与限制
- 证据检索与引用：按问题和组合筛选，发送前冻结，要求 AI 只引用实际进入载荷的来源
- 简化概率校准：使用历史置信度与逻辑结果训练概率意识，不用单笔盈亏评价能力
- 账户与手动云同步：投资域数据端到端加密后上传，支持恢复密钥与版本冲突保护

## 仓库结构

```text
client/                 React 桌面界面与 Tauri 外壳
  src/                  页面、类型和本地 API 客户端
  src-tauri/            启动/打包本地 server sidecar
server/                 可独立启动的本地 HTTP 服务
  src/ai/               可替换工作流、执行器与模型 Provider
  src/db.rs             SQLite 持久化
  src/memory.rs         可替换、可解释的结构化记忆检索接口
  src/evidence.rs       可替换的研究证据检索接口
  src/risk.rs           不依赖大模型的风险规则
docs/                   架构与投资方法论
scripts/                sidecar 构建脚本
```

产品为什么存在、长期不应偏离什么，见 [产品目标与长期原则](docs/product-vision.md)。详细设计见 [架构说明](docs/architecture.md)、[AI 工作流契约](docs/ai-workflow.md)、[长期记忆与多轮检索](docs/long-term-memory.md)、[投资方法论](docs/methodology.md)、[研究证据与引用](docs/research-evidence.md)、[复盘与规则闭环](docs/review-and-rules.md)、[确定性规划模型](docs/planning-model.md)、[AI 数据边界](docs/ai-data-boundary.md) 与 [账户和端到端加密云同步](docs/cloud-sync.md)。

## 本地开发

要求：Node.js 20+、Rust 1.86、macOS 开发工具。

```bash
npm run install:all
npm run dev
```

浏览器客户端运行在 `http://localhost:1420`，本地服务运行在 `http://127.0.0.1:4217`。

桌面调试：

```bash
npm run desktop:dev
```

运行全部静态检查和单元测试：

```bash
npm test
```

## 构建安装包

```bash
npm run desktop:build
```

脚本会先把 `server` 编译成当前平台的 Tauri sidecar，再生成 `.app` 与 `.dmg`。未签名构建适合本机测试；对外发布还需要 Apple Developer 签名、公证和自动更新配置。

## 模型配置

在客户端的“模型与隐私”页面填写：

- API Base URL
- 模型名称
- API Key

当前支持 `/chat/completions` 协议的 OpenAI-compatible 服务。允许 HTTPS 远端地址，也允许 `localhost`/`127.0.0.1` 上的 HTTP 本地模型。API Key 由系统钥匙串保存；SQLite 只保存非敏感元数据。

保存后可在页面内测试连接，也可以随时从系统钥匙串移除密钥。

## 数据位置

默认数据库位于操作系统的本地数据目录 `com.compassinvest.desktop/compass.db`。开发和测试时可通过 `COMPASS_DATA_DIR` 指定隔离目录。

服务端只在用户主动发起 AI 分析时，将完成该任务所需的投资上下文发送给用户配置的模型服务。云同步同样只在用户主动操作时发生，模型密钥、登录令牌与恢复密钥不会进入同步包。当前版本尚未实现本地数据库整体加密、证券行情源或券商交易。
