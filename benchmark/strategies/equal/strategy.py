from decimal import Decimal


def decide(sample):
    stocks = [s["symbol"] for s in sample["stocks"] if s["trade_status"] != "no_bar"]
    return {code: Decimal(1) / len(stocks) for code in stocks}
