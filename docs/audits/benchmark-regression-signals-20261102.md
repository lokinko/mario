# 回归趋势与权重倾斜实验（2026-11-02）

本轮3类48组适配中，`trend_regression_tilt` 总分 **3.4852522851**，进入第三名；第一仍为`trend_basket_risk`的3.5163214312。只登记权重倾斜机制的一组最优配置，直接回归排序与回归＋低波排序均未进入Top-10，不新增正式目录。成绩来自同一148期、同一未来标签和统一执行/成本规则，不宣称超过当前第一或得到样本外改善。

## 来源与研究口径

原始规则见[Andreas Clenow的CMT讲义](https://cmtassociation.org/wp-content/uploads/2025/01/Clenow-Using-Momentum.pdf)，物理第15页的Python示例：对价格取log后按时间做OLS，用252日年化指数斜率乘R²。[作者2024年10月文章](https://cmtassociation.org/technically_speaking/technically-speaking-october-2024/)说明90交易日窗口，并把默认仓位按趋势由弱到强乘0.5至2再归一化。讲义通过Web解析核对共21页；本机下载返回403，未获得本地PDF归档，也未把Web解析冒充本地原件哈希或PASS预检。

本轮测试信号与权重规则的季度A股适配：资产为既有科技/红利池，不能复现作者的多资产、月度All Weather组合，也不是完整的周度/ATR股票交易系统。逆63日波动只是本地比较选项，不等同于ATR。所有参数在2006–2025全部148期上比较，早段84期、后段64期仅作诊断，**都不是严格样本外**。

先测严格窗口24组，发现明确停牌也会令信号不可用，再补停牌持平估值敏感性检验；最终48组含两种口径。这是观察覆盖与初次成绩后的研究扩展，不是事前锁定的独立验证。

## 输入与时点

沿用已对账的172只证券、350,956条Baostock后复权日线及独立交易日历；63日动量2,796项、20日收益2,827项与原切片逐项一致。信号构造不读取`outcomes.jsonl`，全部特征算完后才复制标签。

`regression_features`只用截至决策日最近90个**市场交易日**：

- 严格口径：90日均有成交收盘才计算；2,074/2,960项有效。
- 停牌估值口径：明确`tradestatus=0`时沿用此前有成交收盘，复牌后更新；不读取停牌行的所谓新成交价。真正缺行重置估值基准，不向未来取值，也不向过去延长窗口；2,955项有效，5项因完整90日历史不足保留null。
- 指标为`(exp(252×slope)-1)×100×R²`；在40位Decimal上下文中对相对首日价格取log，避免价格单位改变的大截距误差。恒定价格取0信号、R²=0；非有限或非正成交价格拒绝。

完整研究切片保留两种信号与R²及v6月内收益。正式最小v7只在v6上增加`stock.long_history.clenow_mark_90`及相应问题码，不携带回归原始价格、严格口径或R²冗余字段；`config.input_features=monthly-risk-regression-v1`。全部原字段、148期、2,960项证券决策和标签不变。43个包文件（manifest列42项），manifest SHA256为`35a7169824d8b66822a8a60accebb56a8a383cf32763d27f48e446efa634fa36`。

## 比较结果与唯一准入配置

| 机制 | 配置数（含两种口径） | 严格口径最高 | 停牌估值口径最高 | 最终参数 |
| --- | ---: | ---: | ---: | --- |
| 回归直接排序 | 24 | 2.5721 | 2.7991 | 前1、正信号、R²无额外门槛、等权 |
| 回归＋低波排名 | 12 | 2.6070 | 2.6854 | 前3、正信号、R²无额外门槛 |
| 基础组合权重倾斜 | 12 | 2.5821 | **3.4853** | 核心/卫星、停牌估值、开启4%方差预算 |

直接排序搜索前1/3/5、R²门槛0/0.5、等权/逆波动；低波版把回归降序名次与波动升序名次相加，搜索前1/3/5、R²门槛0/0.5。倾斜版基础组合为全池等权、大市值前5等权、核心卫星，风险预算开/关。每一逻辑只保留最优配置；全部对比都含完整148期，未提交的期按现金计分。

准入版先用现有25%核心/75%卫星规则选股，再对已选且信号已知的原预算乘趋势名次因子0.5至2，并列取平均排名；一只持仓时不作人为倾斜，未知预算留现金。按倾斜后的风险资产权重归一化合成决策月日收益，重新估计总体方差，乘`min(1,0.04²/variance)`敞口。保留红利10%止盈及低波科技8%两档、初始一档网格。它根据独立90日价格路径逐期改变相对权重，区别于只改固定核心比例的参数变体。

最终平均收益得分 **1.6778198527**，胜率 **107/148=72.3%**，早/后段总分 **3.4025564172/3.5937906118**。相对第一的配对差为−0.0310691461；以年份聚类、固定种子20261102抽样20,000次的95%区间为[−0.1632678831,0.1118150878]，包含0。只作当前切片描述，不作显著改善或实盘收益承诺。

3个最优候选的正式隔离执行与扫描成绩完全一致。20个已有策略在完整研究输入和最小v7上的仓位、完整成绩均与v6逐字节相同。新策略在完整输入、最小输入、清理重复导入后的正式目录以及21项强制复跑中，仓位和成绩逐字节一致。按固定日期重建最小包亦须校验manifest相同。

## 当前Top-10与变化

| v7名次 | 策略 | 总分 | 相对v6 |
| ---: | --- | ---: | --- |
| 1 | trend_basket_risk | 3.5163 | 不变 |
| 2 | trend_core_satellite | 3.4959 | 不变 |
| 3 | trend_regression_tilt | 3.4853 | 新增 |
| 4 | trend_single | 3.4787 | 3→4 |
| 5 | calm_mid_momentum_sector | 3.3855 | 4→5 |
| 6 | trend_risk_budget | 3.3226 | 5→6 |
| 7 | calm_mid_momentum | 3.2824 | 6→7 |
| 8 | trend_state_exit | 3.1998 | 7→8 |
| 9 | trend_calm_pullback_grid | 3.1463 | 8→9 |
| 10 | trend_calm_pullback_take_profit | 3.0818 | 9→10 |

`trend_calm_pullback`由10→11，作为独立选股机制保留。当前共21项：17个机制和4个不占名次的基线。

## 复跑与缓存

```powershell
. D:/Users/Desktop/home/code/.venv/Scripts/Activate.ps1
python scripts/benchmark_regression_study.py build --release .runtime/benchmark/pools-v5-20261031 --history .runtime/benchmark/history-baostock-20261102 --calendar .runtime/benchmark/public-collection-20261029/raw/calendar.json --out .runtime/benchmark/pools-regression-mark-study-20261102
python scripts/benchmark_regression_study.py scan --release .runtime/benchmark/pools-regression-mark-study-20261102 --out .runtime/benchmark/regression-study-20261102/scanned-both
python scripts/benchmark_regression_study.py build --minimal --built-on 2026-11-02 --release .runtime/benchmark/pools-v5-20261031 --history .runtime/benchmark/history-baostock-20261102 --calendar .runtime/benchmark/public-collection-20261029/raw/calendar.json --out .runtime/benchmark/pool-scoring-v7
npm run benchmark -- bench-run --release .runtime/benchmark/pool-scoring-v7
```

构建与扫描拒绝覆盖，复跑使用新目录。独立日线、请求快照和日历契约沿用[长历史实验](benchmark-long-signals-20261102.md#复跑与缓存契约)，缺少缓存不能声称仅凭代码重建市场档案。研究缓存与原始价格不入Git、不上传；策略代码、参数生成器、正式成绩表及本审计入Git。发布、下载和最终验证记录在本地`regression-study-20261102/`，网站部署不在本次范围内。

## 发布与交付验证

用户确认成绩后提交的要求下，已向原有两个私有数据集发布不可变`pool-scoring-v7`标签：输入提交`742fb368617a5486b46c9c5ec36c3099b562640c`，标签提交`44328f826492728458fa528e24f2a36bbabe7ef6`。发布后确认两个仓库仍私有、v5/v6的四个标签提交不变，v7标签仓库恰好21项成绩。未上传原始价格或讲义。

`hub-fetch`下载到此前不存在的`hf-verified-v7-20261102`，返回`status=downloaded`；43个文件与批准的本地v7逐字节相同。从新下载包用正式策略目录重跑21项，所有仓位和完整成绩均与本地正式评分、评审及强制复跑一致；按固定日期重建的manifest也相同。正式成绩表17个排名行逐项核对总分、平均收益得分、胜率与分段总分。部署默认标签改为v7，没有执行网站部署。

本轮最终`npm test`通过141个benchmark测试、架构7项、部署检查和前端构建，随后在Prettier阶段被并行任务尚未提交的`client/src/features/settings/CodexBridge.tsx`及`CodexBridge.test.tsx`格式问题阻断，未修改或夹带这些文件；该次后续前端测试和Rust/数据库集成阶段未执行。此前研究阶段的全量检查曾通过266个前端测试、前端构建/格式及Rust fmt/clippy/build，停在缺少PostgreSQL本机工具处；该结果对应当时的工作区，不能替代最新并行改动的验收。没有使用开发或线上数据库。
