from decimal import Decimal

REQUIRES = {
    'pool': 'text',
    'stocks.*.momentum_63': 'decimal?',
    'stocks.*.symbol': 'text',
    'stocks.*.trade_status': 'text',
    'stocks.*.volatility_63': 'decimal?',
}

SATELLITE_THRESHOLD = Decimal("0.15")


def ranks(stocks, key):
    known = sorted((s for s in stocks if key(s) is not None), key=lambda s: (key(s), s["symbol"]))
    result = {s["symbol"]: len(stocks) for s in stocks}
    result.update({s["symbol"]: i for i, s in enumerate(known)})
    return result


def choose(sample, threshold):
    live = [s for s in sample["stocks"] if s["trade_status"] != "no_bar"]
    strong = [s for s in live if s["momentum_63"] is not None and Decimal(s["momentum_63"]) > threshold]
    if not strong:
        known = [s for s in live if s["volatility_63"] is not None]
        return sorted(known, key=lambda s: (Decimal(s["volatility_63"]), s["symbol"]))[:5]
    calm = ranks(strong, lambda s: None if s["volatility_63"] is None else Decimal(s["volatility_63"]))
    trend = ranks(strong, lambda s: -Decimal(s["momentum_63"]))
    ordered = sorted(strong, key=lambda s: (calm[s["symbol"]] + trend[s["symbol"]], s["symbol"]))
    return ordered[:1]


def decide(sample):
    chosen = choose(sample, SATELLITE_THRESHOLD)
    weights = {s["symbol"]: Decimal(1) / len(chosen) for s in chosen}
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
