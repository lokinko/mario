from decimal import Decimal

REQUIRES = {
    'pool': 'text',
    'stocks.*.momentum_63': 'decimal?',
    'stocks.*.return_20': 'decimal?',
    'stocks.*.sw_level1_name': 'text?',
    'stocks.*.symbol': 'text',
    'stocks.*.trade_status': 'text',
    'stocks.*.volatility_63': 'decimal?',
}

PARAMS = {'target': '0.032'}


def live_stocks(sample):
    return [s for s in sample["stocks"] if s["trade_status"] != "no_bar"]


def value(stock, field):
    return None if stock[field] is None else Decimal(stock[field])


def mid_momentum(stock):
    m, r = value(stock, "momentum_63"), value(stock, "return_20")
    return None if m is None or r is None else (1 + m) / (1 + r) - 1


def calm_fallback(stocks):
    known = [s for s in stocks if s["volatility_63"] is not None]
    return sorted(known, key=lambda s: (value(s, "volatility_63"), s["symbol"]))[:5]


def ranked(stocks, key):
    known = sorted((s for s in stocks if key(s) is not None), key=lambda s: (key(s), s["symbol"]))
    ranks = {s["symbol"]: len(stocks) for s in stocks}
    ranks.update({s["symbol"]: i for i, s in enumerate(known)})
    return ranks


def spread(ordered, count):
    chosen, seen = [], set()
    for stock in ordered:
        industry = stock["sw_level1_name"]
        if industry is not None and industry in seen:
            continue
        chosen.append(stock)
        seen.add(industry)
        if len(chosen) == count:
            break
    return chosen


def original_choose(sample):
    live = live_stocks(sample)
    strong = [s for s in live if s["momentum_63"] is not None and value(s, "momentum_63") > Decimal("0.1")]
    if not strong:
        return calm_fallback(live)
    calm = ranked(strong, lambda s: value(s, "volatility_63"))
    trend = ranked(strong, lambda s: None if mid_momentum(s) is None else -mid_momentum(s))
    ordered = sorted(strong, key=lambda s: (calm[s["symbol"]] + trend[s["symbol"]], s["symbol"]))
    return spread(ordered, 3)


def execution(sample, stock, weight):
    if sample["pool"] == "dividend":
        return {"weight": weight, "exit": {"take_profit": "0.1"}}
    vol = value(stock, "volatility_63")
    if vol is not None and vol < Decimal("0.028"):
        return {"weight": weight, "grid": {"step": "0.08", "lots": 2, "initial": 1}}
    return weight


def equal_positions(sample, chosen, exposure=Decimal(1)):
    weight = exposure / len(chosen) if chosen else Decimal(0)
    return {s["symbol"]: execution(sample, s, weight) for s in chosen}

def decide(sample):
    chosen = original_choose(sample)
    if not chosen:
        return {}
    vols = [value(s, "volatility_63") or Decimal("0.028") for s in chosen]
    typical = sum(vols) / len(vols)
    exposure = min(Decimal(1), Decimal(PARAMS["target"]) / typical)
    return equal_positions(sample, chosen, exposure)
