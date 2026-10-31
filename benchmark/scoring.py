"""主指标：按目标仓位计算建仓后三段不重叠区间的总资产增长，加权换算为百分比得分。

区间分界取建仓日之后第 7、30、90 个自然日当天或之前的最后一个交易日收盘；
三段收益分别是 0–7、7–30、30–90 天的总资产增长，权重 0.3、0.3、0.4。
"""

import random
from datetime import date, timedelta
from decimal import Decimal
from statistics import mean, median, pstdev

from .files import require, symbol

SEGMENT_DAYS = (7, 30, 90)
SEGMENT_WEIGHTS = (Decimal("0.3"), Decimal("0.3"), Decimal("0.4"))
BUY_COST = Decimal("0.0013")
COST_BASIS = "买入扣佣金 0.03% 与滑点 0.10%；持有期按收盘价估值，不计卖出成本"


def boundaries(calendar, entry_date):
    """calendar 为升序交易日列表；数据不足 90 天时返回 None。"""
    require(entry_date in calendar, "建仓日不是交易日")
    output = []
    for days in SEGMENT_DAYS:
        limit = (date.fromisoformat(entry_date) + timedelta(days=days)).isoformat()
        if calendar[-1] < limit:
            return None
        output.append(max(d for d in calendar if d <= limit))
    return output


def can_buy(bar):
    """停牌或全天一字涨停时买不进，资金留作现金。"""
    if not bar or bar["tradestatus"] != "1":
        return False
    one_price = bar["open"] == bar["high"] == bar["low"]
    return not (one_price and Decimal(bar["close"]) > Decimal(bar["preclose"]))


def valuation(path, day):
    """path 为按日期升序的复权收盘价 [(日期, 价格)]；停牌沿用最后价格，退市后保持退市前最后价格。"""
    known = [price for when, price in path if when <= day]
    return known[-1] if known else None


def portfolio_segments(weights, outcome, cost=BUY_COST):
    """weights：{证券: Decimal}，合计不超过 1，余下为现金（收益为 0）。"""
    require(all(w >= 0 for w in weights.values()) and sum(weights.values()) <= 1, "仓位须非负且合计不超过 1")
    cash, units, blocked = Decimal(1) - sum(weights.values()), {}, []
    for code, weight in weights.items():
        if not weight:
            continue
        stock = outcome["stocks"][code]
        if not stock["buyable"]:
            cash += weight
            blocked.append(code)
            continue
        units[code] = weight * (1 - cost) / Decimal(stock["entry_open"])
    values = [Decimal(1)]
    for day in outcome["boundaries"]:
        total = cash
        for code, amount in units.items():
            total += amount * Decimal(valuation(outcome["stocks"][code]["path"], day))
        values.append(total)
    returns = [values[i + 1] / values[i] - 1 for i in range(3)]
    return returns, blocked


def index_segments(outcome):
    entry = Decimal(outcome["index"]["entry_open"])
    values = [entry] + [Decimal(valuation(outcome["index"]["path"], day)) for day in outcome["boundaries"]]
    return [values[i + 1] / values[i] - 1 for i in range(3)]


def score(returns):
    return float(100 * sum(w * r for w, r in zip(SEGMENT_WEIGHTS, returns, strict=True)))


def parse_weights(rows, samples):
    pools = {s["id"]: {stock["symbol"] for stock in s["stocks"]} for s in samples}
    output = {}
    for row in rows:
        sample_id, code = row["sample_id"], symbol(row["symbol"])
        require(sample_id in pools and code in pools[sample_id], f"仓位引用未知样本或非股票池证券: {sample_id}/{code}")
        require(isinstance(row["weight"], str), "weight 必须为十进制字符串")
        weight = Decimal(row["weight"])
        require(weight.is_finite() and weight >= 0, "weight 必须为非负有限数")
        require(code not in output.setdefault(sample_id, {}), "仓位键重复")
        output[sample_id][code] = weight
    for sample_id, weights in output.items():
        require(sum(weights.values()) <= 1, f"{sample_id} 仓位合计超过 1")
    return output


def bootstrap_interval(values, rounds=2000, seed=20261031):
    if len(values) < 2:
        return None
    generator = random.Random(seed)
    means = sorted(mean(generator.choice(values) for _ in values) for _ in range(rounds))
    return [means[int(0.025 * rounds)], means[int(0.975 * rounds) - 1]]


def summarize(values):
    return {"mean": mean(values), "median": median(values), "stdev": pstdev(values),
            "count": len(values), "bootstrap_95": bootstrap_interval(values)} if values else None


def evaluate_weights(samples, outcomes, weight_rows):
    """未提交仓位的样本视为全部持有现金，得分为 0，不能靠跳过样本提高均值。"""
    weights = parse_weights(weight_rows, samples)
    by_id = {row["sample_id"]: row for row in outcomes}
    periods = []
    for sample in samples:
        outcome = by_id.get(sample["id"])
        if not outcome:
            continue
        returns, blocked = portfolio_segments(weights.get(sample["id"], {}), outcome)
        equal = {code: Decimal(1) / len(sample["stocks"]) for code in outcome["stocks"]}
        pool_returns, _ = portfolio_segments(equal, outcome)
        index_returns = index_segments(outcome)
        periods.append({
            "sample_id": sample["id"], "pool": sample["pool"], "decision_date": sample["decision_date"],
            "entry_date": outcome["entry_date"], "boundaries": outcome["boundaries"],
            "submitted": sample["id"] in weights, "blocked_entries": blocked,
            "segment_returns": [float(r) for r in returns], "score": score(returns),
            "pool_equal_weight_score": score(pool_returns), "csi300_score": score(index_returns)})
    require(periods, "没有可评分的样本")
    scores = [p["score"] for p in periods]
    by_year, by_pool = {}, {}
    for period in periods:
        by_year.setdefault(period["decision_date"][:4], []).append(period)
        by_pool.setdefault(period["pool"], []).append(period)
    return {
        "metric": "0.3×R(0–7天) + 0.3×R(7–30天) + 0.4×R(30–90天)，总资产增长，单位为百分比",
        "cost_basis": COST_BASIS,
        "score": summarize(scores),
        "pool_equal_weight_score": summarize([p["pool_equal_weight_score"] for p in periods]),
        "csi300_score": summarize([p["csi300_score"] for p in periods]),
        "beat_csi300_periods": sum(p["score"] > p["csi300_score"] for p in periods),
        "beat_pool_equal_weight_periods": sum(p["score"] > p["pool_equal_weight_score"] for p in periods),
        "by_year": {year: {"score": mean(p["score"] for p in rows),
                           "pool_equal_weight_score": mean(p["pool_equal_weight_score"] for p in rows),
                           "csi300_score": mean(p["csi300_score"] for p in rows), "periods": len(rows)}
                    for year, rows in sorted(by_year.items())},
        "by_pool": {pool: {"score": summarize([p["score"] for p in rows]),
                           "pool_equal_weight_score": summarize([p["pool_equal_weight_score"] for p in rows]),
                           "csi300_score": summarize([p["csi300_score"] for p in rows])}
                    for pool, rows in sorted(by_pool.items())},
        "periods": periods,
        "limitations": ["股票池与行业归属尚未人工核对（membership_verified=false）",
                        "退市证券按退市前最后价格估值，未计换股、清算或退市整理期",
                        "后复权价格近似股息再投资，未计红利税与到账时点；沪深300 对照用全收益指数 H00300",
                        "被吸收合并的证券按停牌前最后价格估值，未折算为存续公司股份",
                        "LLM 可能记得历史结果；时间截断不能消除预训练数据污染"]}
