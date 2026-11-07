# 全池等权

对照基线。每个决策点对 `trade_status != "no_bar"` 的全部成员等权分配，没有成员则全现金。停牌成员也可进入目标仓位，是否实际建仓由统一评分器判断，不把“有行情记录”等同于当天可成交。

[实现](strategy.py)的 `REQUIRES` 仅包含 `stocks.*.symbol`、`stocks.*.trade_status`（均为文本），无股票池限制；本基线不占策略排名。
