## Material Passport

- Origin Skill: academic-research-suite / experiment-agent
- Origin Mode: run
- Origin Date: 2026-11-02
- Verification Status: VERIFIED（研究输入、适配代码与重复执行；不是原文组合复现）
- Version Label: external_strategies_batch_2_long_signals

# 外部长窗口策略：同源历史对账与第二批验证

本批从真实历史日线构造长窗口信号，比较 5 类规则、35 组配置。最高分为年度高点/低波排名合成的 **2.9548**，低于当前第十名 **3.0039**，没有新策略准入。现有最高分仍为 `trend_core_satellite` 的 **3.4959**。

## 设计与数据归属

沿用[架构说明的离线研究边界](../architecture.md#离线-benchmark-研究工具)：独立公开行情缓存 → 严格截至决策日的特征 → 不可覆盖研究切片 → 隔离策略执行器 → 既有评分器。新增程序为 [`scripts/benchmark_long_history_study.py`](../../scripts/benchmark_long_history_study.py)，只使用标准库和既有离线模块，不访问网络、账号数据库或在线应用，也不上传数据。

原始切片 `pools-v5-20261031` 的 manifest SHA-256 为 `edd3eb699103c318d447960bc0869f929f4339a6e667d1b718a5faabeb920edc`。研究切片为 `.runtime/benchmark/pools-long-history-study-20261102/`，保留同样的 148 期、2,960 个股票决策记录，只在股票记录内增加 `long_history`。原字段逐项相同，`outcomes.jsonl` 逐字节相同；未来路径只在全部特征生成后复制，特征计算不读取标签。

研究切片不是线上 HF v5 的替换版本，网站继续使用原 v5 与正式策略目录。若后续胜出策略需要新增字段，必须先完成正式切片及部署输入迁移，不能直接提交一个对现有输入报错的策略。

## 同源历史与异常处理

使用 Baostock 后复权最小字段 `date,code,close,high,tradestatus`，固定原切片全部 172 只历史证券，覆盖各股首次入池年份前四年到最后入池年末，保留退出证券。归档共 350,956 行。5 个长区间请求失败，另 2 个请求停在 2,000 行却仍报成功；保留原响应，七股改按两年分块查询、合并后检查日期和代码。最终所有已知可交易决策日都被覆盖，首尾观察之间无独立交易日历缺口。

新历史重新计算原 v5 的非空字段：63 日动量 2,796 项、20 日收益 2,827 项，绝对误差都为 0。请求快照、原始响应、分块、修复归档和验证结果位于 `.runtime/benchmark/history-baostock-20261102/`；构建器会再次验证共同字段、完整区间及原始切片哈希，不只信任 `status=completed`。

Yahoo 最大差异的独立调查见[研究清单](../benchmark-research.md#长历史来源核查)：腾讯未复权的 `30/27.24-1` 与 v5 一致；Yahoo 同时记录两次 2011 年 12:10 拆股，而公司报告明确该次 A 股除权在 7 月。重复/错时事件是有证据支持的解释，内部计算原因尚未证实。本批使用已对账 Baostock，不跨供应商拼接或按股票/日期硬修特征。

## 特征计算和覆盖

令 `P[k]` 为决策月往前 k 个自然月最后一个独立交易日的后复权收盘价，`P[0]` 为决策日收盘。月末停牌、端点缺失不前填，相关特征为 null 并附结构化问题码。全年路径要求完整；不能用上市以来短窗口冒充一年。

| 字段 | 定义 | 非空记录 / 2,960 |
| --- | --- | ---: |
| `momentum_12m` | `P[0]/P[12]-1` | 2,742 |
| `momentum_6m_skip1` | `P[1]/P[7]-1` | 2,773 |
| `momentum_12m_skip1` | `P[1]/P[13]-1` | 2,732 |
| `momentum_12_2` | `P[1]/P[12]-1`，明确为 11 个月累计窗口 | 2,731 |
| `price_to_sma_10m` | `P[0] / mean(P[0]…P[9])-1` | 2,479 |
| `close_to_high_12m` | 决策收盘 / 十二个自然月内日最高价最大值 | 2,837 |
| `close_to_max_close_12m` | 决策收盘 / 同窗口最高收盘 | 2,837 |
| `information_discreteness` | `sign(momentum_12_2) × (负日数−正日数)/观察数` | 2,731 |

年度窗口从 `P[12]` 的下一交易日至决策日，含决策日。FIP 每日符号窗口终止于 `P[1]`，零收益日计入分母；停牌日不算观察，复牌收益与前一有成交收盘比较。缺任一内部交易日时年度高点与 ID 不可用，而仍能计算的端点收益保留。六月中途缺一行不会被整体“跳过”来伪造完整窗口。仅在 63 日原波动明确存在时允许高点/低波合成，缺必要信号的股票不参加该规则。

测试验证：向决策后追加极端价格和未来日历不改变任何特征；月末偏移、十个月均值、正负与零收益、停牌观察数、内部缺失、短上市史、伪成功截断请求均有明确验证。实际 2,960 条记录全部保留，没有为提高覆盖率而删除失败样本或整期决策。

## 原文、改写和参数范围

- **E01 双动量**：依据 [Antonacci 原作者的规则区分](https://www.optimalmomentum.com/dual-relative-absolute-momentum/)，改为池内 12 个月相对排序，再检查绝对收益为正。前 N=1/3/5/6/10，各股固定 `1/N`，负信号和缺名额留现金，持有至评分窗口结束。没有无风险收益调整、池外防御资产或完整 GEM 轮动。
- **E06 均线择时**：[Faber 作者回顾](https://mebfaber.com/2017/12/13/episode-86-quantitative-approach-tactical-asset-allocation/) 使用 10 个月均线。这里固定每股 `1/20` 预算，仅当月末价严格高于均线时买入；季度重新决策，不在三个月内做原文的月度切换，现金收益为 0。只测这一固定规则，不扫描均线长度。
- **E09 年度高点**：[George/Hwang 作者全文 §I、表 I–IV](https://www.bauer.uh.edu/tgeorge/papers/gh4-paper.pdf) 用当前价格与过去十二个月高点的比值；本批按其描述性表格不跳月。分别比较日最高价/最高收盘价版本，前 N=1/3/5/6/10，等权长仓。原文月度重叠六个月、买前 30% 卖后 30% 的多空组合未复现；20 股池前 6 只配置包含对应的长侧比例，但不是原组合收益。
- **E08 FIP**：[2011-11 工作稿 §2–3](https://business.uq.edu.au/sites/default/files/events/files/mitch-warachka-paper.pdf) 用 PRET 条件下的 ID 区分连续/离散信息。此处以明示的 12-2 端点形成累计收益，取正收益赢家前 20%/30%/50%，再从中按 ID 升序选 1/2/3 只等权。原稿的财务过滤、完整五分组、多空及六个月持有均未复现，不能等同于机构完整量化动量流程。
- **高点/低波合成扩展**：受 E09 信号启发的本地扩展，按“距高点近的名次 + 63 日波动低的名次”升序选 N=1/3/5/6/10，日高/收盘高两个版本；等权持有到期。它不是 George/Hwang 原文规则。

所有规则的并列值按证券代码升序，不使用年月、证券身份或未来信息分支；没有附加现有家族的止盈、网格来掩盖外部信号的独立表现。参数按全部 148 期总分选择，早段 84 期、后段 64 期仅作诊断，**没有严格样本外检验**。

## 实测结果

下表各类只列最优参数，分数由正式隔离执行器输出，早后段为同一配置的分段。评分公式、成交限制和成本沿用 v5。

| 规则 | 最优参数 | 总分 | 平均收益得分 | 胜率 | 2006–2017 总分 | 2018–2025 总分 |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| 高点/低波合成 | 日最高价版本、前 1 | 2.9548 | 1.5190 | 57.4% | 3.3321 | 2.4597 |
| FIP 适配 | 赢家前 50%、ID 前 1 | 2.8760 | 1.5077 | 54.7% | 2.2914 | 3.6432 |
| 年度高点长仓 | 日最高价版本、前 5 | 2.6155 | 1.1628 | 58.1% | 2.7750 | 2.4061 |
| 12 个月双动量 | 前 5 | 1.9659 | 0.6315 | 53.4% | 2.0986 | 1.7918 |
| 10 月均线 | 固定规则 | 1.8064 | 0.4213 | 55.4% | 1.9730 | 1.5877 |

这些结果不能确认或否定外部原策略，也没有证明最高候选比原有策略显著优劣。35 次筛选与既有多轮研究增加选择偏差；没有做未经支持的显著性陈述。

所有候选低于现有第十名，按规范不加入 `benchmark/strategies/`。当前 Top-10 及名次变化：1 `trend_core_satellite`、2 `trend_single`、3 `calm_mid_momentum_sector`、4 `trend_risk_budget`、5 `calm_mid_momentum`、6 `trend_state_exit`、7 `trend_calm_pullback_grid`、8 `trend_calm_pullback_take_profit`、9 `trend_calm_pullback`、10 `trend_profit_ladder`，全部不变。完整现行分数仍以 [benchmark 成绩表](../benchmark.md#新增策略与自动评测) 为准。

## 复跑与缓存契约

```powershell
. D:/Users/Desktop/home/code/.venv/Scripts/Activate.ps1
python scripts/benchmark_long_history_study.py build --release .runtime/benchmark/pools-v5-20261031 --history .runtime/benchmark/history-baostock-20261102 --calendar .runtime/benchmark/public-collection-20261029/raw/calendar.json --out .runtime/benchmark/pools-long-history-study-20261102
python scripts/benchmark_long_history_study.py scan --release .runtime/benchmark/pools-long-history-study-20261102 --out .runtime/benchmark/long-signals-study-20261102
npm run benchmark -- bench-run --release .runtime/benchmark/pools-long-history-study-20261102 --strategies .runtime/benchmark/long-signals-study-20261102/candidates --out .runtime/benchmark/long-signals-study-20261102/official-runs
```

`build` 和 `scan` 都拒绝覆盖已有输出，复跑时选新版本目录。`scan` 的 35 个参数配置与生成器已进 Git，临时策略目录不进 Git。运行仍需要独立市场缓存，不是只取代码就能无数据评分：

- `history/request.json`：包含 `source=https://www.baostock.com/`、`adjustflag=1`、`release_manifest_sha256` 与 `requests`；每项为 `symbol/start/end/fields/adjustflag`。
- `history/raw/<symbol>.json`：含相同 `request`、`status=completed`、`rows`（供应商原始字符串字段）。有修复时优先使用 `repaired/<symbol>.json`，保留原失败档案；合并归档仍绑定原请求。
- `calendar`：独立、升序不重复的交易日期 JSON 列表，允许 YYYYMMDD 或 YYYY-MM-DD。不能用工作日推测中国实际休市日。
- 输出 `long-history-study.json` 保存 172 个实际使用源文件哈希、原 manifest 和日历哈希、共同字段比较数与特征覆盖。市场原始数据不提交、不上传。

正式复核五个最优配置后，带 `--force` 再执行一次：5 个仓位文件、5 个成绩文件逐字节一致。扫描与正式总分最大差 `8.88e-16`。19 个已有策略在研究输入上重跑，其仓位和完整成绩文件都与原 v5 逐字节一致，包括所有基线与对照分数。另用相同缓存重建到 `pools-long-history-study-20261102-replay`，全部文件与 manifest 哈希一致。

## 验证与限制

- `npm run test:benchmark`：108 个测试通过（含新增 7 个边界/隔离测试）；架构检查 7 个测试通过；`git diff --check` 通过。
- 完整 `npm test` 初次因 PATH 无 Bash 无法启动；补上本机已有 Git Bash 后重跑，架构、QA 评分、benchmark、部署测试、前端构建/格式及 266 个前端测试、Rust fmt/clippy/build 通过。
- 完整入口随后在创建一次性 PostgreSQL 集群时失败：本机没有发现 `pg_config`，也没有配置 `MARIO_TEST_PG_BIN`。因此 Rust 数据库测试、HTTP/Web/Codex 桥接及后续财务验收未执行，不能报告完整门禁通过；未连接开发或线上数据库。

下一轮继续验证三年周波动的长动量标准分、真实前月组合方差，并推进按披露时间可得的财务/净派息数据。持续研究目标仍在进行中。
