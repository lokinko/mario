# 外部投资策略研究清单

这是持续研究的工作清单，目标是从外部原始规则寻找可迁移机制，并在 Mario benchmark 中逐一验证。它不把外部历史业绩当作本切片结果，也不把数据不全时的近似实现标为原策略复现。每种逻辑只保留一组最优参数，当前有效成绩以 [benchmark 成绩表](benchmark.md#新增策略与自动评测) 和策略目录为准。

## 验证口径

- **规则适配已测**：明确改变了资产池、输入窗口或执行方式，在现有切片上验证改写后的规则；不能据此确认或否定原论文策略。
- **待补数据**：现有时点输入不能计算关键规则，先验证来源和时间口径，不用近似指标偷偷替换。
- **原组合不兼容**：原规则要求做空、杠杆、池外资产或不同调仓频率。可以测试声明过差异的信号迁移，但不声称得到原组合收益。
- 参数比较都属于样本内研究。原有 148 期已被反复研究，2006–2017 和 2018–2025 仅作分段诊断；外部规则事先发表，并不使本项目的适配和参数选择自动变成样本外。

## 已核对来源与进度（2026-11-02）

| ID | 外部来源与核心规则 | 与现有 benchmark 的关系 | 状态与下一步 |
| --- | --- | --- | --- |
| E01 | [Gary Antonacci：Dual, Relative & Absolute Momentum](https://www.optimalmomentum.com/dual-relative-absolute-momentum/) 与 [作者 FAQ](https://www.optimalmomentum.com/faq/)：区分横截面的相对强弱与自身超额收益的绝对趋势；公开模型采用较长回看与跨资产防御配置 | 正式 v5 只有 63 日动量；独立研究输入已补 12 个月收益，仍缺无风险收益与池外防御资产 | **63 日代理已测** 2.6928；**12 个月季度股票长仓/现金适配已测** 1.9659，完整 GEM 组合与现有资产约束不同 |
| E02 | [Moskowitz、Ooi、Pedersen：Time Series Momentum，2012](https://www.aqr.com/Insights/Research/Journal-Article/Time-Series-Momentum)：用自身过去 12 个月超额收益方向形成多空趋势，研究对象为跨资产期货及远期 | 不允许做空或杠杆；研究输入已有 12 个月收益和三年周波动，仍缺无风险收益 | **63 日代理已测** 2.0119；**12 个月/三年周波动逆风险版已测** 1.4373，完整三年覆盖不足导致部分期现金；不解释为原期货策略复现 |
| E03 | [MSCI Momentum Indexes Methodology，2025-04，§2.2–2.4](https://www.msci.com/indexes/documents/methodology/2_MSCI_Momentum_Indexes_Methodology_20250417.pdf)：6/12 个月动量跳过最近一个月，调整无风险收益，除以三年周收益波动，合成标准分并按动量得分与母指数市值权重配置 | 已补跳月动量及严格三年周波动；仍缺利率与调仓时点母指数权重；现有市值是年度选样日值 | **短窗口适配已测** 2.6629；**长窗口适配已测** 2.0202，三年周波动完整覆盖只有1,221/2,960，需保留该限制，不能称为指数复现 |
| E04 | [Moreira、Muir：Volatility-Managed Portfolios，2017，§I.B、表 IV–V](https://amoreira2.github.io/alan-moreira.github.io/VolPortfolios_published.pdf)：按前一月组合已实现方差的倒数缩放风险敞口；另讨论不加杠杆版本 | 同源历史日线已补齐，已计算固定当前权重的历史篮子日收益；它仍不同于原文过去实际因子组合收益 | **平均个股逆方差代理已测** 3.2671；**核心/卫星篮子协方差预算已正式准入** 3.5163，v6 第一，较原最高高0.0204但区间含0；用户确认后已发布并下载复核 |
| E05 | [Asness、Frazzini、Pedersen：Low-Risk Investing without Industry Bets，2014](https://www.aqr.com/-/media/AQR/Documents/Insights/Journal-Article/FAJ-LowRisk-Investing-Without-Industry-Bets.pdf)：研究行业内部的低 beta 效应及行业暴露影响 | 当前没有 beta；个股波动不能等同于市场 beta，也不允许原研究的多空构建 | **行业等预算、行业内低波长仓代理已测**，最优 2.5375；原 beta 计算与多空组合未复现 |
| E06 | [Meb Faber：A Quantitative Approach to Tactical Asset Allocation，作者原规则回顾](https://mebfaber.com/2017/12/13/episode-86-quantitative-approach-tactical-asset-allocation/)：月末价格高于 10 个月简单均线时持有，否则现金 | 独立研究输入已补 10 个月月末价格；原多资产、月度调仓及现金收益与季度股票窗口不同 | **真实 10 月均线信号、季度股票适配已测**，1.8064；低于均线或数据不足的固定预算留现金，未复现原多资产组合 |
| E07 | [Robeco：Guide to Conservative Investing，2023，p.6](https://www.robeco.com/docm/docu-202302-guide-to-conservative-investing.pdf)：先从大市值股票选三年低波，再按净派息率与动量选择，等权季度调仓 | 缺三年波动和可证明历史披露时点的净派息/回购数据；红利池身份不等于净派息率 | **待补数据**，先明确现金股息与净回购分子、时点市值分母及披露日期，不声称此前“低波+动量”就是完整 Conservative Formula |
| E08 | [Da、Gurun、Warachka：Frog in the Pan，2011-11 完整工作稿，§2–3](https://business.uq.edu.au/sites/default/files/events/files/mitch-warachka-paper.pdf)：ID = sign(PRET) ×（负收益日比例 − 正收益日比例），在过去赢家中再找低 ID | 已核对每日符号、零收益分母和跳最近一月；研究输入已补真实日线；仍缺原文财务过滤及月度多空/六个月持仓协议 | **12-2 形成期、季度长仓双排序适配已测**，2.8760；不是原文多空组合复现 |
| E09 | [George、Hwang：The 52-Week High and Momentum Investing，2004，作者全文，§I、表 I–IV](https://www.bauer.uh.edu/tgeorge/papers/gh4-paper.pdf)：当前月末价/过去十二个月最高价排序，原多空组合持有六个月 | 研究输入分别补日最高价与最高收盘价；采用原描述性表格不跳月的信号，未复现月度重叠多空组合 | **季度长仓适配已测**，2.6155；进一步与低波等权排名合成的研究扩展为 2.9548，都未进入 Top-10 |

新增候选来源：[Novy-Marx，Fundamentally, Momentum is Fundamental Momentum，NBER w20984](https://www.nber.org/papers/w20984)。本次搜索取得题录与摘要，但正文和 PDF 访问返回 403；尚未核对完整信号、形成期和财报时点，不把摘要当作可执行规则，也没有测分。下一步应获取作者原稿并核查实际披露时间的数据路径，避免研究一直局限于价格类因子。

E01–E05 的短窗口适配见 [第一批实验](audits/benchmark-external-strategies-20261102.md)；真实长窗口的 5 类/35 组比较、输入覆盖与复核见 [第二批实验](audits/benchmark-long-signals-20261102.md)。这两批没有胜出的独立新机制，未改变当时的Top-10。失败候选只在 `.runtime/` 中，不进入正式策略目录和网站看板；第二批的生成和扫描程序已提交，可从同构缓存重建。

第三批的 4 类/45 组比较、当前 v6 Top-10 与发布复核见 [组合风险实验](audits/benchmark-risk-signals-20261102.md)。用户已授权发布新增输入，现已完成两个私有仓库的不可变v6标签、全新目录下载校验、正式新策略目录和本地看板。当前第一为 `trend_basket_risk`，20项中只新增这一种机制；部署默认数据标签改为v6，本次未执行网站部署。

## 长历史来源核查

现有切片的价格为 Baostock 后复权；原有 Yahoo 候选语料在项目中也一直标为未完成复权对账，不能直接互相替换或拼接。当前研究已核查到：

1. 约定虚拟环境已安装仓库 `market` 可选组声明的 `baostock==0.9.4`。最初的 8 秒超时查询失败；后续使用最小字段 `date,code,close,high,tradestatus` 和 20 秒 socket 上限恢复成功。仅凭此差异不能认定具体字段是最初失败的原因。
2. 使用仓库已有 Yahoo 适配器获取同股 2004-01-01 至 2025-12-31 的原响应，归档了 5382 条有效调整收盘观察，尚未接入 benchmark。按已缓存的独立交易日历，对 v5 的 73 个非空 63 日动量决策点做端点比较，均能找到端点，但原响应有 40 个非交易日观察。
3. 最大绝对动量差约 **22.03 个百分点**，发生于 `tech-2011-03-31`，起点为 2010-12-24。腾讯原始收盘价 `30.00/27.24-1` 与 v5 的 `0.101321585903083700440528634` 一致；Yahoo 收益比值相对它多出约 1.2 倍因子，其响应同时列出 2011-03-18 与 2011-07-07 两次 12:10 拆股。[公司 2011 半年报，正文 p.24](https://www.zte.com.cn/content/dam/zte-site/investorrelations/cn_announcement/58f5b9eac2c6cb26f3ce2f970674c878.pdf) 明确 A 股该次转增的除权除息日是 7 月 7 日。因此重复/错时拆股是对 Yahoo 差异的有力解释，供应商内部处理原因仍为推断；没有逐股硬修 Yahoo，也未改变原 v5。
4. 已取得 v5 全部 172 只历史证券的 350,956 条同源后复权日线，包括退出股票。5 次请求明确失败，另 2 次请求虽报成功却只取得 2,000 行，均保留原记录，改用两年分块补采并逐块缓存。最终无内部交易日缺口；原 v5 非空的 2,796 个 63 日动量值、2,827 个 20 日收益值与新历史逐项完全一致。交易日历和缺失端点检查仍必须保留，不能只以 SDK 状态码判定采集完整。

探测和对账只读取公开价格与既有时点输入，未把 `outcomes.jsonl` 中的未来路径反向回填为策略输入。所有原响应与诊断留在 `.runtime/benchmark/external-research-20261102/` 和 `.runtime/benchmark/history-baostock-20261102/`，不上传或提交市场原始数据。

## 下一轮优先事项

1. 在已发布v6及当前20项基准上继续研究；`trend_basket_risk` 只保留最优参数，原有个股平均波动预算因选股与估计方法不同而保留。网站部署须另有任务授权。
2. 核查盈利公告动量、盈利质量、净派息等原规则与可按披露时间取得的数据，优先处理财报修订和公告日期，不能用最新财务表回填历史。
3. 研究原文过去实际组合收益方差与当前固定篮子风险估计的差异；需要过去执行路径时另补当时开盘及成交状态，不从标签文件回填输入。严格三年周波动因停牌失效的记录保留，不能为提高分数悄悄填值。
4. beta 与净派息字段分别推进，不把指标缺失转化为不透明近似；月度、多空等完整原组合仍与本 benchmark 的季度长仓协议有区别。
5. 继续搜索价格类以外的优秀规则，补充真正按披露时间可得的财务与回购数据。当前研究目标保持进行中；没有把“跑完这一批适配版”定义为完成持续研究。
