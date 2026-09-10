# mario

mario 是一个本地优先、AI-native 的投资决策辅助软件。它不以行情、K 线或荐股作为产品中心，而是通过财务底座、目标配置、风险检查、决策日志和复盘，帮助用户形成可验证的投资方法。

> 当前版本用于投资教育与决策支持，不承诺提高收益，也不替代持牌专业人士针对个人情况提供的建议。

## 当前能力

- macOS 桌面客户端：Tauri + React + TypeScript
- 独立且可嵌入的本地服务端：Rust + Axum；原生客户端使用随机回环端口和每次启动的认证令牌
- 本地 SQLite：财务档案、目标、可编辑持仓、决策与分析历史
- 系统钥匙串：模型与行情 API Key 不写入数据库或同步包
- 确定性风险规则：应急资金、负债压力、集中度、期限错配
- 确定性规划：目标路径模拟、月度投入缺口、风险预算与再平衡偏差
- OpenAI-compatible 模型适配器
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
- 账户与手动云同步：投资域数据端到端加密后上传，支持恢复密钥与版本冲突保护

## 仓库结构

```text
client/                 React 响应式界面与 Tauri 桌面/Android 外壳
  src/                  页面、类型和本地 API 客户端
  src-tauri/            桌面启动 sidecar，Android 内嵌同一服务库
server/                 可独立启动、也可嵌入客户端的本地 HTTP 服务
  src/ai/               可替换工作流、执行器与模型 Provider
  src/db.rs             SQLite 持久化
  src/memory.rs         可替换、可解释的结构化记忆检索接口
  src/evidence.rs       可替换的研究证据检索接口
  src/valuation.rs      基准币种折算、估值完整性与资产类别变化
  src/market_data.rs    可替换市场数据 Provider、ECB 汇率与 Twelve Data 证券价格适配器
  src/performance.rs    流水分类汇总与现金流调整后期间回报
  src/event_import.rs   CSV 解析、字段映射与不可信输入边界
  src/risk.rs           不依赖大模型的风险规则
docs/                   架构与投资方法论
scripts/                sidecar 构建脚本
```

产品为什么存在、长期不应偏离什么，见 [产品目标与长期原则](docs/product-vision.md)。详细设计见 [架构说明](docs/architecture.md)、[本地原生应用威胁模型](docs/threat-model.md)、[Android 调试构建](docs/android.md)、[AI 工作流契约](docs/ai-workflow.md)、[可审计的 AI 分析档案](docs/analysis-history.md)、[从 AI 分析到用户决策](docs/analysis-to-decision.md)、[长期记忆与多轮检索](docs/long-term-memory.md)、[投资方法论](docs/methodology.md)、[研究证据与引用](docs/research-evidence.md)、[复盘与规则闭环](docs/review-and-rules.md)、[组合变化归因](docs/portfolio-attribution.md)、[组合流水 CSV 导入](docs/portfolio-event-import.md)、[可追溯汇率数据](docs/market-data.md)、[确定性规划模型](docs/planning-model.md)、[AI 数据边界](docs/ai-data-boundary.md) 与 [账户和端到端加密云同步](docs/cloud-sync.md)。

## 本地开发

桌面要求：Node.js 20+、Rust 1.86、macOS 开发工具。

```bash
npm run install:all
npm run dev
```

浏览器开发客户端运行在 `http://localhost:1420`，开发服务默认运行在 `http://127.0.0.1:4217`。打包后的桌面客户端会为每次启动选择独立的随机回环端口，不使用固定端口。

直接启动服务端时必须通过 `MARIO_AUTH_TOKEN` 提供至少 32 字符的随机令牌；只有本地浏览器开发可以显式使用 `--allow-unauthenticated-dev`。

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

脚本会先把 `server` 编译成当前平台的 Tauri sidecar，再生成 `.app` 与 `.dmg`。当前使用 ad hoc 签名，适合本机测试；对外发布还需要 Apple Developer 签名、公证和自动更新配置。

Android 调试 APK：

```bash
npm run android:init   # 首次或需要重建 Android 工程时
npm run android:build
adb install -r outputs/mario_0.5.0_android-aarch64-debug.apk
```

Android 版把同一个 Rust/Axum 服务库编译进应用进程，不依赖桌面 sidecar；SQLite 保存在应用沙盒，API Key、账户令牌与恢复密钥保存在 Android Keystore。详细环境要求、架构和调试边界见 [Android 调试构建](docs/android.md)。

Supabase 云同步部署与真实双设备测试：

```bash
supabase db query --linked --project-ref <project-ref> \
  --file server/migrations/supabase-cloud-sync.sql
cargo build --manifest-path server/Cargo.toml
npm run supabase:test -- <project-ref>
```

完整的账户、RLS、端到端加密和恢复密钥边界见 [账户和端到端加密云同步](docs/cloud-sync.md)。

## 模型配置

在客户端的“模型与隐私”页面填写：

- API Base URL
- 模型名称
- API Key

当前支持 `/chat/completions` 协议的 OpenAI-compatible 服务。允许 HTTPS 远端地址，也允许 `localhost`/`127.0.0.1` 上的 HTTP 本地模型。API Key 由系统钥匙串保存；SQLite 只保存非敏感元数据。

保存后可在页面内测试连接，也可以随时从系统钥匙串移除密钥。

同一页面还可配置个人 Twelve Data API Key。证券价格只在用户主动操作时查询；密钥通过服务端 Authorization 请求头发送，不进入 URL、SQLite、AI 上下文或云端同步。mario 保存的是指定日期的未复权日收盘价及其来源，不把它描述为实时成交价或内在价值。可用市场、额度、展示与再分发权利取决于用户自己的 Twelve Data 方案和交易所条款。

## 数据位置

桌面版数据库默认位于操作系统的本地数据目录 `com.lokinko.mario/mario.db`；Android 版位于应用专属沙盒。开发和测试时可通过 `MARIO_DATA_DIR` 指定隔离目录。桌面升级时会自动迁移旧版数据目录和数据库文件；旧的 `COMPASS_DATA_DIR`、`COMPASS_AUTH_TOKEN` 仍作为过渡兼容别名。

服务端只在用户主动发起 AI 分析时，将完成该任务所需的投资上下文发送给用户配置的模型服务。用户主动查询汇率时，ECB 只收到币种与日期；用户主动查询证券估值时，Twelve Data 收到代码、日期和账户密钥。云同步同样只在用户主动操作时发生，模型与行情密钥、登录令牌、恢复密钥与本机提醒设置不会进入同步包。原生应用提醒必须由用户主动开启并授权系统通知；应用关闭后不会在后台运行。当前版本尚未实现本地数据库整体加密、现金流时点全组合估值、严格 TWR 或券商交易执行。
