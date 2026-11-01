"""主指标：收益得分的平均值加胜率得分。

收益得分：按目标仓位计算建仓后三段不重叠区间的总资产增长，加权换算为百分比。区间分界取建仓日之后
第 7、30、90 个自然日当天或之前的最后一个交易日收盘；三段为 0–7、7–30、30–90 天，权重 0.3、0.3、0.4。
总分 = 平均收益得分 + 5 ×（胜率 − 50%），胜率为收益得分大于 0 的期数占比；等价于每期收益得分赢加 2.5、
输（含等于 0）减 2.5 后取平均，所以逐期总分可以直接求中位数、自助法区间和分组均值。

仓位可附卖出规则（止损、移动止损、止盈、最长持有天数），在决策日确定，由本模块按建仓后的价格逐日执行：
收盘触发，次一个可卖出交易日开盘卖出，所得留作现金直到 90 天结束，不再买回。

仓位也可改附网格：资金分成若干份，建仓买入其中几份，收盘价每向下穿过一条网格线加一份、向上穿过一条减一份，
次一个可买入或可卖出交易日开盘成交；网格只在该股票的资金内循环，不与卖出规则同用。
"""

import math
import random
from datetime import date, timedelta
from decimal import Decimal
from statistics import mean, median, pstdev

from .files import require, symbol

SEGMENT_DAYS = (7, 30, 90)
SEGMENT_WEIGHTS = (Decimal("0.3"), Decimal("0.3"), Decimal("0.4"))
WIN_RATE_WEIGHT = 5
SERIES = ("", "pool_equal_weight_", "csi300_")
METRIC = ("总分 = 平均收益得分 + 5 ×（胜率 − 50%）；收益得分 = 0.3×R(0–7天) + 0.3×R(7–30天) + 0.4×R(30–90天)，"
          "总资产增长，单位为百分比；胜率为收益得分大于 0 的期数占比")
BUY_COST = Decimal("0.0013")
SELL_COST = Decimal("0.0013")
STAMP_DUTY = (("2023-08-28", Decimal("0.0005")), ("2008-04-24", Decimal("0.001")), ("2007-05-30", Decimal("0.003")),
              ("0000-00-00", Decimal("0.001")))
EXIT_FRACTIONS = ("stop_loss", "trailing_stop", "take_profit")
MAX_GRID_LOTS = 20
COST_BASIS = ("买入扣佣金 0.03% 与滑点 0.10%；卖出规则触发后次一个可卖出交易日开盘卖出，扣佣金 0.03%、滑点 0.10% "
              "与当时的卖出印花税；网格加减仓在收盘穿线后次一个可买入或可卖出交易日开盘成交，成本相同；"
              "未卖出的仓位按收盘价估值，不计卖出成本")


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


def can_sell(bar):
    """停牌或全天一字跌停时卖不出。"""
    if not bar or bar["tradestatus"] != "1":
        return False
    one_price = bar["open"] == bar["high"] == bar["low"]
    return not (one_price and Decimal(bar["close"]) < Decimal(bar["preclose"]))


def sell_cost(day):
    return SELL_COST + next(rate for start, rate in STAMP_DUTY if day >= start)


def valuation(path, day):
    """path 按日期升序，每行首两项为 [日期, 复权收盘价]；停牌沿用最后价格，退市后保持退市前最后价格。"""
    known = [row[1] for row in path if row[0] <= day]
    return known[-1] if known else None


def exit_rule(value):
    """校验卖出规则：止损、移动止损为 (0, 1) 内的比例，止盈为正比例，最长持有天数为 1–90 的自然日。"""
    require(isinstance(value, dict) and value and set(value) <= {*EXIT_FRACTIONS, "max_hold_days"}, "卖出规则字段无效")
    rule = {}
    for key in EXIT_FRACTIONS:
        if key in value:
            require(isinstance(value[key], str), f"{key} 必须为十进制字符串")
            fraction = Decimal(value[key])
            require(fraction.is_finite() and fraction > 0 and (key == "take_profit" or fraction < 1), f"{key} 超出范围")
            rule[key] = fraction
    if "max_hold_days" in value:
        days = value["max_hold_days"]
        require(isinstance(days, int) and not isinstance(days, bool) and 1 <= days <= SEGMENT_DAYS[-1],
                "max_hold_days 必须为 1–90 的整数")
        rule["max_hold_days"] = days
    return rule


def triggered(rule, cost, high, close, held_days):
    if "stop_loss" in rule and close <= cost * (1 - rule["stop_loss"]):
        return "stop_loss"
    if "trailing_stop" in rule and close <= high * (1 - rule["trailing_stop"]):
        return "trailing_stop"
    if "take_profit" in rule and close >= cost * (1 + rule["take_profit"]):
        return "take_profit"
    if "max_hold_days" in rule and held_days >= rule["max_hold_days"]:
        return "max_hold_days"
    return None


def simulate_exit(units, stock, rule, entry_date):
    """按收盘价检查触发；触发后在之后第一个可卖出交易日开盘卖出。窗口内没卖出时 sell_date 为 None。"""
    cost = high = Decimal(stock["entry_open"])
    signal = None
    for day, close, open_price, sellable, _ in stock["path"]:
        if signal:
            if sellable:
                proceeds = units * Decimal(open_price) * (1 - sell_cost(day))
                return {**signal, "sell_date": day, "proceeds": proceeds}
            continue
        close = Decimal(close)
        high = max(high, close)
        reason = triggered(rule, cost, high, close, (date.fromisoformat(day) - date.fromisoformat(entry_date)).days)
        if reason:
            signal = {"reason": reason, "signal_date": day}
    return {**signal, "sell_date": None} if signal else None


def grid_rule(value):
    """校验网格：step 为 (0, 1) 内的线距比例，lots 为 2–20 份，initial 为建仓买入的份数（1 到 lots）。"""
    require(isinstance(value, dict) and set(value) == {"step", "lots", "initial"}, "网格须含且只含 step、lots、initial")
    require(isinstance(value["step"], str), "step 必须为十进制字符串")
    step = Decimal(value["step"])
    require(step.is_finite() and 0 < step < 1, "step 超出范围")
    lots, initial = value["lots"], value["initial"]
    require(all(isinstance(v, int) and not isinstance(v, bool) for v in (lots, initial)), "lots 与 initial 必须为整数")
    require(2 <= lots <= MAX_GRID_LOTS and 1 <= initial <= lots, f"lots 须为 2–{MAX_GRID_LOTS}，initial 须为 1 到 lots")
    return {"step": step, "lots": lots, "initial": initial}


def simulate_grid(budget, stock, rule, entry_date, edges, cost):
    """网格线为建仓价 ×（1 + step）^i。持有份数 = initial − 当前所在线号，限制在 0 到 lots 之间；
    收盘越过上一条或下一条线才移动线号，因此同一条线上不会来回成交。返回各分界日的资产值与成交次数。"""
    lot, base, ratio = budget / rule["lots"], Decimal(stock["entry_open"]), 1 + rule["step"]
    held = target = rule["initial"]
    units = held * lot * (1 - cost) / base
    cash, level, buys, sells, marks = budget - held * lot, 0, 0, 0, []
    for day, close, open_price, sellable, buyable in stock["path"]:
        if day > entry_date and target > held and buyable:
            spend = min((target - held) * lot, cash)
            units, cash, held, buys = units + spend * (1 - cost) / Decimal(open_price), cash - spend, target, buys + 1
        elif day > entry_date and target < held and sellable:
            sold = units * (held - target) / held
            cash += sold * Decimal(open_price) * (1 - sell_cost(day))
            units, held, sells = units - sold, target, sells + 1
        close = Decimal(close)
        while level < rule["initial"] and close >= base * ratio ** (level + 1):
            level += 1
        while level > rule["initial"] - rule["lots"] and close <= base * ratio ** (level - 1):
            level -= 1
        target = rule["initial"] - level
        marks.append((day, cash + units * close))
    values = [next(v for d, v in reversed(marks) if d <= edge) for edge in edges]
    return {"values": values, "buys": buys, "sells": sells}


def portfolio_segments(weights, outcome, exits=None, grids=None, cost=BUY_COST):
    """weights：{证券: Decimal}，合计不超过 1，余下为现金（收益为 0）；exits：{证券: 卖出规则}；grids：{证券: 网格}。"""
    require(all(w >= 0 for w in weights.values()) and sum(weights.values()) <= 1, "仓位须非负且合计不超过 1")
    cash, units, blocked, sales, gridded = Decimal(1) - sum(weights.values()), {}, [], {}, {}
    for code, weight in weights.items():
        if not weight:
            continue
        stock = outcome["stocks"][code]
        if not stock["buyable"]:
            cash += weight
            blocked.append(code)
            continue
        if grids and code in grids:
            gridded[code] = simulate_grid(weight, stock, grids[code], outcome["entry_date"], outcome["boundaries"], cost)
            continue
        units[code] = weight * (1 - cost) / Decimal(stock["entry_open"])
        if exits and code in exits:
            sale = simulate_exit(units[code], stock, exits[code], outcome["entry_date"])
            if sale:
                sales[code] = sale
    values = [Decimal(1)]
    for index, day in enumerate(outcome["boundaries"]):
        total = cash + sum(grid["values"][index] for grid in gridded.values())
        for code, amount in units.items():
            sale = sales.get(code)
            if sale and sale["sell_date"] and sale["sell_date"] <= day:
                total += sale["proceeds"]
            else:
                total += amount * Decimal(valuation(outcome["stocks"][code]["path"], day))
        values.append(total)
    returns = [values[i + 1] / values[i] - 1 for i in range(3)]
    sold = [{"symbol": code, **{k: v for k, v in sale.items() if k != "proceeds"}} for code, sale in sorted(sales.items())]
    traded = [{"symbol": code, "buys": grid["buys"], "sells": grid["sells"]} for code, grid in sorted(gridded.items())]
    return returns, blocked, sold, traded


def index_segments(outcome):
    entry = Decimal(outcome["index"]["entry_open"])
    values = [entry] + [Decimal(valuation(outcome["index"]["path"], day)) for day in outcome["boundaries"]]
    return [values[i + 1] / values[i] - 1 for i in range(3)]


def return_score(returns):
    return float(100 * sum(w * r for w, r in zip(SEGMENT_WEIGHTS, returns, strict=True)))


def total_score(value):
    """单期总分；各期平均即为 平均收益得分 + 5 ×（胜率 − 50%）。"""
    return value + WIN_RATE_WEIGHT * ((value > 0) - 0.5)


def parse_weights(rows, samples):
    """返回 ({样本: {证券: 仓位}}, {样本: {证券: 卖出规则}}, {样本: {证券: 网格}})；行可带 exit 或 grid 之一。"""
    pools = {s["id"]: {stock["symbol"] for stock in s["stocks"]} for s in samples}
    output, exits, grids = {}, {}, {}
    for row in rows:
        sample_id, code = row["sample_id"], symbol(row["symbol"])
        require(sample_id in pools and code in pools[sample_id], f"仓位引用未知样本或非股票池证券: {sample_id}/{code}")
        require(isinstance(row["weight"], str), "weight 必须为十进制字符串")
        weight = Decimal(row["weight"])
        require(weight.is_finite() and weight >= 0, "weight 必须为非负有限数")
        require(code not in output.setdefault(sample_id, {}), "仓位键重复")
        output[sample_id][code] = weight
        require(row.get("exit") is None or row.get("grid") is None, f"{sample_id}/{code} 不能同时使用卖出规则与网格")
        if row.get("exit") is not None:
            exits.setdefault(sample_id, {})[code] = exit_rule(row["exit"])
        if row.get("grid") is not None:
            grids.setdefault(sample_id, {})[code] = grid_rule(row["grid"])
    for sample_id, weights in output.items():
        require(sum(weights.values()) <= 1, f"{sample_id} 仓位合计超过 1")
    return output, exits, grids


def bootstrap_interval(values, rounds=2000, seed=20261031):
    if len(values) < 2:
        return None
    generator = random.Random(seed)
    means = sorted(math.fsum(generator.choice(values) for _ in values) / len(values) for _ in range(rounds))
    return [means[int(0.025 * rounds)], means[int(0.975 * rounds) - 1]]


def summarize(values):
    return {"mean": mean(values), "median": median(values), "stdev": pstdev(values),
            "count": len(values), "bootstrap_95": bootstrap_interval(values)} if values else None


def series_stats(periods, prefixes=SERIES):
    """策略与两个对照（或 prefixes 指定的序列）各自的总分、收益得分汇总和胜率。"""
    output = {}
    for prefix in prefixes:
        returns = [p[f"{prefix}return_score"] for p in periods]
        output.update({f"{prefix}score": summarize([p[f"{prefix}score"] for p in periods]),
                       f"{prefix}return_score": summarize(returns),
                       f"{prefix}win_rate": sum(v > 0 for v in returns) / len(returns)})
    return output


def evaluate_weights(samples, outcomes, weight_rows):
    """未提交仓位的样本视为全部持有现金，得分为 0，不能靠跳过样本提高均值。"""
    weights, exits, grids = parse_weights(weight_rows, samples)
    by_id = {row["sample_id"]: row for row in outcomes}
    periods = []
    for sample in samples:
        outcome = by_id.get(sample["id"])
        if not outcome:
            continue
        returns, blocked, sold, traded = portfolio_segments(weights.get(sample["id"], {}), outcome,
                                                            exits.get(sample["id"]), grids.get(sample["id"]))
        equal = {code: Decimal(1) / len(sample["stocks"]) for code in outcome["stocks"]}
        pool_returns = portfolio_segments(equal, outcome)[0]
        period = {
            "sample_id": sample["id"], "pool": sample["pool"], "decision_date": sample["decision_date"],
            "entry_date": outcome["entry_date"], "boundaries": outcome["boundaries"],
            "submitted": sample["id"] in weights, "blocked_entries": blocked, "exits": sold, "grid_trades": traded,
            "segment_returns": [float(r) for r in returns]}
        for prefix, segments in zip(SERIES, (returns, pool_returns, index_segments(outcome)), strict=True):
            value = return_score(segments)
            period.update({f"{prefix}return_score": value, f"{prefix}score": total_score(value)})
        periods.append(period)
    require(periods, "没有可评分的样本")
    by_year, by_pool = {}, {}
    for period in periods:
        by_year.setdefault(period["decision_date"][:4], []).append(period)
        by_pool.setdefault(period["pool"], []).append(period)
    return {
        "metric": METRIC,
        "cost_basis": COST_BASIS,
        **series_stats(periods),
        "beat_csi300_periods": sum(p["return_score"] > p["csi300_return_score"] for p in periods),
        "beat_pool_equal_weight_periods": sum(p["return_score"] > p["pool_equal_weight_return_score"] for p in periods),
        "positive_periods": sum(p["return_score"] > 0 for p in periods),
        "exits_by_reason": {reason: sum(e["reason"] == reason for p in periods for e in p["exits"])
                            for reason in sorted({e["reason"] for p in periods for e in p["exits"]})},
        "grid_trades": {side: sum(t[side] for p in periods for t in p["grid_trades"]) for side in ("buys", "sells")},
        "by_year": {year: series_stats(rows) for year, rows in sorted(by_year.items())},
        "by_pool": {pool: series_stats(rows) for pool, rows in sorted(by_pool.items())},
        "periods": periods,
        "limitations": ["股票池与行业归属尚未人工核对（membership_verified=false）",
                        "退市证券按退市前最后价格估值，未计换股、清算或退市整理期",
                        "后复权价格近似股息再投资，未计红利税与到账时点；沪深300 对照用全收益指数 H00300",
                        "被吸收合并的证券按停牌前最后价格估值，未折算为存续公司股份",
                        "卖出规则与网格只按收盘价触发、次日开盘成交，不模拟盘中止损或盘中网格单；"
                        "2008-09-19 前的买入印花税未计",
                        "LLM 可能记得历史结果；时间截断不能消除预训练数据污染"]}
