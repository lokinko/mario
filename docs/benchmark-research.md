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
| E01 | [Gary Antonacci：Dual, Relative & Absolute Momentum](https://www.optimalmomentum.com/dual-relative-absolute-momentum/) 与 [作者 FAQ](https://www.optimalmomentum.com/faq/)：区分横截面的相对强弱与自身超额收益的绝对趋势；公开模型采用较长回看与跨资产防御配置 | 当前只有 63 日动量和股票池，缺 12 个月信号、无风险收益及池外防御资产 | **63 日长仓/现金适配已测**，最优 2.6928；12 个月信号待价格口径对账，完整 GEM 组合与现有资产约束不同 |
| E02 | [Moskowitz、Ooi、Pedersen：Time Series Momentum，2012](https://www.aqr.com/Insights/Research/Journal-Article/Time-Series-Momentum)：用自身过去 12 个月超额收益方向形成多空趋势，研究对象为跨资产期货及远期 | 不允许做空或杠杆，缺 12 个月收益与无风险收益 | **63 日、逆波动预算、空头改现金适配已测**，2.0119；不将其解释为原期货策略复现 |
| E03 | [MSCI Momentum Indexes Methodology，2025-04，§2.2–2.4](https://www.msci.com/indexes/documents/methodology/2_MSCI_Momentum_Indexes_Methodology_20250417.pdf)：6/12 个月动量跳过最近一个月，调整无风险收益，除以三年周收益波动，合成标准分并按动量得分与母指数市值权重配置 | 缺 6/12 个月回看、三年周波动、利率和调仓时点母指数权重；现有市值是年度选样日值 | **63 日/剔除 20 日的短窗口标准分适配已测**，最优 2.6629；先补齐价格再测试原动量计算，不把短窗口版称为 MSCI 指数复现 |
| E04 | [Moreira、Muir：Volatility-Managed Portfolios，2017，§I.B、表 IV–V](https://amoreira2.github.io/alan-moreira.github.io/VolPortfolios_published.pdf)：按前一月组合已实现方差的倒数缩放风险敞口；另讨论不加杠杆版本 | 当前只有个股 63 日波动，不能直接获得前月组合的日收益方差 | **平均个股波动代理的逆方差适配已测**，3.2671，弱于现有同家族逆波动预算 3.3226，故不新增参数变体；原组合方差版待历史日收益 |
| E05 | [Asness、Frazzini、Pedersen：Low-Risk Investing without Industry Bets，2014](https://www.aqr.com/-/media/AQR/Documents/Insights/Journal-Article/FAJ-LowRisk-Investing-Without-Industry-Bets.pdf)：研究行业内部的低 beta 效应及行业暴露影响 | 当前没有 beta；个股波动不能等同于市场 beta，也不允许原研究的多空构建 | **行业等预算、行业内低波长仓代理已测**，最优 2.5375；原 beta 计算与多空组合未复现 |
| E06 | [Meb Faber：A Quantitative Approach to Tactical Asset Allocation，作者原规则回顾](https://mebfaber.com/2017/12/13/episode-86-quantitative-approach-tactical-asset-allocation/)：月末价格高于 10 个月简单均线时持有，否则现金 | 当前没有 10 个月价格路径；原多资产、月度调仓及现金收益也与季度股票窗口不同 | **待补数据**，优先复现均线信号，再单独声明季度股票协议的适配，不能用动量为正替代均线比较 |
| E07 | [Robeco：Guide to Conservative Investing，2023，p.6](https://www.robeco.com/docm/docu-202302-guide-to-conservative-investing.pdf)：先从大市值股票选三年低波，再按净派息率与动量选择，等权季度调仓 | 缺三年波动和可证明历史披露时点的净派息/回购数据；红利池身份不等于净派息率 | **待补数据**，先明确现金股息与净回购分子、时点市值分母及披露日期，不声称此前“低波+动量”就是完整 Conservative Formula |
| E08 | [Da、Gurun、Warachka：Frog in the Pan，作者工作稿](https://papers.ssrn.com/sol3/papers.cfm?abstract_id=1777988)；[Alpha Architect 自述的动量质量流程](https://alphaarchitect.com/quantitative-momentum-investing-philosophy/)：关注收益路径的连续性，而非只看期末累计涨幅 | 仅凭累计动量与标准差不能重建每日涨跌频率或离散信息度；机构方法还包含其他筛选 | **待补长日线并核对完整定义**，只得到低波动量高分不能视作 FIP 复现 |
| E09 | [George、Hwang：The 52-Week High and Momentum Investing，2004，出版记录](https://onlinelibrary.wiley.com/doi/abs/10.1111/j.1540-6261.2004.00695.x) | 现有输入没有 52 周最高价；完整形成期、跳月和组合定义仍需读原文确认 | **待补数据/读原文**，不以当前股价绝对高低或 63 日动量充当距年度高点的信号 |

E01–E05 的逐项规则、参数范围、正式成绩及复核方式见 [本轮实验记录](audits/benchmark-external-strategies-20261102.md)。本轮没有胜出的独立新机制，当前 Top-10 不变。临时失败候选只在 `.runtime/` 中，不进入正式策略目录和网站看板。

## 长历史来源核查

现有切片的价格为 Baostock 后复权；原有 Yahoo 候选语料在项目中也一直标为未完成复权对账，不能直接互相替换或拼接。当前研究已核查到：

1. 约定虚拟环境已安装仓库 `market` 可选组声明的 `baostock==0.9.4`。对 `000063.SZ`、2024-01-01 至 2025-12-31 的后复权查询，在 8 秒 socket 超时与 35 秒父进程上限的探测中返回“网络接收错误”，未取得该次日线文件。此事实不证明所有时期和所有接口都不可用。
2. 使用仓库已有 Yahoo 适配器获取同股 2004-01-01 至 2025-12-31 的原响应，归档了 5382 条有效调整收盘观察，尚未接入 benchmark。按已缓存的独立交易日历，对 v5 的 73 个非空 63 日动量决策点做端点比较，均能找到端点，但原响应有 40 个非交易日观察。
3. 最大绝对动量差为 0.220263946，即约 **22.03 个百分点**，发生于 `tech-2011-03-31`，起点为 2010-12-24：Yahoo 比值为 0.321585532，v5 为 0.101321586。尚不能确定差异来自哪一方或具体公司行动，需要进一步独立核对；不据此修改已有成绩或宣布某一来源错误。

探测和对账只读取公开价格与既有时点输入，未把 `outcomes.jsonl` 中的未来路径反向回填为策略输入。所有原响应与诊断留在 `.runtime/benchmark/external-research-20261102/`，不上传或提交市场原始数据。

## 下一轮优先事项

1. 独立核查 `000063.SZ` 的 2010-12 至 2011-03 价格及公司行动，先解释最大差异；验证另一来源的未复权价与复权因子，不盲目批量下载后拼接。
2. 统计 v5 历年已选证券的并集和最长回看需求，分来源采集所需历史，包含已退出或退市证券。明确端点缺失、停牌、交易日历与公司行动的处理，不用当前存续名单替代历史覆盖。
3. 在独立、不可覆盖的新研究输入版本中加入经过验证的 6/12 个月收益、月末均线、年度高点、每日涨跌频率及组合历史波动；所有计算严格截断到决策日，新增信息不足的状态显式保留。先比较新旧共同字段与标签不变性，再使用长窗口信号。
4. 优先验证 E01、E03、E06、E08、E09 的长窗口信号；需要 beta、三年周收益或净派息字段的工作分别推进，不把指标缺失转化为不透明近似。
5. 继续搜索价格类以外的优秀规则，补充真正按披露时间可得的财务与回购数据。当前研究目标保持进行中；没有把“跑完这一批适配版”定义为完成持续研究。
