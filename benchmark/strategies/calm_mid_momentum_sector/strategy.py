from decimal import Decimal

MOMENTUM = Decimal("0.1")
CALM = Decimal("0.028")
TAKE_PROFIT = {"take_profit": "0.1"}
GRID = {"step": "0.08", "lots": 2, "initial": 1}


def mid_momentum(stock):
    if stock["return_20"] is None:
        return None
    return (1 + Decimal(stock["momentum_63"])) / (1 + Decimal(stock["return_20"])) - 1


def rank_sum(stocks, keys):
    total = {s["symbol"]: 0 for s in stocks}
    for key in keys:
        known = sorted((s for s in stocks if key(s) is not None), key=lambda s: (key(s), s["symbol"]))
        for index, s in enumerate(known):
            total[s["symbol"]] += index
        for s in stocks:
            if key(s) is None:
                total[s["symbol"]] += len(stocks)
    return total


def one_per_industry(ordered, count):
    seen, chosen = set(), []
    for s in ordered:
        industry = s["sw_level1_name"]
        if industry is not None and industry in seen:
            continue
        seen.add(industry)
        chosen.append(s)
        if len(chosen) == count:
            break
    return chosen


def choose(sample):
    live = [s for s in sample["stocks"] if s["trade_status"] != "no_bar"]
    strong = [s for s in live if s["momentum_63"] is not None and Decimal(s["momentum_63"]) > MOMENTUM]
    if strong:
        calm = lambda s: None if s["volatility_63"] is None else Decimal(s["volatility_63"])
        trend = lambda s: None if mid_momentum(s) is None else -mid_momentum(s)
        ranks = rank_sum(strong, (calm, trend))
        return one_per_industry(sorted(strong, key=lambda s: (ranks[s["symbol"]], s["symbol"])), 3)
    known = [s for s in live if s["volatility_63"] is not None]
    return sorted(known, key=lambda s: (Decimal(s["volatility_63"]), s["symbol"]))[:5]


def position(sample, stock, weight):
    if sample["pool"] == "dividend":
        return {"weight": weight, "exit": TAKE_PROFIT}
    if stock["volatility_63"] is not None and Decimal(stock["volatility_63"]) < CALM:
        return {"weight": weight, "grid": GRID}
    return weight


def decide(sample):
    chosen = choose(sample)
    weight = Decimal(1) / len(chosen) if chosen else 0
    return {s["symbol"]: position(sample, s, weight) for s in chosen}
