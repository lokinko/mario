from decimal import Decimal


def decide(sample):
    rising = [s for s in sample["stocks"] if s["trade_status"] != "no_bar"
              and Decimal(s["momentum_63"] or 0) > 0 and s["volatility_63"] is not None]
    chosen = sorted(rising, key=lambda s: (Decimal(s["volatility_63"]), s["symbol"]))[:5]
    return {s["symbol"]: Decimal(1) / len(chosen) for s in chosen}
