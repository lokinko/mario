from decimal import Decimal


def decide(sample):
    stocks = [s for s in sample["stocks"] if s["trade_status"] != "no_bar"]
    chosen = sorted(stocks, key=lambda s: (-Decimal(s["market_cap_at_selection"]), s["symbol"]))[:5]
    return {s["symbol"]: Decimal(1) / len(chosen) for s in chosen}
