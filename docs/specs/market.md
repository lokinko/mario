# 市场域 spec

市场域提供可追溯、口径明确、状态真实的公共市场数据。以交易标的为单位维护共享价格，供用户域和 AI 域使用。全局职责与现有代码映射见[架构](../architecture.md)，个人估值和买点观察的行为见[用户域 spec](user.md)。

本文定义业务行为与数据口径；具体 HTTP 字段和错误格式由 [API 文档](../api.md)维护。

## 1. 标的归一与价格复用

同一交易标的在不同账户、持仓记录和代码别名之间共用一份价格，不为每条证券或持仓记录维护独立行情，也不增加逐条持仓的身份核验流程。

代码归一的目的是让同一标的的数据得到复用。例如，同一只上海证券的 `.SH` 与 `.SS` 表示可以映射到同一个标的。证券名称或别名不同，不意味着必须维护多套价格。

同一公司的 A 股与 H 股属于不同交易标的，价格与币种仍分别维护。标的归一不把不同上市证券合并成一份参考价格。

证券基础信息属于市场域；人工筛选的观察候选目录属于用户域。数据保留来源、采集时间和实际数据日期。

实现与验证依据：[代码归一](../../server/src/models/market.rs)、[公共价格服务](../../server/src/application/public_prices.rs)、[跨账号、别名与重启复用回归](../../server/src/application/public_prices/tests.rs)。现有实现已经覆盖部分常用代码别名和共享价格；本文不宣称支持任意自由文本别名，也不引入新的人工映射功能。

## 2. 价格与历史序列

提供已完成交易日的日线数据，以十年为当前历史窗口目标。上市时间不足、来源缺失或回补未完成时，明确标出实际覆盖范围，不能补造历史。

区分来源价格、复权价格和复权基准。当前报价接口返回前复权收盘价，历史复权价不等于当时实际成交价。不同来源的原始价格口径也可能不同，不能混用。

根据交易所时区、收盘时间和实际来源日期判断可用数据，不把盘中数据、周末或节假日误认为新的完整交易日。行情更新与个人账本操作分别处理，历史复权变化不能改写真实成交。

实现依据：[价格历史契约](../../server/src/models/market/history.rs)、[完成交易日判断](../../server/src/models/market/close.rs)、[行情来源适配与切换](../../server/src/market_data/history.rs)。

## 3. 汇率

提供带币种方向、请求日期、实际观测日期和来源的参考汇率。同币种换算为 1；跨币种计算使用共同日期的数据。

当前支持人民币、美元、港币、欧元、日元和英镑，来源适配为 ECB。参考汇率与实际成交汇率分别处理。用户人工指定的汇率属于用户域，不能覆盖公共参考数据。

实现依据：[ECB 来源适配与共同日期计算](../../server/src/market_data.rs)、[币种契约](../../server/src/models/market.rs)、[用户汇率用例](../../server/src/application/fx.rs)。

## 4. 分红与公司行动

保存已披露分红计划、已实施分红及拆股等公司行动，明确阶段、日期、金额口径和来源。

预案、批准、实施安排、取消等状态分别处理；未来计划不能计入已实施分红。来源缺失或分页未收齐，不能解释为“没有分红”。

市场域提供公司事实，用户域决定如何用于观察、日历和个人账本；公司分红不等于个人已经到账。

公共事件、日期覆盖和每股 TTM 计算由[市场分红契约](../../server/src/models/market/dividends.rs)维护，公共价格条目与采集输入见[行情序列](../../server/src/models/market/series.rs)。市场来源与公共仓储只使用市场契约，不依赖用户自选、观察阈值或账号锁能力。证券资料查询返回[官方来源元数据](../../server/src/models/market/securities.rs)，由用户域决定是否加入候选目录。内置分红资料包不计算用户观察参考价，CSV 参考价展示由用户域生成。

实现依据：[分红来源](../../server/src/market_data/watch.rs)、[已披露计划解析](../../server/src/market_data/watch/plans.rs)、[公共分红仓储](../../server/src/db/public_dividends.rs)、[分红计划缓存](../../server/src/db/public_dividend_plans.rs)、[海外公司行动解析](../../server/src/market_data/history/overseas.rs)。已披露计划的类型、缓存和采集分别由市场域的[计划类型](../../server/src/models/market/dividend_plans.rs)、[计划仓储](../../server/src/db/public_dividend_plans.rs)、[计划服务](../../server/src/application/dividend_plans.rs)管理；用户日历只通过缓存读端口消费。计划仓储独立打开原有表，不初始化或升级候选目录。

## 5. 共享缓存、更新与失败处理

同一交易标的数据在账号之间复用。更新需要控制并发、重试和超时，避免重复采集。

更新失败保留上次成功版本，同时记录失败和数据日期；没有旧数据时明确返回不可用。保留旧版本不意味着本次更新成功，也不意味着所有调用都会静默返回旧数据。

备用来源失败不能丢弃已有可用结果。切换来源不能拼接成口径不一致的历史序列。

专门的缓存读取和采集状态读取不触发联网；需要联网的查询、刷新能力明确区分。

实现依据：[公共价格用例](../../server/src/application/public_prices.rs)、[公共价格仓储](../../server/src/db/public_prices.rs)、[共享分红查询](../../server/src/application/watch_market.rs)、[市场查询端口](../../server/src/ports/market.rs)。

## 6. 采集范围与质量状态

当前公共价格采集包含沪深300成分范围，以及按需登记的其他标的。成分表更新需要核对标的、数量、重复项和日期；失败或日期倒退时保留原范围。成分表完整性校验用于判断采集范围，不增加个人持仓的重复身份核验。

分别记录采集成功、失败、待处理、数据滞后，以及历史不足和缺失值。采集成功不等于数据完整或足够新。

采集状态覆盖全部已保存公共价格的标的：沪深300成分、作为 A 股交易日参考的上证指数，以及按需登记的 A 股、港股、美股股票和基金/ETF，并标明所属范围、市场、品种和币种。每个标的按所在交易所的收盘时间判断应完成的交易日；与上证指数比较的滞后判断只用于 A 股。当日统计只计入仍在采集范围内的标的；登记已过期的标的保留已有价格并标为停止跟踪，不再计入成功或失败。按需登记标的没有公共名称时只显示代码，不读取账号数据补全。

采集进度需要持久保存，中断和重启后能够识别未完成任务。市场域提供这些状态，运营看板负责展示和访问控制。

实现依据：[成分范围来源](../../server/src/market_data/price_universe.rs)、[质量状态投影](../../server/src/application/public_prices/monitor.rs)、[采集状态持久化](../../server/src/db/public_prices/monitor.rs)、[运营接口](../../server/src/hosted/activity.rs)。

## 7. 验收场景

| 范围 | 必须核对的场景 | 现有验证入口 |
| --- | --- | --- |
| 标的与共享价格 | 同一标的跨账号、持仓和别名共用价格；个人数量和成本独立；不同上市证券不误合并 | [标的归一测试](../../server/src/models/market.rs)、[共享价格回归](../../server/src/application/public_prices/tests.rs) |
| 价格口径与日期 | 交易所时区、夏令时、完成交易日、复权基准、实际覆盖不足、缺失价格 | [收盘策略测试](../../server/src/models/market/close.rs)、[价格历史测试](../../server/src/models/market/history/tests.rs)、[来源解析测试](../../server/src/market_data/history/tests.rs) |
| 汇率 | 共同观测日期、同币种换算、数据缺失、参考汇率与个人覆盖值分离 | [ECB 模块内测试](../../server/src/market_data.rs)、[个人汇率用例内测试](../../server/src/application/fx.rs) |
| 分红与公司行动 | 已披露计划与已实施事件区分、分页不完整、未知与零的区别、来源日期与标的一致 | [分红模块内测试](../../server/src/market_data/watch.rs)、[计划解析测试](../../server/src/market_data/watch/plans.rs)、[海外公司行动测试](../../server/src/market_data/history/overseas/tests.rs) |
| 更新与恢复 | 来源切换、失败保留旧数据、并发复用、重启恢复、慢标的不阻塞整轮采集 | [公共价格回归](../../server/src/application/public_prices/tests.rs)、[来源切换回归](../../server/src/market_data/history/tests.rs)、[采集状态回归](../../server/src/db/public_prices/monitor.rs) |

以上是验收场景与现有测试依据，不表示文档整理时重新执行了全部测试或联网来源验收。
