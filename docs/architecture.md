# 架构

当前生产结构：React Web → Rust API → PostgreSQL；Rust API → 独立 AI Agent → 模型提供方。完整配置与运行边界见 [自建部署](self-hosting.md)。

```mermaid
flowchart LR
    W[网页] --> G[Nginx HTTPS]
    G --> A[API：认证、业务规则、上下文授权]
    A --> P[(PostgreSQL：账号与用户关系表)]
    A --> R[无状态 AI Agent]
    R --> M[模型提供方]
```

- `client/src/api/` 按领域提供接口；`lib/httpClient.ts` 统一处理认证、超时、版本和账号切换保护。`api.ts` 仅保留旧导入兼容出口。`app/useApplicationData.ts` 管理启动、同步、草稿保护和数据刷新。
- `server/src/hosted.rs` 根据服务端会话选择用户数据库，执行版本检查、数据库 advisory lock、导出和导入。用户请求不能指定租户。
- `server/src/accounts.rs` 管理公开注册、Argon2 密码、随机会话及撤销和登录限速。任何用户均可通过邮箱和密码注册，会话在数据库仅保存哈希。
- `server/src/db/` 保留领域仓储和事务校验；`storage.rs` 隔离 SQL 引擎。PostgreSQL 保存关系表，SQLite 仅用于旧文件迁移和本地开发回归。
- `server/src/runtime.rs` 负责启动配置与维护命令，`http/` 按功能注册路由和转换输入输出，`application/` 复用组合、分析和模型配置的业务流程。
- `server/src/models/` 保存每日资产、行情值和分析轨迹的数据契约。数据库不引用 AI 工作流实现、网络 Provider 或行情刷新模块；HTTP 错误映射独立放在 `http/error.rs`。
- `server/src/agent.rs` 定义冻结上下文的执行接口，可连接独立进程或在本地开发内联执行。工作流、Provider、输出验证仍在 `server/src/ai/`，其代码不调用仓储。
- `server/src/secrets.rs` 在托管模式用主密钥加密用户凭据，AAD 包含用户 schema 和密钥用途；本地开发同样使用主密钥加密，按密钥用途绑定 AAD。

账号使用独立 PostgreSQL schema，而不是共用单用户 AppState。每个请求只持有已认证用户的连接；连接关闭即释放任何未显式释放的会话锁。业务写入和数据版本在同一事务推进，避免应用崩溃造成“数据已变而版本未变”。多个 API 实例使用同一数据库锁，客户端旧版本写入返回 409。

AI 分析的预览与执行共享同一准备过程，读取并冻结用户授权的数据；外部模型工作期间释放账户锁，避免长时间阻塞其他设备。应用服务依赖 `AnalysisExecutor` 执行接口，Agent 适配器负责独立进程调用或本地内联运行。结果保留输入版本与分析轨迹，随后通过 API 写入分析历史。独立 Agent 运行不需要数据库连接或账号服务。

当前同步是在线版本检查及按需刷新，没有离线写入队列、实时推送或持久 AI 作业队列。连接按请求创建，业务请求并发有上限；此实现面向小规模自建，进一步扩大并发应增加连接池、分页/增量资源接口和任务队列。

源码层面的职责、依赖方向、生命周期、扩展步骤与本轮验证见 [模块边界与开发约束](module-boundaries.md)。运行 `npm run check:architecture` 可检查反向依赖、跨功能页面引用和依赖循环；常规测试入口与 CI 都执行该检查。
