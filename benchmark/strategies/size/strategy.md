# 市值前 5

对照基线。在 `trade_status != "no_bar"` 的成员中，按选样日总市值降序、代码升序取最多 5 只等权分配；无候选时全现金，实际成交由统一评分器决定，不要求决策日市值重新排序。

[实现](strategy.py)的 `REQUIRES` 为 `stocks.*` 下的 `symbol`、`trade_status`（文本）与 `market_cap_at_selection`（非空十进制），无股票池限制；本基线不占策略排名。
