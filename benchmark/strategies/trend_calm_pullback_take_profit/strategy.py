from decimal import Decimal

REQUIRES = {
    'pool': 'text',
    'stocks.*.momentum_63': 'decimal?',
    'stocks.*.return_20': 'decimal?',
    'stocks.*.symbol': 'text',
    'stocks.*.trade_status': 'text',
    'stocks.*.volatility_63': 'decimal?',
}

CALM = Decimal("0.028")
TAKE_PROFIT = {"take_profit": "0.08"}


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


def takes_profit(sample, stock):
    if sample["pool"] == "dividend":
        return True
    return stock["volatility_63"] is not None and Decimal(stock["volatility_63"]) < CALM


def decide(sample):
    chosen = choose(sample)
    weight = Decimal(1) / len(chosen) if chosen else 0
    return {s["symbol"]: {"weight": weight, "exit": TAKE_PROFIT} if takes_profit(sample, s) else weight
            for s in chosen}
