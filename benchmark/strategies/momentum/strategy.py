from decimal import Decimal


def decide(sample):
    ranked = [s for s in sample["stocks"] if s["trade_status"] != "no_bar" and s["momentum_63"] is not None]
    chosen = sorted(ranked, key=lambda s: (-Decimal(s["momentum_63"]), s["symbol"]))[:5]
    return {s["symbol"]: Decimal(1) / len(chosen) for s in chosen}
