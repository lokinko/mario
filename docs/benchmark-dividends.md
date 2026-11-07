# 股票分红事实采集

入口 `dividends-collect` 按显式股票清单采集东方财富分红送配原始记录，再按除权除息日期筛选窗口。它是原始事实整理，不执行选股、不计算持有收益或股息率，也不会把当前供应商记录作为历史时点已核验的策略输入。

```bash
npm run benchmark -- dividends-collect \
  --symbols-file <含symbols列表的JSON> \
  --start 2021-11-04 --end 2026-11-04 \
  --out <新的分红归档> --cache-root <分红响应缓存>
```

来源适配在 `public_sources.dividend_page`，使用东方财富 [分红送配](https://data.eastmoney.com/yjfp/) 的 `RPT_SHAREBONUS_DET` 数据接口；只依赖标准库 HTTP，不增加 SDK。采集器归 `dividends.py`，最多两个线程，校验证券代码、总记录数、分页稳定性及重复记录；缓存保存请求、原始响应、获取时间和哈希，输出目录不可覆盖。

## 日期与单位

- 窗口以 `EX_DIVIDEND_DATE`（除权除息日）判定，按该日历年份汇总；不以财报报告期筛选，因此不会漏掉以前年度利润在窗口内实施的分红。除权除息日不等同于银行账户到账日。
- 只把 `ASSIGN_PROGRESS=实施分配` 且除权除息日在窗口内的记录计入实施明细。其余记录仅在方案公告日或最新公告日落在窗口内时单列：已实施但除息日不在窗口内记为 `outside_window`，其余记为 `proposal_or_pending`，均不计入窗口内已实施现金。
- `PRETAX_BONUS_RMB` 保存为每 10 股、人民币、税前现金；`BONUS_RATIO` 和 `IT_RATIO` 分别为每 10 股送股、转增股数。数值保留十进制字符串，未知保留 null。
- 年度派现合计是原方案每 10 股金额的直接汇总，没有对送转、拆并股和投资者持股变化作复权；不能当作实际账户收入或持有收益。
- 原始记录中的方案公告日、最新公告日、股权登记日与除权除息日分别保存，不把供应商当前记录推断为历史修订已核验；标记 `pit_verified=false`。

## 输出与覆盖状态

| 文件 | 内容 |
| --- | --- |
| `summary.csv`、`summary.jsonl` | 每只股票的覆盖状态、实施/现金分红记录数、逐年每10股派现 |
| `events.csv`、`events.jsonl` | 窗口内全部实施记录，保留日期、金额、送转和方案 |
| `stocks/<证券>.json` | 逐股实施记录、其他方案和来源引用 |
| `raw/<证券>-<页>.json` | 原始响应、获取时间和请求身份 |
| `coverage.json`、`manifest.json` | 窗口、覆盖摘要与逐文件哈希 |

`observed` 表示查到窗口内实施记录；`no_implemented_record_in_window` 表示来源有历史记录，但窗口内没有查到实施记录；`source_no_records` 表示接口明确返回无记录；`source_unavailable` 表示请求、分页或内容校验失败。这些状态不能相互替代，CSV 年份空白不自动解释为公司当年零分红。

逐证券读取失败会记录为 `source_unavailable` 并继续输出其他证券；命令正常完成不保证所有证券均已覆盖，须检查 `coverage.json` 与逐股状态。复用同一缓存目录会沿用已保存响应及原获取时间；需要重新向来源查询时使用新的缓存目录和输出目录，保留旧归档用于追溯。

实现见 [`dividends.py`](../benchmark/dividends.py)，CLI 参数见 [`cli.py`](../benchmark/cli.py)。[`test_dividends.py`](../benchmark/tests/test_dividends.py) 用合成响应覆盖窗口、方案分离、证券身份、空来源及分页缓存，不访问真实来源，也不证明供应商无遗漏。在线买点的已实施历史和分红日历另由[观察规范](buy-watch.md)定义；这里的年度每十股汇总不替代在线 TTM 股息率或个人到账记录。

2026-11-04 的历史采集覆盖当时行情范围的 210 只股票，时间为 2021-11-04 至 2026-11-04；2021/2026 年仅含窗口内部分。数据和覆盖结果见[当次审计](audits/benchmark-raw-workflow-20261104.md)，不表示当前覆盖数量或新的自动扩库任务。
