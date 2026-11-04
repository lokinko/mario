"""BOLL、MACD 与富贵技术条件；只消费截止时点前的行情。"""

from datetime import date
from decimal import Decimal

from .files import require

FIELDS = ("monthly_low", "first_pullback", "daily_lower", "weekly_lower", "monthly_upper_return",
          "monthly_macd_negative", "monthly_macd_diff", "monthly_macd_signal", "monthly_macd_histogram")


def boll(prices):
    if len(prices) < 20 or any(p is None for p in prices[-20:]):
        return None
    window = prices[-20:]
    middle = sum(window) / 20
    deviation = (sum((p - middle) ** 2 for p in window) / 20).sqrt()
    return middle - 2 * deviation, middle, middle + 2 * deviation

def macd(prices):
    """首个收盘初始化 EMA；至少 35 根月线，固定种子，不按回测表现选择。"""
    if len(prices) < 35 or any(p is None for p in prices):
        return None
    fast = slow = prices[0]
    signal = Decimal(0)
    for price in prices[1:]:
        fast += Decimal(2) / 13 * (price - fast)
        slow += Decimal(2) / 27 * (price - slow)
        diff = fast - slow
        signal += Decimal(2) / 10 * (diff - signal)
    return diff, signal, 2 * (diff - signal)

def fugui_features(bars, calendar, decision):
    """只读截至决策日的行情。未知缺行切断历史，明确停牌沿用此前已知收盘。"""
    days = [d for d in calendar if d <= decision]
    require(days and days[-1] == decision, "决策日不在交易日历中")
    monthly, weekly, daily = {}, {}, []
    previous = None
    for day in days:
        bar = bars.get(day)
        if bar is None:
            previous = None
            # 缺行会影响所在周期的完整性，不能只凭月底价格覆盖它。
            monthly[day[:7]] = None
            weekly[date.fromisoformat(day).isocalendar()[:2]] = None
            daily.append(None)
            continue
        if bar["tradestatus"] == "1":
            previous = Decimal(bar["close"])
            require(previous.is_finite() and previous > 0, "无效历史价格")
        week = date.fromisoformat(day).isocalendar()[:2]
        for groups, key in ((monthly, day[:7]), (weekly, week)):
            if key not in groups or groups[key] is not None:
                groups[key] = previous
        daily.append(previous)

    # 月 MACD 使用最后一个未知周期之后的连续月线，不跨缺口初始化。
    months = list(monthly.values())
    start = max((i + 1 for i, p in enumerate(months) if p is None), default=0)
    months = months[start:]
    weeks = list(weekly.values())
    output = {k: None for k in FIELDS}
    output.update(months=len(months), issues={})
    now = daily[-1]
    month_band, week_band, day_band = boll(months), boll(weeks), boll(daily)
    monthly_macd = macd(months)
    if monthly_macd is not None:
        output["monthly_macd_negative"] = all(value < 0 for value in monthly_macd)
        for key, value in zip(("monthly_macd_diff", "monthly_macd_signal", "monthly_macd_histogram"), monthly_macd):
            output[key] = str(value)
    if now is not None:
        for key, band in (("daily_lower", day_band), ("weekly_lower", week_band)):
            if band:
                output[key] = now <= band[0] + now * Decimal("0.05")
        if month_band:
            output["monthly_upper_return"] = str(month_band[2] / now - 1)
    if len(months) >= 36 and monthly_macd is not None and now is not None:
        previous_band = boll(months[:-1])
        high, low = max(months[-36:]), min(months[-36:])
        flat = abs(month_band[0] - previous_band[0]) <= now * Decimal("0.02")
        near_low = now <= low + (high - low) * Decimal("0.2")
        underwater = all(value < 0 for value in monthly_macd)
        output["monthly_low"] = now <= high * Decimal("0.5") and near_low and flat and underwater
    if len(months) >= 35 and now is not None:
        # 对20月与30月均线分别找最近6个月的突破；随后第一次进入支撑±5%才算回踩。
        hit = False
        for period in (20, 30):
            averages = [sum(months[i - period + 1:i + 1]) / period if i >= period - 1 else None
                        for i in range(len(months))]
            crosses = [i for i in range(max(period, len(months) - 7), len(months) - 1)
                       if months[i - 1] <= averages[i - 1] and months[i] > averages[i]]
            if not crosses:
                continue
            cross = crosses[-1]
            first = all(months[i] > averages[i] * Decimal("1.05") for i in range(cross + 1, len(months) - 1))
            support = averages[-1]
            hit |= first and support * Decimal("0.95") <= now <= support * Decimal("1.05")
        output["first_pullback"] = hit
    for key in FIELDS:
        if output[key] is None:
            output["issues"][key] = "insufficient_contiguous_history"
    return output
