# 架构说明

## 设计目标

1. **本地优先**：投资数据默认停留在用户设备。
2. **客户端与服务端解耦**：UI 不直接访问数据库、密钥或模型。
3. **规则与模型解耦**：能够确定计算的风险先由代码完成，大模型处理解释、比较和反思。
4. **模型可替换**：业务工作流只依赖 `ModelProvider` 接口。
5. **工作流可组合**：记忆、检索、多方案和反思是独立阶段，可以关闭或替换。
6. **同步可替换且零明文托管**：账户 Provider 可替换，云端对象存储只接触密文和版本元数据。

## 运行拓扑

```text
┌───────────────────────────────┐
│ Tauri Desktop Client          │
│ React UI + typed HTTP client  │
└───────────────┬───────────────┘
                │ 127.0.0.1:4217
┌───────────────▼───────────────┐
│ Local Axum Server             │
│                               │
│ API ─┬─ deterministic rules   │
│      ├─ planning simulation   │
│      ├─ review snapshots      │
│      ├─ versioned user rules  │
│      ├─ evidence retriever    │
│      ├─ context builder       │
│      ├─ workflow definition   │
│      ├─ AI orchestrator       │
│      ├─ memory retriever      │
│      ├─ provider adapter      │
│      └─ encrypted sync       │
└──────┬───────────────┬──────────────┘
       │               │ explicit analysis only
┌──────▼───────┐ ┌─────▼────────────────┐
│ SQLite       │ │ User-selected model   │
│ local data   │ │ OpenAI-compatible API │
└──────┬───────┘ └──────────────────────┘
       │
┌──────▼─────────────┐       explicit manual sync
│ OS Keychain        │ ┌─────────────────────────────┐
│ model key / tokens ├─┤ Account + ciphertext store  │
│ / recovery key     │ │ Supabase or self-hosted     │
└────────────────────┘ └─────────────────────────────┘
```

## AI 模块边界

`server/src/ai/provider.rs` 定义模型接口：

```rust
#[async_trait]
pub trait ModelProvider: Send + Sync {
    async fn complete(&self, messages: Vec<ChatMessage>) -> AppResult<ModelCompletion>;
    fn model_name(&self) -> &str;
    fn provider_name(&self) -> &str;
}
```

新增供应商时实现这个接口即可，不需要修改投资方法论或 HTTP 层。当前的 `OpenAiCompatibleProvider` 是第一个适配器。

`server/src/ai/workflow.rs` 中的 `AnalysisWorkflow` 定义阶段职责和提示契约，`InvestmentWorkflowV4` 是当前实现。`InvestmentOrchestrator` 通过 `with_workflow` 接受替代实现，并只负责执行、检索、计时、汇总与审计：

1. 读取规则引擎已经计算的风险事实。
2. 读取本地规划引擎生成的目标情景、风险预算和再平衡偏差。
3. 让模型形成研究计划和检索线索。
4. 从结构化记忆候选中，使用原问题与计划分别进行一次可解释混合检索并合并去重。
5. 让彼此隔离的“稳健基准”和“目标推进”模块分别生成候选方案。
6. 使用独立提示词进行反方审查。
7. 最后整合结论、未知项、行动和证伪条件。
8. 用独立模块校验结构化输出和证据 ID；失败时只修复一次，再失败则拒绝保存。

编排器不知道 API Key 的存储方式，也不直接访问数据库。它只接收 Workflow、Provider、Retriever、BuiltContext 和 MemoryItem，因此可以单元测试并在未来替换为图式工作流。每个模型调用返回统一的正文与可选 usage；执行器记录阶段耗时，完整研究计划、候选方案、批判和调用轨迹由 SQLite 本地保存。详细契约见 [AI 分析工作流](ai-workflow.md)。

`server/src/memory.rs` 的 `MemoryRetriever` 不依赖数据库或模型。当前 `HybridMemoryRetriever` 综合字段/内容匹配、投资概念关联、复盘可信度和时间衰减，并为每条结果生成可见命中原因。决策记忆由不可变原始快照与独立复盘动态构造，历史 AI 回答始终标为未经结果验证。详细契约见 [长期记忆与多轮检索](long-term-memory.md)。

`ContextBuilder` 位于 `server/src/context.rs`，负责在编排前执行最小披露策略。编排器不再接收完整 `Snapshot`，只接收经过用户选择、发送前预览和一致性指纹校验的 `BuiltContext`。候选记忆也在预览阶段冻结，后续检索不能越出该集合。详细契约见 [AI 数据边界](ai-data-boundary.md)。

周期系统复盘与个人投资规则属于独立的本地领域模型。复盘写入时冻结组合和方法指标；规则更新采用追加版本，当前状态与历史证据同时保留。两类信息都由 `ContextBuilder` 单独控制，用户可以在每次 AI 分析前选择是否发送。详细契约见 [复盘与规则闭环](review-and-rules.md)。

`server/src/evidence.rs` 定义独立的 `EvidenceRetriever`。当前词法实现按问题与持仓名称筛选最多 12 条有效记录；候选集合在预览时冻结，归档或新增相关证据会使旧指纹失效。证据内容由用户整理，服务端校验 HTTPS、日期和结构，但不声称已核验来源正文。未来外部行情与基本面 Provider 应写入同一个证据契约。详细说明见 [研究证据与引用](research-evidence.md)。

## 数据与安全边界

- 服务端只监听 loopback 地址，不暴露局域网端口。
- 桌面端把自身进程号传给 sidecar；桌面进程退出后，本地服务会自动停止。
- CORS 只允许桌面 WebView 和本地开发地址。
- 模型密钥保存在系统钥匙串。
- API Key 只进入 HTTP Authorization 请求头，不进入提示词、预览或分析审计。
- AI 分析必须携带与当前本地上下文一致的预览指纹。
- Base URL 默认要求 HTTPS，本机模型例外。
- 模型提示词明确禁止编造实时市场数据、收益保证和确定性买卖指令。
- 规则层输出与模型推理分离，降低大模型覆盖基础风险事实的概率。
- 云同步使用固定数据表白名单与 XChaCha20-Poly1305；恢复密钥不上传。
- 同步写入以云端 revision 做原子比较；并发修改不会静默覆盖。
- 拉取完成解密、结构和内容指纹检查后，才在一个 SQLite 事务中替换投资域数据；本地设置和密钥不在事务范围内。

账户与云同步的协议、冲突语义和 Supabase RLS 参考迁移见 [账户与端到端加密云同步](cloud-sync.md)。

后续安全工作：数据库加密、应用签名与公证、进程间认证令牌、Prompt Injection 防护和自动更新签名。

## 推荐扩展顺序

1. 用数据库迁移工具替代当前幂等建表脚本。
2. 增加持仓 CSV 导入与组合变化归因。
3. 新增本地嵌入向量 Retriever，与当前可解释检索融合并保留离线降级。
4. 增加受信任行情 Provider；外部数据必须标注来源和时间。
5. 为工作流增加可恢复 checkpoint 与版本评测。
6. 扩展 HTTP 级 Mock Provider 场景，覆盖超时、限流和中途断线。
