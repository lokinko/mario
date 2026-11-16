# OpenAI API 接入验证（2026-11-16，历史记录）

## 目的与范围

验证用户提供的 OpenAI API Key 能否通过 Mario 现有模型配置与问答路径调用。沿用[架构说明第 6.2 节](../architecture.md#62-ai-分析)的冻结、执行与保存流程，以及[模块边界](../module-boundaries.md#执行接口与适配器)的模型连接接口，不新增供应商适配器、配置入口、存储或迁移。操作方法见[自建服务](../self-hosting.md#高级运行与开发)。

本次复用本机已有的 `server/target/debug/mario-server`，没有重新编译；二进制 SHA-256 为 `7bf2c9177805ce163657409039f273db21536ce91eb57319c7bd948d8ebdcfcf`。检查时源码 HEAD 为 `d89752a`，不据此声称已有二进制由该提交构建。

通过 `scripts/postgres-cluster.mjs` 创建一次性 PostgreSQL 集群，在临时 API 中注册空账号，走与生产一致的认证、租户、版本检查、模型配置加密和问答接口。未连接开发或线上数据库，未使用真实持仓、现金或成交，未修改现有账号设置，未部署或重启既有服务。问答采用进程内 Agent 执行路径；本次没有验证独立 Agent 部署或网页交互。

## 实际结果

| 检查 | 结果 |
| --- | --- |
| OpenAI `GET /v1/models` | HTTP 200，返回可用模型列表 |
| Mario `PUT /api/model-config`、`GET /api/model-config` | 保存 `openai-responses`、`https://api.openai.com/v1` 和模型名；读取返回 `hasApiKey=true`，不含原始密钥 |
| `gpt-5.4-mini` 连接测试 | `POST /api/model-config/test` 返回 `ok=true`，耗时 2840 毫秒 |
| `gpt-5.4-mini` 完整问答 | HTTP 502；首次输出引用未授权的 `/facts/summary`，一次修复后仍缺少有效 `sourceRefs`，按现有规则拒绝，没有放宽校验 |
| `gpt-5.4` 完整问答 | 预览后携带 `previewRevision` 执行成功；本机请求耗时 39620 毫秒，生成加一次格式修复共 2 次模型调用 |
| 结构化结果 | `structuredOutputValidated=true`，报告包含 `answer`、`suggestions`、`facts`、`question`；返回的输入 token 合计 3881、输出 token 合计 2349 |
| 历史持久化 | `GET /api/analyses` 找到结果，`GET /api/analyses/{id}` 可读回详情 |
| 密钥隔离与清理 | 配置读取、预览与成功问答结果均不含原始密钥；成功测试后删除临时账号密钥，所有一次性集群均由测试工具停止并删除 |

两次完整问答均关闭联网、个人档案、持仓和本地资料授权，只输入通用观察问题。成功问题大意为：“我只观察沪深 A 股，尚未提供持仓和现金资料；为什么技术观察信号不能直接转成买入，下一步应核查什么？”模型区分了技术关注、估值与分红持续性，没有生成买入数量。该样本不能证明投资建议质量或模型普遍可靠性，也没有验证网页搜索权限、其他模型或 Chat Completions。

密钥由本机验证进程消费，不写入仓库文件、测试脚本、报告或应用日志。模型列表读取之外，本轮普通连接生成 1 次，小模型问答及修复 2 次，`gpt-5.4` 问答及修复 2 次，共 5 次生成请求；未据模型目录推断余额或费用。

## 交付与清理

生产代码无需修改，本轮仅补充接入操作说明和这份历史记录。文档检查包括引用路径、`git diff --check` 和敏感凭据扫描；没有运行与文档修改无关的业务门禁。真实调用结果如上，不将连接测试或单个成功问题等同完整回归通过。

本轮没有生成 Rust 编译产物；已有 `server/target` 正被原开发服务使用，保留该服务和产物。临时验证脚本所在的 `.runtime/openai-api-smoke/` 在确认任务进程结束、没有文件占用后删除；未清理其他任务的 `.runtime` 文件、数据库或缓存。
