from decimal import Decimal

REQUIRES = {
    'pool': 'text',
    'stocks.*.long_history.daily_returns_1m': 'returns',
    'stocks.*.momentum_63': 'decimal?',
    'stocks.*.return_20': 'decimal?',
    'stocks.*.sw_level1_name': 'text?',
    'stocks.*.symbol': 'text',
    'stocks.*.trade_status': 'text',
    'stocks.*.volatility_63': 'decimal?',
}

CORE_WEIGHT = Decimal("0.25")
SATELLITE_THRESHOLD = Decimal("0.15")


def ranks(stocks, key):
    known = sorted((s for s in stocks if key(s) is not None), key=lambda s: (key(s), s["symbol"]))
    result = {s["symbol"]: len(stocks) for s in stocks}
    result.update({s["symbol"]: i for i, s in enumerate(known)})
    return result


def mid_trend(stock):
    if stock["return_20"] is None:
        return None
    return -((1 + Decimal(stock["momentum_63"])) / (1 + Decimal(stock["return_20"])) - 1)


def choose(sample, threshold, count, core):
    live = [s for s in sample["stocks"] if s["trade_status"] != "no_bar"]
    strong = [s for s in live if s["momentum_63"] is not None and Decimal(s["momentum_63"]) > threshold]
    if not strong:
        known = [s for s in live if s["volatility_63"] is not None]
        return sorted(known, key=lambda s: (Decimal(s["volatility_63"]), s["symbol"]))[:5]
    calm = ranks(strong, lambda s: None if s["volatility_63"] is None else Decimal(s["volatility_63"]))
    trend = ranks(strong, mid_trend if core else lambda s: -Decimal(s["momentum_63"]))
    ordered = sorted(strong, key=lambda s: (calm[s["symbol"]] + trend[s["symbol"]], s["symbol"]))
    chosen, seen = [], set()
    for stock in ordered:
        industry = stock["sw_level1_name"]
        if core and industry is not None and industry in seen:
            continue
        chosen.append(stock)
        seen.add(industry)
        if len(chosen) == count:
            break
    return chosen


def base_positions(sample):
    sleeves = ((CORE_WEIGHT, choose(sample, Decimal("0.1"), 3, True)),
               (1 - CORE_WEIGHT, choose(sample, SATELLITE_THRESHOLD, 1, False)))
    weights = {}
    for budget, chosen in sleeves:
        if budget and chosen:
            for stock in chosen:
                code = stock["symbol"]
                weights[code] = weights.get(code, Decimal(0)) + budget / len(chosen)
    stocks = {s["symbol"]: s for s in sample["stocks"]}
    output = {}
    for code, weight in weights.items():
        stock = stocks[code]
        if sample["pool"] == "dividend":
            output[code] = {"weight": weight, "exit": {"take_profit": "0.1"}}
        elif stock["volatility_63"] is not None and Decimal(stock["volatility_63"]) < Decimal("0.028"):
            output[code] = {"weight": weight, "grid": {"step": "0.08", "lots": 2, "initial": 1}}
        else:
            output[code] = weight
    return output

PARAMS = {'base': 'trend_core_satellite', 'target': '0.04', 'power': 2}

def decide(sample):
    original = base_positions(sample)
    if not original:
        return {}
    weights = {k: v["weight"] if isinstance(v, dict) else v for k, v in original.items()}
    total = sum(weights.values())
    stocks = {s["symbol"]: s for s in sample["stocks"]}
    histories = {code: stocks[code]["long_history"]["daily_returns_1m"] for code in weights}
    dates = sorted(histories[next(iter(weights))])
    if not dates or any(set(h) != set(dates) or any(v is None for v in h.values()) for h in histories.values()):
        return {}
    basket = [sum(weight / total * Decimal(histories[code][day]) for code, weight in weights.items()) for day in dates]
    average = sum(basket) / len(basket)
    variance = sum((v - average) ** 2 for v in basket) / len(basket)
    target = Decimal(PARAMS["target"])
    exposure = min(Decimal(1), (target / variance.sqrt()) ** PARAMS["power"]) if variance else Decimal(1)
    return {code: {**v, "weight": v["weight"] * exposure} if isinstance(v, dict) else v * exposure
            for code, v in original.items()}
