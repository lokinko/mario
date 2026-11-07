# 63 日动量前 5

对照基线。在 `trade_status != "no_bar"`、且 63 个交易日动量非 null 的成员中，取动量最高的最多 5 只等权分配；动量相同按代码升序。动量可以为零或负数，不施加正动量门槛；无候选时全现金，实际成交由统一评分器决定。

[实现](strategy.py)的 `REQUIRES` 为 `stocks.*` 下的 `symbol`、`trade_status`（文本）与 `momentum_63`（可为 null 的十进制），无股票池限制；本基线不占策略排名。
