from decimal import Decimal


def decide(sample):
    # 仅测红利池技术底仓，红利成分不能证明股息率、国资属性或分红连续性。
    if sample["pool"] != "dividend":
        return {}
    chosen = []
    for stock in sample["stocks"]:
        signal = stock["fugui"]
        if stock["trade_status"] != "trading" or Decimal(stock["market_cap_at_selection"]) < Decimal("100000000000"):
            continue
        if stock["volatility_63"] is None or Decimal(stock["volatility_63"]) > Decimal("0.025"):
            continue
        if signal["monthly_low"] is True or (signal["first_pullback"] is True
                and signal["daily_lower"] is True and signal["weekly_lower"] is True):
            chosen.append(stock)
    chosen.sort(key=lambda s: (Decimal(s["volatility_63"]), s["symbol"]))
    # 每只18%底仓、最多五只，空缺预算和10%机动资金留现金。
    return {s["symbol"]: Decimal("0.18") for s in chosen[:5]}
