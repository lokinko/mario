# 架构说明

## 设计目标

1. **本地优先**：投资数据默认停留在用户设备。
2. **客户端与服务端解耦**：UI 不直接访问数据库、密钥或模型。
3. **规则与模型解耦**：能够确定计算的风险先由代码完成，大模型处理解释、比较和反思。
4. **模型可替换**：业务工作流只依赖 `ModelProvider` 接口。
5. **工作流可组合**：记忆、检索、多方案和反思是独立阶段，可以关闭或替换。

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
│      ├─ AI orchestrator       │
│      ├─ memory retriever      │
│      └─ provider adapter      │
└──────┬────────────────┬───────┘
       │                │ on explicit analysis only
┌──────▼───────┐  ┌─────▼────────────────┐
│ SQLite       │  │ User-selected model   │
│ local data   │  │ OpenAI-compatible API │
└──────────────┘  └──────────────────────┘
       │
┌──────▼──────────┐
│ OS Keychain     │
│ API key only    │
└─────────────────┘
```

## AI 模块边界

`server/src/ai/provider.rs` 定义模型接口：

```rust
#[async_trait]
pub trait ModelProvider: Send + Sync {
    async fn complete(&self, messages: Vec<ChatMessage>) -> AppResult<String>;
    fn model_name(&self) -> &str;
    fn provider_name(&self) -> &str;
}
```

新增供应商时实现这个接口即可，不需要修改投资方法论或 HTTP 层。当前的 `OpenAiCompatibleProvider` 是第一个适配器。

`InvestmentOrchestrator` 负责阶段组合：

1. 读取规则引擎已经计算的风险事实。
2. 读取本地规划引擎生成的目标情景、风险预算和再平衡偏差。
3. 让模型形成研究计划和检索线索。
4. 使用原问题与计划分别检索一次本地记忆并合并去重。
5. 按用户设置探索多个方案。
6. 使用独立提示词进行反方审查。
7. 最后整合结论、未知项、行动和证伪条件。

编排器不知道 API Key 的存储方式，也不直接访问数据库。它只接收 Provider、Retriever、BuiltContext 和 MemoryItem，因此可以单元测试并在未来替换为图式工作流。

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

后续安全工作：数据库加密、应用签名与公证、进程间认证令牌、Prompt Injection 防护和自动更新签名。

## 推荐扩展顺序

1. 用数据库迁移工具替代当前幂等建表脚本。
2. 增加持仓 CSV 导入与组合变化归因。
3. 新增嵌入向量 Retriever，同时保留词法检索作为离线降级。
4. 增加受信任行情 Provider；外部数据必须标注来源和时间。
5. 为编排阶段增加 token、延迟和错误可观测性。
6. 使用 Mock Provider 完成 HTTP 级 AI 工作流测试。
