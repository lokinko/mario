"""三年周波动与组合风险研究：只用独立缓存日线，不使用标签构造输入。"""

import argparse
import sys
from datetime import date, timedelta
from decimal import Decimal
from itertools import product
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from scripts.benchmark_long_history_study import COMMON, FIELDS, build, features, prune_source, scan
from benchmark.files import read_json


RISK_FIELDS = (*FIELDS, "volatility_3y_weekly", "daily_returns_1m")
ROOT = Path(__file__).resolve().parents[1]


def monthly_features(bars, calendar, decision):
    """新增正式输入只需要这个月内历史收益契约，不携带其他失败候选的特征。"""
    days = [d for d in calendar if d <= decision]
    previous = None
    returns = {}
    for day in days:
        bar = bars.get(day)
        if bar is None:
            value, previous = None, None
        elif bar["tradestatus"] == "0":
            value = Decimal(0) if previous is not None else None
        else:
            current = Decimal(bar["close"])
            value = current / previous - 1 if previous is not None else None
            previous = current
        if day[:7] == decision[:7]:
            returns[day] = str(value) if value is not None else None
    issues = {"daily_returns_1m": "missing_monthly_daily_return"} if any(v is None for v in returns.values()) else {}
    return {"daily_returns_1m": returns, "issues": issues}


def risk_features(bars, calendar, decision):
    output = features(bars, calendar, decision)
    days = [d for d in calendar if d <= decision]
    cutoff = date.fromisoformat(decision)
    try:
        start = cutoff.replace(year=cutoff.year - 3)
    except ValueError:
        start = cutoff.replace(year=cutoff.year - 3, day=28)
    # 固定周五网格；节假日用截至周五的最后一个市场交易日，决策后的周五不使用。
    friday = start - timedelta(days=(start.weekday() - 4) % 7)
    endpoints = []
    index = 0
    while friday <= cutoff:
        while index < len(days) and days[index] <= friday.isoformat():
            index += 1
        endpoints.append(days[index - 1] if index else None)
        friday += timedelta(days=7)
    prices = []
    for day in endpoints:
        bar = bars.get(day)
        prices.append(Decimal(bar["close"]) if bar and bar["tradestatus"] == "1" else None)
    output["volatility_3y_weekly"] = None
    if len(prices) >= 156 and all(p is not None and p.is_finite() and p > 0 for p in prices):
        returns = [b / a - 1 for a, b in zip(prices, prices[1:])]
        average = sum(returns) / len(returns)
        # 总体标准差 × sqrt(52)，不偷偷换成63日或缩短上市史。
        output["volatility_3y_weekly"] = str((sum((r - average) ** 2 for r in returns) / len(returns) * 52).sqrt())
    else:
        output["issues"]["volatility_3y_weekly"] = "incomplete_three_year_weekly_endpoints"

    monthly = monthly_features(bars, calendar, decision)
    output["daily_returns_1m"] = monthly["daily_returns_1m"]
    output["issues"].update(monthly["issues"])
    return output


MSCI = '''
def standardize(values):
    average = sum(values.values()) / len(values)
    deviation = (sum((v - average) ** 2 for v in values.values()) / len(values)).sqrt()
    return {k: (v - average) / deviation if deviation else Decimal(0) for k, v in values.items()}


def decide(sample):
    known = [s for s in eligible(sample, ["momentum_6m_skip1", "volatility_3y_weekly"])
             if signal(s, "volatility_3y_weekly") > 0]
    if not known:
        return {}
    short = standardize({s["symbol"]: signal(s, "momentum_6m_skip1") / signal(s, "volatility_3y_weekly") for s in known})
    long = {s["symbol"]: signal(s, "momentum_12m_skip1") / signal(s, "volatility_3y_weekly")
            for s in known if signal(s, "momentum_12m_skip1") is not None}
    long = standardize(long) if long else {}
    combined = standardize({s["symbol"]: (short[s["symbol"]] + long[s["symbol"]]) / 2
                           if s["symbol"] in long else short[s["symbol"]] for s in known})
    chosen = sorted((s for s in known if combined[s["symbol"]] > 0),
                    key=lambda s: (-combined[s["symbol"]], s["symbol"]))[:PARAMS["count"]]
    raw = {s["symbol"]: (1 + min(Decimal(3), combined[s["symbol"]])) *
           (Decimal(s["market_cap_at_selection"]) if PARAMS["capitalization"] else 1) for s in chosen}
    total = sum(raw.values())
    return {k: v / total for k, v in raw.items()} if total else {}
'''

TSMOM = '''
def decide(sample):
    known = [s for s in eligible(sample, ["momentum_12m", "volatility_3y_weekly"])
             if signal(s, "volatility_3y_weekly") > 0]
    inverse = {s["symbol"]: 1 / signal(s, "volatility_3y_weekly") for s in known}
    total = sum(inverse.values())
    return {s["symbol"]: inverse[s["symbol"]] / total for s in known if signal(s, "momentum_12m") > 0}
'''

LOWVOL = '''
def decide(sample):
    known = eligible(sample, ["volatility_3y_weekly"])
    safe = sorted(known, key=lambda s: (signal(s, "volatility_3y_weekly"), s["symbol"]))[:max(1, len(known) // 2)]
    rising = [s for s in safe if s["momentum_63"] is not None and Decimal(s["momentum_63"]) > 0]
    chosen = sorted(rising, key=lambda s: (-Decimal(s["momentum_63"]), s["symbol"]))[:PARAMS["count"]]
    return positions(chosen)
'''

OVERLAY = '''
def decide(sample):
    original = base_positions(sample)
    if not original:
        return {}
    weights = {k: v["weight"] if isinstance(v, dict) else v for k, v in original.items()}
    total = sum(weights.values())
    stocks = {s["symbol"]: s for s in sample["stocks"]}
    histories = {code: stocks[code]["long_history"]["daily_returns_1m"] for code in weights}
    dates = sorted(histories[next(iter(weights))])
    if not dates or any(set(h) != set(dates) or any(v is None for v in h.values()) for h in histories.values()):
        return {}
    basket = [sum(weight / total * Decimal(histories[code][day]) for code, weight in weights.items()) for day in dates]
    average = sum(basket) / len(basket)
    variance = sum((v - average) ** 2 for v in basket) / len(basket)
    target = Decimal(PARAMS["target"])
    exposure = min(Decimal(1), (target / variance.sqrt()) ** PARAMS["power"]) if variance else Decimal(1)
    return {code: {**v, "weight": v["weight"] * exposure} if isinstance(v, dict) else v * exposure
            for code, v in original.items()}
'''

GRIDS = {
    "msci_long_momentum": [dict(count=n, capitalization=c) for n, c in product((1, 3, 5, 10), (True, False))],
    "time_series_12m": [{}],
    "long_lowrisk_momentum": [dict(count=n) for n in (1, 3, 5, 10)],
    "basket_risk_overlay": [dict(base=b, target=t, power=p) for b, t, p in product(
        ("trend_calm_pullback_grid", "trend_core_satellite"),
        ("0.008", "0.012", "0.016", "0.02", "0.024", "0.028", "0.032", "0.04"), (1, 2))],
}


def source_for(family, parameters):
    common = COMMON.replace("PARAMETERS", repr(parameters))
    if family == "basket_risk_overlay":
        # 只在研究主进程读取已有规则，随后将自包含源码交给隔离子进程。
        base = (ROOT / "benchmark" / "strategies" / parameters["base"] / "strategy.py").read_text(encoding="utf-8")
        return base.replace("def decide(sample):", "def base_positions(sample):") + "\nPARAMS = " + repr(parameters) + "\n" + OVERLAY
    body = {"msci_long_momentum": MSCI, "time_series_12m": TSMOM, "long_lowrisk_momentum": LOWVOL}[family]
    return prune_source(common + body)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    builder = sub.add_parser("build")
    builder.add_argument("--history", type=Path, required=True)
    builder.add_argument("--calendar", type=Path, required=True)
    builder.add_argument("--minimal", action="store_true", help="仅新增风险预算需要的月内收益，准备正式输入升级")
    builder.add_argument("--built-on", default=date.today().isoformat(), help="最小切片的构建日期，复现时固定")
    for command in (builder, sub.add_parser("scan")):
        command.add_argument("--release", type=Path, required=True)
        command.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "build":
        projection, fields = (monthly_features, ("daily_returns_1m",)) if args.minimal else (risk_features, RISK_FIELDS)
        config = {"input_features": "monthly-risk-v1", "built_on": date.fromisoformat(args.built_on).isoformat(),
                  "version": read_json(args.release / "config.json")["version"] + "-monthly-risk-v1"} if args.minimal else None
        report = build(args.release, args.history, args.calendar, args.out, projection, fields, config)
        print({k: v for k, v in report.items() if k != "raw_hashes"})
    else:
        report = scan(args.release, args.out, GRIDS, source_for)
        print(f"COMPLETED {len(report['trials'])} trials")


if __name__ == "__main__":
    main()
