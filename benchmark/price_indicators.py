"""行情衍生指标的确定性计算；不读取文件、网络、策略或未来标签。"""

from datetime import date, timedelta
from decimal import Decimal, localcontext

from .files import require

LOOKBACK = 63
LONG_FIELDS = ("momentum_12m", "momentum_6m_skip1", "momentum_12m_skip1", "momentum_12_2",
               "price_to_sma_10m", "close_to_high_12m", "close_to_max_close_12m", "information_discreteness")
REGRESSION_FIELDS = ("clenow_90", "clenow_r2_90", "clenow_mark_90", "clenow_mark_r2_90")


def momentum(bars, calendar, day, sessions=LOOKBACK):
    index = calendar.index(day)
    if index < sessions:
        return None
    now, before = bars.get(day), bars.get(calendar[index - sessions])
    if not now or not before or "0" in (now.get("tradestatus", "1"), before.get("tradestatus", "1")):
        return None
    return str(Decimal(now["close"]) / Decimal(before["close"]) - 1)

def volatility(bars, calendar, day, sessions=LOOKBACK):
    """最近 sessions 个有成交日的日收益率总体标准差（不年化）；停牌日跳过，不足时为空。"""
    closes = [Decimal(bars[d]["close"]) for d in calendar[:calendar.index(day) + 1]
              if d in bars and bars[d].get("tradestatus", "1") == "1"]
    if len(closes) <= sessions:
        return None
    returns = [closes[i] / closes[i - 1] - 1 for i in range(len(closes) - sessions, len(closes))]
    average = sum(returns) / sessions
    return str((sum((r - average) ** 2 for r in returns) / sessions).sqrt())


def month_key(day, offset=0):
    index = int(day[:4]) * 12 + int(day[5:7]) - 1 + offset
    year, month = divmod(index, 12)
    return f"{year:04}-{month + 1:02}"

def long_features(bars, calendar, decision):
    """所有分母、窗口、月末和观察数严格限制于 decision，不前填缺失价格。"""
    days = [d for d in calendar if d <= decision]
    require(days and days[-1] == decision, "决策日必须在交易日历中")
    ends = {d[:7]: d for d in days}
    anchors = {i: ends.get(month_key(decision, -i)) for i in range(14)}
    output = {key: None for key in LONG_FIELDS}
    issues = {}

    def price(day, field="close"):
        bar = bars.get(day)
        if not bar or bar.get("tradestatus") != "1" or bar.get(field) is None:
            return None
        value = Decimal(bar[field])
        require(value.is_finite() and value > 0, f"无效价格: {day}")
        return value

    def ratio(key, a, b):
        x, y = price(anchors[a]), price(anchors[b])
        if x is not None and y is not None:
            output[key] = str(x / y - 1)
        else:
            issues[key] = "missing_or_suspended_endpoint"

    ratio("momentum_12m", 0, 12)
    ratio("momentum_6m_skip1", 1, 7)
    ratio("momentum_12m_skip1", 1, 13)
    ratio("momentum_12_2", 1, 12)
    monthly = [price(anchors[i]) for i in range(10)]
    if all(v is not None for v in monthly):
        output["price_to_sma_10m"] = str(monthly[0] / (sum(monthly) / 10) - 1)
    else:
        issues["price_to_sma_10m"] = "missing_or_suspended_month_end"

    start = anchors[12]
    window = [d for d in days if start and start < d <= decision]
    complete = start is not None and start in bars and all(d in bars for d in window)
    now = price(decision)
    if complete and now is not None:
        for field, key in (("high", "close_to_high_12m"), ("close", "close_to_max_close_12m")):
            observed = [price(d, field) for d in window if bars[d]["tradestatus"] == "1"]
            if observed and all(p is not None for p in observed):
                output[key] = str(now / max(observed))
    for key in ("close_to_high_12m", "close_to_max_close_12m"):
        if output[key] is None:
            issues[key] = "incomplete_year_or_unavailable_decision_price"

    # FIP 的 12-2 操作定义：P[上月末]/P[12个月前月末]-1，共11个月；不混用13月端点。
    formation_end = anchors[1]
    formation = [d for d in window if formation_end and d <= formation_end]
    if complete and output["momentum_12_2"] is not None:
        previous = price(start)
        signs = []
        for day in formation:
            current = price(day)
            if current is not None:
                if previous is not None:
                    signs.append((current > previous) - (current < previous))
                previous = current
        if signs:
            pret = Decimal(output["momentum_12_2"])
            sign = (pret > 0) - (pret < 0)
            output["information_discreteness"] = str(-Decimal(sign) * sum(signs) / len(signs))
            output["fip_observations"] = len(signs)
    if output["information_discreteness"] is None:
        issues["information_discreteness"] = "incomplete_formation_or_missing_endpoint"
    return {**output, "issues": issues}


def monthly_features(bars, calendar, decision):
    """新增正式输入只需要这个月内历史收益契约，不携带其他失败候选的特征。"""
    days = [d for d in calendar if d <= decision]
    previous = None
    returns = {}
    for day in days:
        bar = bars.get(day)
        if bar is None:
            value, previous = None, None
        elif bar["tradestatus"] == "0":
            value = Decimal(0) if previous is not None else None
        else:
            current = Decimal(bar["close"])
            value = current / previous - 1 if previous is not None else None
            previous = current
        if day[:7] == decision[:7]:
            returns[day] = str(value) if value is not None else None
    issues = {"daily_returns_1m": "missing_monthly_daily_return"} if any(v is None for v in returns.values()) else {}
    return {"daily_returns_1m": returns, "issues": issues}

def weekly_features(bars, calendar, decision):
    output = {"issues": {}}
    days = [d for d in calendar if d <= decision]
    cutoff = date.fromisoformat(decision)
    try:
        start = cutoff.replace(year=cutoff.year - 3)
    except ValueError:
        start = cutoff.replace(year=cutoff.year - 3, day=28)
    # 固定周五网格；节假日用截至周五的最后一个市场交易日，决策后的周五不使用。
    friday = start - timedelta(days=(start.weekday() - 4) % 7)
    endpoints = []
    index = 0
    while friday <= cutoff:
        while index < len(days) and days[index] <= friday.isoformat():
            index += 1
        endpoints.append(days[index - 1] if index else None)
        friday += timedelta(days=7)
    prices = []
    for day in endpoints:
        bar = bars.get(day)
        prices.append(Decimal(bar["close"]) if bar and bar["tradestatus"] == "1" else None)
    output["volatility_3y_weekly"] = None
    if len(prices) >= 156 and all(p is not None and p.is_finite() and p > 0 for p in prices):
        returns = [b / a - 1 for a, b in zip(prices, prices[1:])]
        average = sum(returns) / len(returns)
        # 总体标准差 × sqrt(52)，不偷偷换成63日或缩短上市史。
        output["volatility_3y_weekly"] = str((sum((r - average) ** 2 for r in returns) / len(returns) * 52).sqrt())
    else:
        output["issues"]["volatility_3y_weekly"] = "incomplete_three_year_weekly_endpoints"

    return output


def risk_features(bars, calendar, decision):
    output = long_features(bars, calendar, decision)
    for addition in (weekly_features(bars, calendar, decision), monthly_features(bars, calendar, decision)):
        output["issues"].update(addition["issues"])
        output.update({k: v for k, v in addition.items() if k != "issues"})
    return output


def regression_features(bars, calendar, decision):
    days = [d for d in calendar if d <= decision]
    require(days and days[-1] == decision, "决策日必须是交易日")
    window = days[-90:]
    output = monthly_features(bars, calendar, decision)
    output.update({key: None for key in REGRESSION_FIELDS[:4]})
    if len(window) != 90 or any(d not in bars for d in window):
        output["issues"].update({key: "incomplete_90_session_window" for key in REGRESSION_FIELDS[:4]})
        return output
    # 敏感性口径仅在明确停牌时沿用上次真实成交价，与v6月内估值相同；缺行会重置。
    previous, marked = None, []
    for day in days:
        bar = bars.get(day)
        if bar is None:
            previous = None
        elif bar["tradestatus"] == "1":
            previous = Decimal(bar["close"])
            require(previous.is_finite() and previous > 0, "回归历史存在无效价格")
        if day >= window[0]:
            marked.append(previous)
    if any(value is None for value in marked):
        output["issues"].update({key: "unavailable_90_session_valuation" for key in REGRESSION_FIELDS[2:4]})
    else:
        output["clenow_mark_90"], output["clenow_mark_r2_90"] = regression(marked)
    if any(bars[d]["tradestatus"] != "1" for d in window):
        output["issues"].update({key: "suspended_regression_window" for key in REGRESSION_FIELDS[:2]})
    else:
        output["clenow_90"], output["clenow_r2_90"] = output["clenow_mark_90"], output["clenow_mark_r2_90"]
    return output

def regression(prices):
    with localcontext() as context:
        context.prec = 40
        require(all(p.is_finite() and p > 0 for p in prices), "回归窗口存在无效价格")
        # 相对首日价格取log，避免价格单位改变时大截距造成消减误差。
        logs = [(p / prices[0]).ln() for p in prices]
        average = sum(logs) / 90
        x = [Decimal(i) - Decimal("44.5") for i in range(90)]
        xx = sum(v * v for v in x)
        yy = sum((v - average) ** 2 for v in logs)
        xy = sum(a * (b - average) for a, b in zip(x, logs))
        slope = xy / xx
        r2 = min(Decimal(1), max(Decimal(0), xy * xy / (xx * yy))) if yy else Decimal(0)
        return str(((slope * 252).exp() - 1) * 100 * r2), str(r2)

def minimal_features(bars, calendar, decision):
    output = regression_features(bars, calendar, decision)
    return {"daily_returns_1m": output["daily_returns_1m"], "clenow_mark_90": output["clenow_mark_90"],
            "issues": {key: value for key, value in output["issues"].items()
                       if key in ("daily_returns_1m", "clenow_mark_90")}}
