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
}
```

新增供应商时实现这个接口即可，不需要修改投资方法论或 HTTP 层。当前的 `OpenAiCompatibleProvider` 是第一个适配器。

`InvestmentOrchestrator` 负责阶段组合：

1. 读取规则引擎已经计算的风险事实。
2. 让模型形成研究计划和检索线索。
3. 使用原问题与计划分别检索一次本地记忆并合并去重。
4. 按用户设置探索多个方案。
5. 使用独立提示词进行反方审查。
6. 最后整合结论、未知项、行动和证伪条件。

编排器不知道 API Key 的存储方式，也不直接访问数据库。它只接收 Provider、Retriever、Snapshot 和 MemoryItem，因此可以单元测试并在未来替换为图式工作流。

## 数据与安全边界

- 服务端只监听 loopback 地址，不暴露局域网端口。
- 桌面端把自身进程号传给 sidecar；桌面进程退出后，本地服务会自动停止。
- CORS 只允许桌面 WebView 和本地开发地址。
- 模型密钥保存在系统钥匙串。
- Base URL 默认要求 HTTPS，本机模型例外。
- 模型提示词明确禁止编造实时市场数据、收益保证和确定性买卖指令。
- 规则层输出与模型推理分离，降低大模型覆盖基础风险事实的概率。

后续安全工作：数据库加密、应用签名与公证、API 请求审计、模型数据发送预览、Prompt Injection 防护、自动更新签名。

## 推荐扩展顺序

1. 用数据库迁移工具替代当前幂等建表脚本。
2. 增加持仓 CSV 导入与组合级周期复盘。
3. 实现目标成功概率与再平衡建议的确定性计算。
4. 新增嵌入向量 Retriever，同时保留词法检索作为离线降级。
5. 增加受信任行情 Provider；外部数据必须标注来源和时间。
6. 为编排阶段增加 token、延迟和错误可观测性。
7. 使用 Mock Provider 完成端到端 AI 工作流测试。
