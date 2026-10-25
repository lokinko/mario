# 数据问题提示收敛方案（草稿）

> 状态：待评审。本文只给出分析与执行计划，尚未修改代码。评审确认后按“执行计划”分阶段实施，并在同次交付中同步 [架构说明](docs/architecture.md) 与 [模块边界](docs/module-boundaries.md)。

## 1. 问题陈述

“我的情况”页顶部经常出现“行动建议已暂停”的警告横幅，用户读完以后不知道哪里出了问题、为什么会这样，也不知道该怎么处理。原因不是某一处文案写得不好，而是一种设计模式在项目里被反复使用：

> **任何一项数据不完美，就关闭整个功能，再用一段警告告诉用户“已暂停”。**

这种做法看似安全，实际有三个代价：

1. **警告失去信号价值。** 数据完全“完美”的状态几乎达不到，所以横幅几乎一直挂着。用户学会忽略它以后，真正需要处理的问题也会被一起忽略。
2. **影响范围被放大。** 一项现金 8 天没有重新确认，会让整个组合的再平衡计算和 AI 调仓建议一起停掉。
3. **代码难以理解和修改。** 同一条规则分散在多个文件中，以“事后补丁”的方式叠加，没有人能从一个地方看清楚“什么情况下会暂停”。

## 2. 现状分析：“行动建议已暂停”是怎么产生的

### 2.1 生成链路

```text
valuation::status()               → decision_ready = warnings 为空 且 有持仓
  ↓
db/snapshot.rs::build_snapshot()  → !decision_ready 时清空 plan.rebalancing，拼接 assumptions
  ↓
db/portfolio.rs::snapshot_on()    → 再追加 6 类条件，逐个把 decision_ready 改成 false
  ↓                                 并再次 build_snapshot()，按 title 字符串挑回部分 findings
  ├→ 前端 MyFacts.tsx              → decisionReady === false 时显示“行动建议已暂停”横幅
  └→ AI context.rs / structured_output.rs
                                   → 强制 actions 只能是一句固定文案，否则判定输出无效并重试
```

### 2.2 触发 `decision_ready = false` 的全部条件

| # | 条件 | 位置 | 真的阻止计算吗？ | 当前影响范围 |
| --- | --- | --- | --- | --- |
| 1 | 外币持仓缺少汇率 | `valuation.rs` | **是**：无法求组合总值 | 全局 |
| 2 | 持仓缺少估值日期 | `valuation.rs`（重复报两次） | 否：金额存在，只是缺少日期 | 全局 |
| 3 | 任一持仓的估值日期超过 7 天（**包括现金和非上市资产**） | `valuation.rs` | 否 | 全局 |
| 4 | 任一外币持仓的汇率观察日超过 7 天 | `valuation.rs` | 否 | 全局 |
| 5 | 存在待确认的拆股或合股 | `db/portfolio.rs` | 对**该持仓**是：数量可能相差一个倍数 | 全局 |
| 6 | 存款余额待重新核对 | `db/portfolio.rs` | 否：余额已保留，只是可能有偏差 | 全局 |
| 7 | 交易链被隔离 | `db/portfolio.rs` | 对**该持仓**是：数量和成本不可信 | 全局 |
| 8 | 价格观察日超过 7 天 | `db/portfolio.rs`（与 #3 重复） | 否 | 全局 |
| 9 | 价格未确认，使用的是成交参考价 | `db/portfolio.rs` | 否：参考价是合理的近似值 | 全局 |
| 10 | 存在未平仓期权 | `db/portfolio.rs` | 否：只是期权不适合按股票再平衡 | 全局 |

10 个条件里只有 1 个真正让整个组合无法计算，另有 2 个只影响单个持仓；其余 7 个都属于数据新鲜度或可信度的说明，却被当作“必须暂停”的理由。

### 2.3 最荒谬的地方

- **页面上没有“行动建议”。** 前端从未渲染 `snapshot.findings` 和 `snapshot.plan.rebalancing`，这两个字段只进入 AI 上下文。横幅暂停的是一个在当前页面不存在的功能。
- **现金也受 7 天时效约束。** 现金持仓的 `valuation_date` 是余额核对日期，银行存款一周不重新确认就会触发全局暂停。
- **同一个原因被报 3–4 次。** 例如一只证券价格是 10 天前的，会同时出现“估值缺少有效日期或已超过 7 天”“实际价格观察日缺失或已超过 7 天”，以及 `plan.assumptions` 中拼接的同一句话；横幅和“查看原因”抽屉又把同一组 `warnings` 再显示一遍。
- **所有手工估值的资产都会额外生成一条风险项**“部分持仓仍是用户声明估值”，其中也包括现金。
- **AI 被迫输出固定的套话。** `decisionReady` 为 false 时，AI 的 `actions` 只能是“核对资产数据后再评估调仓”这句固定文案，措辞稍有不同就判定输出无效，需要额外调用模型重试。

## 3. 同类过度防御模式清单

下面这些不是“行动建议已暂停”的一部分，但属于同一种思路，建议一并处理或评审。

| 编号 | 模式 | 典型位置 | 问题 | 建议 |
| --- | --- | --- | --- | --- |
| D1 | 全局一票否决 | `decision_ready`、`comparable` | 局部问题导致全局停用 | 按持仓或币种限定影响范围（见第 5 节） |
| D2 | 同一问题多处报警 | `valuation.rs`、`portfolio.rs`、`findings`、`plan.assumptions`、横幅、抽屉 | 重复、冗长、互相矛盾 | 每个问题只生成一次，只在一个位置显示 |
| D3 | 一刀切的时效阈值 | 硬编码的 7 天（`valuation.rs`、`portfolio.rs`） | 现金、房产等资产不适用 | 时效只作为显示元数据（日期和距今天数），不作为门槛 |
| D4 | 噪音型风险项 | “部分持仓仍是用户声明估值” | 手工估值是正常用法，并非风险 | 删除，改为持仓行上的来源标签 |
| D5 | 用字符串充当协议 | 前端 `String(e).includes("结果待确认")`；后端按 `title` 过滤 `findings`；`plan.assumptions` 多次 `format!` 拼接 | 改一处文案就可能让逻辑失效 | 改为结构化的错误码、问题码和字段 |
| D6 | 快照二次构建与事后补丁 | `snapshot_on()` 两次调用 `build_snapshot()`，中间不断修改 `valuation_status` | 难以推理最终状态 | 先读取全部原始数据，然后单次构建 |
| D7 | 对 AI 输出逐字强制 | `structured_output.rs` 第 16、39–50 行 | 浪费模型调用，输出机械化 | 把数据问题作为上下文交给模型，只保留结构校验和引用校验 |
| D8 | 解释性的冗余文案 | 删除确认框中的长段说明、“金额已隐藏；复制和导出仍含真实数据”、“旧数据中还有其他币种存款……” | 把实现细节直接展示给用户 | 逐条评审，留到第 4 阶段处理 |

### 应当保留的防御（不属于过度防御）

以下机制防止的是**真实存在、后果严重**的错误，并且在正常使用时不会打扰用户，应该保留：

- 账号隔离和租户选择（第 8 节不变量）。
- `If-Match` 版本检查，以及财务写入回执与重放（防止网络丢失响应后重复扣款）。
- 十进制精确金额。
- 交易链隔离时**保留原始数据**（隔离本身合理，问题只是影响范围被扩大成全局）。
- AI 证据 ID 和引用路径校验（防止模型编造来源）。

判断标准是：这项防御在正常流程中是否可见；如果可见，它指出的是否是用户必须处理的问题。

## 4. 设计准则（评审通过后写入 AGENTS.md）

一条提示只有**同时满足**以下 4 点，才可以作为警告显示：

1. **可定位**：指向具体的持仓、币种或记录，而不是笼统地说“数据有问题”。
2. **可操作**：附带一个能直接完成修复的入口，例如“补汇率”“确认拆股”“修复交易链”。
3. **有后果**：不处理会让用户**看到错误的数字或错误的建议**。仅仅“不够新鲜”或“不是外部来源”不算后果。
4. **只出现一次**：同一问题只在离受影响数据最近的位置显示一次。

不满足以上条件的信息，降级为**数据元数据**（日期、来源标签、“参考价”标记），或直接删除。

功能降级的规则：**只停用无法正确计算的那一部分**。能计算的照常计算，并标明“基于某日的数据”。

## 5. 目标设计

### 5.1 统一的数据问题模型

在 `server/src/valuation.rs` 中用一个结构化的问题列表替代 `decision_ready` 和 `warnings: Vec<String>`：

```rust
pub enum IssueCode {
    MissingFxRate,          // 外币持仓无汇率
    PendingCorporateAction, // 待确认拆股或合股
    QuarantinedTrades,      // 交易链被隔离
    CashNeedsReconciliation,// 存款待重新核对
}

pub struct DataIssue {
    pub code: IssueCode,
    pub holding_ids: Vec<String>, // 或 currency，用于把问题定位到具体对象
    pub message: String,          // 一句话，面向用户
}
```

- **只有“会导致错误数字”的问题才进入 `issues`。** 价格时效、参考价、手工估值都不是问题，它们已经作为持仓字段存在（`valuation_date`、`price_status`、`holding_valuations`），由前端显示为元数据。
- `IssueCode` 是封闭枚举，前端按 `code` 决定修复入口，不再匹配中文字符串。

### 5.2 各类问题的影响范围

| 问题 | 影响什么 | 不影响什么 |
| --- | --- | --- |
| 缺汇率 | 组合总值、集中度和再平衡（总值是计算分母，无法部分计算） | 该持仓的原币金额；其他持仓 |
| 待确认拆合股 | 该持仓的估值，以及依赖总值的再平衡 | 其他持仓的显示 |
| 交易链隔离 | 同上 | 同上 |
| 存款待核对 | 只在该币种存款行标注“待核对” | 总值、再平衡照常计算 |
| 未平仓期权 | 把期权及其标的从再平衡目标中排除，在计划中注明 | 其他持仓的再平衡 |
| 目标权重合计不为 100% | 再平衡结果为空，计划区显示“目标权重合计 X%” | 其他所有计算 |
| 价格较旧、参考价、手工估值 | 无。只显示日期和来源 | — |

`plan` 改为结构化表达不计算的原因，不再拼接字符串：

```rust
pub struct PortfolioPlan {
    pub rebalancing: Vec<RebalanceAction>,
    pub rebalancing_blocked_by: Vec<IssueCode>, // 为空表示已计算
    pub excluded_holding_ids: Vec<String>,      // 例如期权
    pub as_of: Option<String>,                  // 参与计算的最早估值日期
    // 其余字段不变
}
```

### 5.3 快照构建：单次计算

`db/portfolio.rs::snapshot_on()` 改为：

```text
读取 profile、holdings、valuations、trades、cash_state、corporate_actions、quarantine、options
  → amounts::build（会计投影）
  → valuation::issues(...)          // 唯一判定问题的地方
  → build_snapshot(..., &issues)    // 只调用一次；planning 和 risk 根据 issues 决定范围
```

删除 `snapshot_on()` 中所有 `decision_ready = false`、`warnings.push`、`rebalancing.clear()` 补丁，删除第二次 `build_snapshot()` 调用和按 `title` 过滤 `findings` 的代码。

### 5.4 AI 侧

- `context.rs`：上下文中传 `portfolio.dataIssues` 和 `plan.rebalancingBlockedBy`，删除 `valuationStatus.decisionReady` 和 `warnings`。
- 系统策略第 5 条改为：

  > `dataIssues` 列出的持仓数值不可靠，不得据此给出精确金额；`rebalancingBlockedBy` 非空时不得输出组合层面的调仓金额。引用价格或估值时须说明其日期。

- `structured_output.rs`：删除第 16 行的固定文案要求和第 39–50 行的逐字校验，保留结构校验、证据 ID 校验和引用路径校验。

### 5.5 前端

| 当前 | 改为 |
| --- | --- |
| `MyFacts.tsx` 第 311–327 行的“行动建议已暂停”横幅，以及“估值待核对原因”抽屉 | **删除** |
| — | `AssetOverview` 只在存在 `issues` 时显示一行“N 项需要处理”，点击后定位到对应持仓 |
| `AssetAmountRow` 中黄色“待核对”徽章 | 中性的“参考价”标签和日期；只有出现在 `issues` 中的持仓才使用警告色，并附修复入口 |
| 存款待核对的 `Callout`（按币种逐个显示在页面顶部） | 移到对应币种的存款行内 |
| `String(e).includes("结果待确认")`（共 5 处） | `financialRequest` 抛出带 `kind: "pending"` 的类型化错误，调用方按类型判断 |

## 6. 执行计划

每个阶段单独提交，独立通过验证后再开始下一阶段。

### 阶段 1：后端问题模型与单次构建（核心）

1. `server/src/models.rs`：新增 `IssueCode`、`DataIssue`；`PortfolioValuationStatus` 删除 `decision_ready` 和 `warnings`，新增 `issues`；`PortfolioPlan` 增加第 5.2 节中的字段。
2. `server/src/valuation.rs`：`status()` 拆成 `issues()`，删除 7 天时效判定和重复的日期警告。
3. `server/src/db/portfolio.rs`：按第 5.3 节重写 `snapshot_on()` 的后半段。
4. `server/src/db/snapshot.rs`、`server/src/planning.rs`、`server/src/risk.rs`：根据 `issues` 决定计算范围；删除“部分持仓仍是用户声明估值”和“持仓缺少估值日期”两类风险项。
5. 测试改写：`db/tests.rs` 第 1273–1283 行、`db/financial_tests.rs`、`db/trades.rs`、`db/options.rs` 中对 `decision_ready` 的断言，改为断言具体的 `issues` 和影响范围。新增以下用例：
   - 现金 30 天未核对：再平衡照常计算。
   - 一项持仓交易链被隔离：`issues` 只指向该持仓。
   - 存在未平仓期权：期权从再平衡中排除，其他持仓照常计算。

**验收标准**：仓库中搜索 `decision_ready`、`暂停` 在 `server/src/db` 和 `valuation.rs` 中均无结果（迁移代码除外）；`cargo fmt --check`、`npm run test:server`、`cargo clippy --all-targets -- -D warnings` 全部通过。

### 阶段 2：AI 上下文与输出校验

1. `server/src/context.rs`：按第 5.4 节修改上下文和系统策略。
2. `server/src/ai/structured_output.rs`：删除固定文案约束及其测试（第 384–404 行附近），保留其他校验。
3. `server/src/ai/orchestrator.rs` 第 972 行、`context.rs` 第 351 行的测试夹具同步修改。

**验收标准**：存在 `dataIssues` 时，模型给出正常的结构化建议且一次通过校验；结构校验、证据校验的现有测试全部通过。

### 阶段 3：前端

1. `client/src/types.ts`：同步新契约。
2. `MyFacts.tsx`、`AssetOverview.tsx`、`AssetAmountRow.tsx`：按第 5.5 节修改；删除 `help === "valuation"` 分支。
3. `api/portfolio/financialRequest.ts`：定义类型化的待确认错误；修改 `HoldingTrades.tsx`、`StockOptions.tsx`、`useHoldingRemoval.ts` 中的 5 处字符串匹配。
4. 测试：`MyFacts.test.tsx` 删除“行动建议已暂停”相关断言，新增“只有存在问题的持仓显示修复入口”的用例。

**验收标准**：客户端构建、Prettier 和 Vitest 通过；在没有缺汇率、隔离或拆合股问题的账号中，“我的情况”页不显示任何警告色元素。

### 阶段 4：文案评审与文档同步

1. 逐条评审第 3 节 D8 列出的冗余文案，由你决定保留或删除。
2. 更新 `docs/architecture.md`：
   - 第 8 节“决策依据缺失、过期、隔离或待核对时，规划清空行动建议”改为“只停用无法正确计算的部分”。
   - 财务一致性约束表中“不可信数据生成行动”一行同步修改。
3. 更新 `docs/planning-model.md`，以及 `system_ui_design.md` 第 253 行。
4. 把第 4 节的设计准则写入 `AGENTS.md`。
5. 运行完整的 `npm test`（涉及前后端契约变更）和 `npm run check:architecture`。

## 7. 兼容性与迁移

- **快照是实时计算的，不持久化**，因此 `valuation_status` 结构变化不需要数据迁移。
- **历史分析记录**冻结的上下文中保留旧的 `decisionReady` 和 `warnings` 原文，属于审计记录，不改写；前端读取历史报告时不依赖这两个字段。
- **备份格式**（`db/sync_format.rs`）不包含快照派生字段，不受影响。
- 这是一次非兼容更新：旧字段、旧文案、旧测试在同次交付中删除，不保留并行实现。

## 8. 需要你决定的问题

1. **价格时效是否完全不提示？** 方案默认只显示日期。另一种做法是：对有公开代码的证券，价格超过 N 个交易日时，在持仓行显示“更新价格”按钮（不使用警告色）。
2. **缺汇率时，组合总值是否显示“不含该持仓”的部分合计？** 方案默认不显示总值，因为部分总值容易被误读。
3. **待确认拆合股是否只排除该持仓，再用剩余持仓计算再平衡？** 方案默认阻止组合层面的再平衡，因为它会让分母出错。
4. **D8 中的哪些文案需要保留？**
