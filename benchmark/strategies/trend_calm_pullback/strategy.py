from decimal import Decimal


def rank_sum(stocks, fields):
    total = {s["symbol"]: 0 for s in stocks}
    for field in fields:
        known = sorted((s for s in stocks if s[field] is not None), key=lambda s: (Decimal(s[field]), s["symbol"]))
        for index, s in enumerate(known):
            total[s["symbol"]] += index
        for s in stocks:
            if s[field] is None:
                total[s["symbol"]] += len(stocks)
    return total


def choose(sample):
    live = [s for s in sample["stocks"] if s["trade_status"] != "no_bar"]
    rising = [s for s in live if Decimal(s["momentum_63"] or 0) > 0]
    if rising:
        ranks = rank_sum(rising, ("volatility_63", "return_20"))
        return sorted(rising, key=lambda s: (ranks[s["symbol"]], s["symbol"]))[:5]
    calm = [s for s in live if s["volatility_63"] is not None]
    return sorted(calm, key=lambda s: (Decimal(s["volatility_63"]), s["symbol"]))[:5]


def decide(sample):
    chosen = choose(sample)
    return {s["symbol"]: Decimal(1) / len(chosen) for s in chosen}
