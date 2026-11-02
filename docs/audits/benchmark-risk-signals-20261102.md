## Material Passport

- Origin Skill: academic-research-suite / experiment-agent
- Origin Mode: run
- Origin Date: 2026-11-02
- Verification Status: VERIFIED（研究输入、适配代码、统一评分、复跑及 v6 发布/下载复核）
- Version Label: external_strategies_batch_3_risk

# 第三批：三年周波动和组合方差

本批比较 4 类规则、45 组配置。`trend_basket_risk` 的本地正式评分为 **3.5163214312**，比现行最高 `trend_core_satellite` 的 3.4959143225 高 **0.0204071087**。这只是在反复研究的 148 期上的小幅领先：按年份聚类的配对 95% 区间为 **[−0.0283, 0.0979]**，最差单期没有改善，不构成稳定增益证据。

该规则需要新增月内历史收益。用户明确授权“发布 v6 并更新正式策略记录”后，已发布不可变 v6 标签、从远端下载到全新目录并校验，再将 `trend_basket_risk` 登记到正式策略目录，重跑全部20项并更新[正式成绩表](../benchmark.md#新增策略与自动评测)。部署读取标签改为 v6；本次没有执行网站部署。

## 设计与原文差异

继续沿用[离线研究边界](../architecture.md#离线-benchmark-研究工具)。`scripts/benchmark_risk_study.py` 只读取上批已核对的 Baostock 后复权缓存，复用长历史工具的构建与扫描流程，不访问网络、账号数据库或线上系统。策略隔离子进程只接收截至决策日的输入；研究者在主进程用结果标签比较配置。

- [MSCI 2025-04 方法 §2.2–2.4](https://www.msci.com/indexes/documents/methodology/2_MSCI_Momentum_Indexes_Methodology_20250417.pdf) 用跳过最近一月的 6/12 个月收益，分别除以三年周收益波动，再做标准分合成。此处使用真实长回看，12 月信号缺失时只用 6 月信号，合成后重新标准化、正分排序、上限 3 的得分映射。比较前 1/3/5/10，以及纯得分/得分乘年度选样日市值，共 8 组。没有无风险利率、调仓日母指数权重、原市场范围及半年度缓冲/权重上限，不是 MSCI 指数复现。
- [AQR Time Series Momentum](https://www.aqr.com/Insights/Research/Journal-Article/Time-Series-Momentum) 的 12 个月方向迁移到池内股票。已知 12 月信号且三年周波动为正者先按逆波动分配全池预算，仅自身过去收益为正者投资，其他预算留现金。固定 1 组；没有无风险收益、做空、期货和跨资产目标风险。
- 长期低风险/动量是本地组合试验：按三年周波动取已知成员前一半，再在 63 日动量为正者中取动量前 1/3/5/10。没有净派息字段，不冒充 Robeco 的完整 Conservative Formula。
- [Moreira–Muir §I.B](https://amoreira2.github.io/alan-moreira.github.io/VolPortfolios_published.pdf) 按前月组合已实现方差缩放下一月风险。本批改为**当前选股和固定目标权重构造的历史日度再平衡篮子**：用决策月的日线估计篮子方差，再控制下一季度的预算。它保留真实协动关系，但不是上月实际持有策略的收益序列；也没有重放过去止盈/网格或原文因子多空组合，必须明确这一差别。

组合方差家族比较两种基础组合（行业分散回调网格、核心/卫星）、目标日波动 0.8/1.2/1.6/2.0/2.4/2.8/3.2/4.0%、缩放幂次 1/2，共 32 组。按全段总分只保留最优：核心/卫星、4.0%、幂次 2。所有参数比较都是样本内，2018–2025 不独立。

## 时点和缺失口径

研究输入 `.runtime/benchmark/pools-risk-study-20261102/` 含上批长信号和两个新字段：

- `volatility_3y_weekly`：从决策日前三个自然年起，在固定周五网格取截至该周五的最后一个市场交易日收盘，以周收益总体标准差乘 `sqrt(52)` 年化。决策后的周五不用；整周市场休市可沿用日历确认的上一个市场收盘。任一股票周端点缺失或停牌则 null，不缩短回看、不把未知停牌价格前填为周末价。2,960 条股票决策中只有 **1,221** 条完整，其他预算按各规则留现金；低覆盖是本批结果的重要限制，不能据此否定原方法。
- `daily_returns_1m`：只含决策月截至决策日的日期/每日后复权收益。已知停牌且已有估值时记 0，复牌与此前有成交收盘比较；未知缺行记 null，并断开下一次收益的基准，不能把跨未知缺口的多日涨幅当作单日收益。全池共 2,960 个向量，其中 **3** 个不完整，均保留记录与问题码。

候选篮子日收益为 `sum(w_i / sum(w) * r_i,d)`，日度目标权重固定，因此包含个股协方差。总体方差为这些篮子日收益的离均差平方均值，**不是个股标准差的平均值**。最优预算 `min(1,0.04² / variance)`；方差为 0 时预算 1，任一入选股票的必要日收益未知或日期集合不齐时留现金。其余现金不重新分配，不加杠杆。核心/卫星选股和红利止盈、科技网格规则完整保留。

最小发布包 `.runtime/benchmark/pool-scoring-v6/` 只在 v5 输入上新增 `stock.long_history.daily_returns_1m` 与 `issues`，不携带另外三类失败规则的特征。`config.input_features=monthly-risk-v1`，构建日期固定为 2026-11-02。148 期、股票名单、原字段逐项不变，标签逐字节不变；输入文件 4,027,286 字节。manifest SHA-256：`3ed8080387e7a7f1174198357b2818fed36085be91c47940563b0eb23d20fe55`。

## 统一评分

| 家族最优 | 配置 | 总分 | 平均收益得分 | 胜率 | 2006–2017 总分 | 2018–2025 总分 |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| 组合方差预算 | 核心/卫星、4%日风险、平方缩放 | 3.5163 | 1.6920 | 73.0% | 3.3641 | 3.7161 |
| MSCI 风格长动量 | 前5、得分×选样日市值 | 2.0202 | 0.8378 | 47.3% | 1.4444 | 2.7760 |
| 长周波动低风险＋动量 | 前5 | 1.6877 | 0.8262 | 34.5% | 1.5747 | 1.8360 |
| 12 月时序动量长仓 | 固定规则 | 1.4373 | 0.4407 | 39.9% | 1.2575 | 1.6734 |

四个最优原型均经 `bench-run` 的静态检查、隔离子进程、8 位仓位向下截断及现有交易/成本引擎评分。扫描与正式总分最大差 `4.44e-16`。非胜出候选只在研究输出，不安装正式目录。

## 领先的来源与不确定性

与未减仓的核心/卫星组合相比，只有五期仓位改变，胜率仍为 108/148，最差一期仍为 −13.6105：

| 决策点 | 新敞口 | 收益得分变化 |
| --- | ---: | ---: |
| 红利 2015-06-30 | 70.07% | +2.4065 |
| 红利 2021-09-30 | 86.80% | −0.2383 |
| 科技 2015-06-30 | 74.77% | +2.5547 |
| 科技 2020-03-31 | 96.36% | −0.4031 |
| 科技 2024-09-30 | 77.96% | −1.2995 |

增益集中于 2015 年两池同时减仓。对逐期配对总分差按自然年重采样 10,000 次（种子 20261102，抽中的年份保留该年的两池全部期）得到上述包含 0 的区间。这个区间也没有校正全段选参及多轮搜索的选择偏差，不能据此宣称统计显著。

## 正式排名与策略身份

评审目录 `.runtime/benchmark/risk-study-20261102/review-strategies/` 保留 19 个现有实现，另加最优 `trend_basket_risk`，共 20 项。旧 `trend_risk_budget` 的行业分散选股、63 日个股平均波动预算，与新策略的核心/卫星构建和月内组合协方差不同，属于不同机制；不能按“仅参数不同”删掉旧规则。32 个新预算配置只保留一个，不能拆成目标值或幂次变体占榜。

| v6 名次 | 策略 | 总分 | 相对原 v5 |
| ---: | --- | ---: | --- |
| 1 | `trend_basket_risk` | 3.5163 | 新入榜 |
| 2 | `trend_core_satellite` | 3.4959 | 1→2 |
| 3 | `trend_single` | 3.4787 | 2→3 |
| 4 | `calm_mid_momentum_sector` | 3.3855 | 3→4 |
| 5 | `trend_risk_budget` | 3.3226 | 4→5 |
| 6 | `calm_mid_momentum` | 3.2824 | 5→6 |
| 7 | `trend_state_exit` | 3.1998 | 6→7 |
| 8 | `trend_calm_pullback_grid` | 3.1463 | 7→8 |
| 9 | `trend_calm_pullback_take_profit` | 3.0818 | 8→9 |
| 10 | `trend_calm_pullback` | 3.0527 | 9→10 |

`trend_profit_ladder` 10→11，3.0039；作为独立执行机制保留。19 个现有策略在最小 v6 上的仓位和完整成绩文件都与 v5 逐字节一致。新策略的最小 v6 仓位和成绩与完整研究输入逐字节一致，说明未依赖不准备发布的长信号。

最终 20 项评审再次带 `--force` 执行，全部仓位与成绩文件逐字节一致；四个研究原型也重复一致。按固定构建日期从同一缓存重建最小 v6，manifest 哈希相同。

## 复跑与发布准备

```powershell
. D:/Users/Desktop/home/code/.venv/Scripts/Activate.ps1
python scripts/benchmark_risk_study.py build --release .runtime/benchmark/pools-v5-20261031 --history .runtime/benchmark/history-baostock-20261102 --calendar .runtime/benchmark/public-collection-20261029/raw/calendar.json --out .runtime/benchmark/pools-risk-study-20261102
python scripts/benchmark_risk_study.py scan --release .runtime/benchmark/pools-risk-study-20261102 --out .runtime/benchmark/risk-study-20261102
npm run benchmark -- bench-run --release .runtime/benchmark/pools-risk-study-20261102 --strategies .runtime/benchmark/risk-study-20261102/candidates --out .runtime/benchmark/risk-study-20261102/official-runs
python scripts/benchmark_risk_study.py build --minimal --built-on 2026-11-02 --release .runtime/benchmark/pools-v5-20261031 --history .runtime/benchmark/history-baostock-20261102 --calendar .runtime/benchmark/public-collection-20261029/raw/calendar.json --out .runtime/benchmark/pool-scoring-v6
```

构建/扫描不可覆盖，已有目录复跑要改用新目录。缓存契约沿用[第二批记录](benchmark-long-signals-20261102.md#复跑与缓存契约)。程序配置及参数已进入代码；评审目录的新策略来自 `source_for("basket_risk_overlay", {"base":"trend_core_satellite","target":"0.04","power":2})`，其余原有目录完整复制。不能仅凭代码在缺市场缓存时重建输入。

本机通过 `bench-serve --release .runtime/benchmark/pool-scoring-v6 --out .runtime/benchmark/risk-study-20261102/review-runs --port 8766` 提供评审看板，页面与数据接口均实测 HTTP 200。它是本地评审入口，不是已部署网站。

用户授权后，向原有两个私有仓库创建 `pool-scoring-v6` 标签：输入仓库提交 `698ab24cc388e980f0cd8bbb25e45941760ee0f9`，标签仓库提交 `e55fa56f97c5c060981fc073d9305e01bcb8db9f`。发布后重新查询，两个仓库仍为私有，v6 标签指向上述提交，v5 标签的两个提交不变；标签仓库恰好20项策略成绩，无参数重复残留。未上传原始长日线。

使用 `hub-fetch --tag pool-scoring-v6 --out .runtime/benchmark/hf-verified-v6-20261102` 下载到此前不存在的目录，返回 `status=downloaded` 并通过43个文件的校验；manifest 与本地批准版本一致。随后由正式 `benchmark/strategies/` 评分到 `.runtime/benchmark/bench/pool-scoring-v6/`，20项均成功，仓位和完整成绩与批准的评审版本一致。`deploy.sh` 默认标签及部署文档已改为 v6，本次不执行部署；线上页面要在服务器按既有流程部署后更新。

数据卡准确说明新字段截止时点与 null 语义。正式新策略只保存一组参数，代码和说明已提交；原v5数据及旧提交仅作为历史复现入口，不在新策略中保留降级兼容分支。

验证：正式准入后115个 benchmark 测试、架构检查7项、266个前端测试、前端构建/格式、部署测试、Rust fmt/clippy/build通过。新增测试确认注册策略要求v6字段而不会静默退回旧逻辑。完整 `npm test` 在一次性 PostgreSQL 集群初始化前因找不到 PostgreSQL 工具失败，数据库、HTTP/Web/桥接与后续财务验收未执行；未使用开发或线上数据库。另已复跑上一批35组，结果JSON完全相同，确认共享扫描器对旧无执行规则策略没有改变成绩。

持续研究仍在进行；v6 发布、正式登记和本地看板已完成，不将推送代码宣称为网站已部署。
