## Material Passport

- Origin Skill: academic-research-suite / experiment-agent
- Origin Mode: run
- Origin Date: 2026-11-02
- Verification Status: VERIFIED（公告反应适配、时点输入、统一评分和重复执行；未复现原文组合）
- Version Label: external_strategies_batch_4_report_car3

# 第四批：定期报告公告反应与盈利动量

本批取得作者原稿，采集172只历史证券的公告目录及独立市场全收益行情，比较3类公告反应适配、30组配置。最优“CAR3＋低波排序”为 **2.9939**，低于当前v6第十名 **3.0527**，不新增正式策略。当前第一仍为 `trend_basket_risk` 的 **3.5163**，Top-10 不变。

## 原文及未复现部分

来源为 [Novy-Marx 作者站工作稿](https://mysimon.rochester.edu/novy-marx/research/FMFM.pdf)《Fundamentally, momentum is fundamental momentum》，另有 [NBER w20984（2015）题录](https://www.nber.org/papers/w20984)。NBER页面和原PDF路径返回403，后取得作者站文件248,702字节，SHA-256 `02989a1267b116da54b6c6022fbb15c0e7e7edf7e312fa33a9d4f1635f272b1f`。作者文件无封面日期，不假定它与NBER文件逐字节一致。

核对§2.1、§2.3和§3：SUE为最新单季EPS同比变化相对近期盈利创新标准差，八次公告窗口至少六次观察；CAR3为最近盈利公告前后一日共三日的市场调整收益。原文采用月度、大小市值分层、价值加权多空组合；控制价格动量的主要实验使用SUE，并非此处的CAR3分层扩展。

本批只测试**首次非摘要定期报告日期代理**上的CAR3，保守处理无时分秒的公告日。它没有覆盖更早的盈利快报或预告，也没有原文的EPS、异常项、大小市值分层和做空；季度20股长仓、交易成本及90天评分也不同。完整SUE需要按历史披露版本还原单季EPS与同比差分，不能用累计净利润同比增速或最新重述EPS代替，仍待后续数据工作。

PDF结构预检三个页数信号均为45、无修复警告，判定PASS。初次pypdf文本提取提示缺少字体解析依赖，最终用已安装PyMuPDF读取正文，未根据不完整编码猜公式。预检输出和原文仅留本地，不提交或上传论文全文。

## 数据和异常审计

沿用[离线研究边界](../architecture.md#离线-benchmark-研究工具)。新增 `scripts/benchmark_earnings_study.py` 消费独立源缓存，复用长历史构建器、月内收益计算和策略扫描器；不读取账号数据，不从结果标签回填特征。

- 原切片172只证券全部保留，包含退市股。按各股入池年份范围补采巨潮公告，172项均返回并通过页数/ID去重检查，共 **7,183** 条目录记录，归并为 **4,115** 个首次非摘要定期报告事件；未经逐篇PDF核验，`version_verified=false` 保留。
- 只按每条公告的证券代码归属。组织标识查询 `601360` 返回两条旧代码 `601313` 的2017年报记录，严格排除并在配置记录 `foreign_security_in_org_query`、证券代码和公告ID，不因公司组织标识相同而混入新证券事件。
- 同一报告期取最早的原始报告；标题标明修订、更正、更新、补充的记录不构造新事件，摘要不纳入。没有把多年后的修订日期当作旧年首次公告。
- 市场对照另从中证官网取得H00300全收益 **5,103** 行，与既有独立交易日历对齐后5,101个交易日完整。源中 `2005-01-01`、`2018-06-18` 两个非交易日观察保留原档，计算只使用交易日历，未当成额外交易日。
- 个股使用前批已对账的Baostock后复权日线；股票和市场均含分红收益，不把个股全收益与价格指数混算。

原始公告、请求快照、索引及诊断位于 `.runtime/benchmark/earnings-study-20261102/`。本批资料及扩充输入未上传到Hugging Face，没有改写已获授权发布的v6标签。

## 信号与截止时点

巨潮日期字段通常为00:00，沿用来源适配器的保守规则：公告日23:59:59后才可用。决策时间为季度末18:00，同日公告不可用。有效事件日取公告日期之后第一个市场交易日，CAR窗口为它之前一日、当日和后一日；为计算三个日收益，还需要窗口前一个收盘。

`report_car3 = sum(r_stock,d − r_H00300,d)`，明确使用三个日超额收益的算术和。只使用最新已知的原始报告事件；若该事件三日窗口尚未结束，不借用未来价格，也不偷偷退回较旧报告。事件超过183自然日、窗口缺日或停牌则null。183日为固定研究适配约定，未扫描其长度。该时点对齐不同于原文直接使用RDQ，不声称精确复制原文公告收益。

新增 `long_history.report_car3`、只含已知日期/原件引用的 `report_car3_event` 和问题码。2,960个股票决策中 **2,746** 个信号可用；不可用214项仍保留：

| 问题码 | 条数 |
| --- | ---: |
| `announcement_window_not_yet_observed` | 54 |
| `missing_or_suspended_announcement_window` | 151 |
| `no_prior_original_report` | 4 |
| `stale_report_event` | 5 |

研究目录 `.runtime/benchmark/pools-earnings-study-20261102/` 保留全部148期。以不可变v5及独立历史为构建来源，同时重建v6月内收益；逐项验证v6已有字段、月内收益及原有问题码含义不变，结果标签逐字节相同。172个公告源哈希及市场源哈希写入配置，加入发布目录manifest。原件标题和当前简称不进入策略输入。

## 比较规则与结果

参数在全部148期总分上选择，早段84期、后段64期只用于诊断，**不是样本外**。只按事件信号和当时可得字段选股，无证券/年月专用分支；所有策略持有至评分窗口结束，没有附加止盈或网格。

| 适配逻辑 | 搜索空间与最优 | 总分 | 平均收益得分 | 胜率 | 2006–2017 总分 | 2018–2025 总分 |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| CAR3＋低波排名 | 前1/3/6 × 正信号过滤有/无，6组；最优前3、仅正CAR3 | 2.9939 | 1.4905 | 60.1% | 3.2987 | 2.5939 |
| CAR3直接排序 | 前1/3/6 × 正信号过滤有/无 × 等权/选样日市值，12组；最优前3、不过滤、选样日市值 | 2.8368 | 1.3841 | 58.1% | 2.6784 | 3.0447 |
| 价格动量层内CAR3 | 12-2动量分2/3/5层 × 每层前1/2 × 正信号过滤有/无，12组；最优2层、每层1、不过滤 | 2.7782 | 1.4437 | 53.4% | 2.8311 | 2.7087 |

低波排名以CAR3降序名次与63日波动升序名次相加，等权取前N。分层版每层预算相等，层内按CAR3降序选取，缺候选的层预算留现金；它是受控制价格动量思路启发的本地扩展，不是原文SUE配对/多空中性化。并列均按代码升序。

三个最优配置均经正式静态检查、隔离子进程、8位仓位截断及统一成交/成本引擎评分，扫描与正式总分最大差 **8.88e-16**。带 `--force` 再执行，三个仓位和三个成绩文件逐字节一致。20个现有正式策略在研究输入上重跑，全部仓位与完整成绩文件都与当前v6逐字节一致；所有比较使用同一结果标签。

以上结果只说明这些定期报告日期适配在当前协议下没有入榜，不能据此否定原文盈利动量效应。来源目录覆盖、事件时点替代、全段选参和多轮探索都是实际限制；没有显著性声明或实盘收益承诺。

## 复跑和下一步

```powershell
. D:/Users/Desktop/home/code/.venv/Scripts/Activate.ps1
python scripts/benchmark_earnings_study.py build --release .runtime/benchmark/pools-v5-20261031 --history .runtime/benchmark/history-baostock-20261102 --calendar .runtime/benchmark/public-collection-20261029/raw/calendar.json --announcements .runtime/benchmark/earnings-study-20261102/announcements --market .runtime/benchmark/earnings-study-20261102/csi-H00300-2005-2025.json --out .runtime/benchmark/pools-earnings-study-20261102
python scripts/benchmark_earnings_study.py scan --release .runtime/benchmark/pools-earnings-study-20261102 --out .runtime/benchmark/earnings-study-20261102/scanned
npm run benchmark -- bench-run --release .runtime/benchmark/pools-earnings-study-20261102 --strategies .runtime/benchmark/earnings-study-20261102/scanned/candidates --out .runtime/benchmark/earnings-study-20261102/official-runs
```

构建/扫描拒绝覆盖，复跑要使用新目录。代码与参数生成器进Git，失败候选目录不进Git。复跑需另有公开缓存，不能只取代码就重建市场数据：

- `announcements/../announcement-request.json` 绑定原v5 manifest与逐证券请求范围；各 `<证券>.json` 含相同 `request`、`status=completed`、原始 `announcements`，由已有巨潮适配器获取。目录可包含试点较大范围的原响应，特征仍逐决策严格截断。
- 市场JSON含 `source`、`request={code:H00300,start,end}`、`status=completed`、`rows=[{date,close}]`；源为现有 `csi_index_closes` 适配器，不能由标签中的指数路径反向生成。
- 个股与交易日历缓存契约沿用[长历史实验](benchmark-long-signals-20261102.md#复跑与缓存契约)。

下一步补原始单季EPS、扣除异常项的口径与历史披露版本，验证SUE；另核对快报/预告的首次信息时点。当前不会用更容易取得的净利润增速替代尚缺字段。

验证：121个benchmark测试（含新增6个公告时点/身份/缺失/隔离测试）、架构检查7项、266个前端测试、前端构建/格式、部署测试及Rust fmt/clippy/build通过。完整 `npm test` 在创建一次性PostgreSQL集群前因缺少本机工具失败，数据库、HTTP/Web/桥接与后续财务验收未执行，未使用开发或线上数据库。源校验补充后重建到新目录，manifest哈希与首次研究切片一致，并重跑相关6个测试通过。
