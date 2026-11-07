# Benchmark 原始数据与衍生指标

数据按来源事实、指标计算、策略依赖组织。快照名称、日期和文件哈希用于复现；不再以 `input_features`、v8/v9 或指定的前一版本决定输入能否被消费。来源格式由适配器处理，新增字段不要求旧策略升级。

## 职责与数据流

```text
公开来源 / 已有采集缓存 / 人工核验财报原件
    ↓ 来源适配、证券身份、日历、价格口径、可得时间、文件哈希
原始研究归档（raw_data）
    ↓ 截止决策时点的记录
确定性指标计算（*_indicators）
    ↓ 按输出字段组合，保留扩展字段及缺失原因
时点输入与指标来源记录（feature_pipeline）
    ↓ 生产评测按策略 REQUIRES 从零计算；研究入口可显式补算
隔离策略执行（strategy_runner）
    ↓ 仓位
评分器 ← 独立未来标签
```

采集层继续使用现有 `collect.py`、`daily_prices.py`、`public_sources.py` 等适配器；不为新增指标重新采集已有事实。`raw_data.py` 读取两类既有 Baostock 缓存，核验证券身份、重叠记录、完成状态和独立交易日历，并可导入为独立原始归档。财报原件的证据核验也归该模块。原始目录和研究记录不覆盖、不删除。

指标函数只接收值，不读文件、网络、账号或标签。`feature_pipeline.py` 是原始读取与指标计算的组合层，`strategy_runner.py` 只调用组合层及输入检查，不导入来源 SDK。状态仍归 `.runtime/benchmark/` 的研究文件；不进入 PostgreSQL 账号事务。

## 持续评测的唯一生产路径

workflow 先执行 `benchmark.live prepare`，再执行 `benchmark.live evaluate`。`prepare` 由 `hub` 固定输入仓库、标签仓库及原始行情仓库的提交，下载评分计划的范围、年度选样事实和独立未来标签；明确不下载 `inputs.jsonl`。原始行情取同源 Baostock 双口径归档，逐文件核对原清单，局部下载包另建清单并保留完整来源清单。

`evaluation_data.py` 从独立市场日历生成季度末决策日，从年度快照取证券与选样原始收盘/已公告股数；市值重新相乘，不读取候选记录中已经计算的市值。证券当前简称不进入输入。交易状态与决策日收盘来自未复权原始日线，行情指标来自同源后复权历史。输入起初没有 `momentum_63`、`long_history`、`fugui` 等指标；按适用策略依赖的并集计算，检查完成后才把标签原样复制进评分目录。计划中出现预计算 `inputs.jsonl` 会被拒绝，无法从原始来源计算时不会借用旧切片字段。

`BENCHMARK_PLAN_REVISION` 默认 `pool-scoring-v8`，仅定位原始选样事实、评分范围及标签；`BENCHMARK_RAW_REVISION` 默认 `baostock-daily-20261102`，定位同源原始行情。默认值来自 [`benchmark-live.yml`](../.github/workflows/benchmark-live.yml) 和 [`live.py`](../benchmark/live.py)，是可配置的来源身份，新增指标不要求改这些出处。旧 `BENCHMARK_SCORING_TAG` 已删除。新输入标识为 `raw-<内容指纹>`，看板记录计划与原始来源的固定提交、清单哈希和 `input_origin=raw_facts`；历史成绩中的具体 `raw-` 标识仅对应那次来源和计算实现，不承诺等于此刻线上快照。

`bench-run --raw` 和 `features-build` 仍是显式本地研究入口：可以为已有切片补算；它们不再是持续评测的生产路径。

只读复现生产成绩不需要 HF 写入权限。在仓库根目录使用相同 Python 环境和一个新的工作目录：

```bash
python -m benchmark.live prepare --workspace .runtime/benchmark/raw-replay
python -c "from pathlib import Path; from benchmark.evaluation_data import rebuild_evaluation; from benchmark.strategy_runner import discover; root=Path('.runtime/benchmark/raw-replay'); sources=[(folder/'strategy.py').read_text(encoding='utf-8') for folder in discover(Path('benchmark/strategies')).values()]; rebuild_evaluation(root/'plan',root/'raw',root/'derived',sources)"
npm run benchmark -- bench-run --release .runtime/benchmark/raw-replay/derived --out .runtime/benchmark/raw-replay/runs
```

这条复现路径与 workflow 调用同一构建器和评分器，不读取旧预计算输入，也不上传看板。

## 数据与指标分组

| 原始事实 | 衍生结果 | 计算模块 |
| --- | --- | --- |
| 独立市场日历、日线收盘、交易状态及复权口径 | 20/63 日收益、63 日波动 | `price_indicators.py` |
| 同源长期日线、最高价及日历 | 长窗口动量、10 月均线偏离、12 月高点比例、信息离散度 | `price_indicators.py` |
| 同源日线、交易状态及日历 | 三年周波动、决策月每日收益、90 日回归与 R² | `price_indicators.py` |
| 同源日线及日历 | 日/周/月 BOLL、月 MACD、月线低位和首次回踩条件 | `technical_indicators.py` |
| 报告公告记录、个股及市场价格 | 首次定期报告事件的 CAR3 | `announcement_indicators.py` |
| 人工核验累计利润、普通股恒定股数证据、披露时间 | 单季利润/EPS、同比创新及 SUE | `financial_indicators.py` |
| 历史成分、行业生效区间、选样日收盘及已公告股数 | 股票池、选样日市值、行业属性 | 既有股票池/市值模块；不从技术指标推断 |

行情指标的完整可组合目录由 `npm run benchmark -- indicators` 输出。目前公告、SUE 仍由各自研究工具装配证据输入，再调用共享纯计算模块；尚未接入正式行情指标组合入口，不能把未核验财务数据当成已就绪指标。

## 扩展与消费规则

每个 `Indicator` 独立声明输出路径、值类型、单位、原始依赖、价格口径和计算器。组合层保持现有 `stock.long_history`、`stock.fugui` 等语义路径，可以加入新命名空间和字段；额外字段不会触发拒绝。独立的 `monthly_macd_negative`、DIF、DEA、柱值可以从原始行情计算，无需先升级整包输入。

策略通过源码中的静态字典声明实际消费的能力，例如：

```python
REQUIRES = {
    "stocks.*.symbol": "text",
    "stocks.*.fugui.monthly_macd_negative": "boolean?",
    "stocks.*.long_history.daily_returns_1m": "returns",
}
```

`*` 表示样本中的证券集合；类型后的 `?` 允许明确的 `null`，不允许缺少字段。`returns` 是按日期映射的十进制日收益，允许某日为 `null`。只适用于特定池的策略可声明 `REQUIRES_POOLS = ("dividend",)`；池标识本身始终校验，池外不要求其专属指标。自定义研究策略未声明时保留自由的 `decide(sample)` 接口；正式策略均有显式声明。

执行器在启动子进程之前检查所有适用样本。提供 `--raw` 时只补算不存在的所需指标；已经明确为 `null` 的值不会被另一个来源悄悄覆盖。没有计算定义、没有证券原始记录、价格口径不一致或字段类型错误时分别报告具体问题及样本/证券/字段。其他可运行策略仍执行，但正式看板只有全部策略成功后才替换上次成功成绩。

同一输出名称必须保持同一含义、单位与窗口。改变含义应使用新的指标名称或显式重建新快照，不能把旧结果重贴成新公式。计算指纹、来源哈希和缺失语义是必要的审计信息，不作为整包版本兼容开关。

## 原始归档和构建命令

以下命令只在本地整理数据，不访问采集服务或上传 HF：

```bash
# 原始归档只需导入一次，不绑定某个评分切片的 manifest。
npm run benchmark -- raw-import \
  --history <既有独立历史缓存> --calendar <独立交易日历.json> \
  --out <新的原始归档>

# 另一种现有来源：股票池构建缓存。release 在此仅提供证券范围。
npm run benchmark -- raw-import \
  --universe-root <股票池缓存> --release <任意评分切片> \
  --out <新的原始归档>

# 可重复 --field；不指定则补全目录中全部行情指标。
npm run benchmark -- features-build \
  --release <评分切片> --raw <原始归档> \
  --field fugui.monthly_macd_negative \
  --field long_history.daily_returns_1m --out <新的派生切片>

# 也可以只在评测时按各策略声明补算，不修改原切片。
npm run benchmark -- bench-run \
  --release <评分切片> --raw <原始归档> --out <新的评测目录>
```

原始归档包含 `bars/<证券>.jsonl`、`calendar.json`、`sources.json` 和逐文件哈希清单。供应商差异只在 `RawData.at()` 接口前适配；不在策略中增加某供应商或某快照分支。当前两个导入适配器均为 Baostock 后复权历史，新的供应商应显式增加适配和对账能力。每日 Yahoo 行情仍为未对账来源，不自动转换成 Baostock 复权尺度，不填造交易状态，也不用于补齐正式历史信号。

`features-build` 先完成计算，再原样复制未来标签。新目录保留原输入、未知扩展字段、原有命名空间及指标构建记录，新增 `features.json` 记录定义、代码指纹、来源与计算覆盖。构建不会改写原切片；错误由临时目录提交机制清理，恢复时修正来源后使用新输出目录。构建日期可通过 `--built-on` 显式提供；原 `input_features` 等整包能力标签不会被新组合包沿用。

## 时点、缺失与复现

原始读取只提供决策日及以前、且 `available_at` 不晚于截止时间的记录。既有供应商缓存没有记录可得时间时使用交易日 18:00 的研究约定，并明确保留 `pit_verified=false`；这一约定不证明供应商复权数据或日历具有历史时点保证。

停牌延续估值、未知缺行、历史不足、公告窗口尚未结束和财务证据未核验继续沿用各指标的问题码。缺失不能转换成 0、`False` 或通过选股条件。使用其他源补算前还会比较已有非空 20/63 日收益，拒绝共同字段不一致的数据；新增字段不会覆盖已有 `null` 或更改已有信号含义。

评分仍固定数据快照、标签及 Git 提交；固定标签是实验样本身份，不是策略兼容版本。`run.json` 记录实际输入哈希及按需补算信息，缓存指纹包含策略、切片、计算实现与原始数据内容。来源或计算改变后不能沿用旧成绩。持续评测先跑全部 benchmark 测试，再对下载后的真实输入执行依赖检查；失败保留上次成功结果，不用跳过策略掩盖缺字段。

迁移不删除历史切片。已有预计算字段只供显式历史复现或本地研究；持续评测始终从原始事实构建所需指标，采集来源缺口仍如实保留。实现分别见 [`evaluation_data.py`](../benchmark/evaluation_data.py)、[`feature_pipeline.py`](../benchmark/feature_pipeline.py) 和 [`input_contract.py`](../benchmark/input_contract.py)。架构静态检查限制依赖方向；[`test_evaluation_data.py`](../benchmark/tests/test_evaluation_data.py) 覆盖拒绝预计算输入、原始缺失不回退和标签不变，[`test_feature_pipeline.py`](../benchmark/tests/test_feature_pipeline.py) 覆盖缺字段与 null 区别、时点隔离、来源口径及缓存指纹。这些合成回归不核验真实 HF 当前内容或历史来源的时点真实性。
