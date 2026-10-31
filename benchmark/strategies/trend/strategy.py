from decimal import Decimal


def decide(sample):
    chosen = [s["symbol"] for s in sample["stocks"]
              if s["trade_status"] != "no_bar" and Decimal(s["momentum_63"] or 0) > 0]
    return {code: Decimal(1) / len(chosen) for code in chosen}
