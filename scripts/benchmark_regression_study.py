"""Clenow 90日回归趋势研究；使用独立缓存，不自动发布或安装正式策略。"""

import argparse
import sys
from datetime import date
from itertools import product
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from benchmark.files import read_json, require
from scripts.benchmark_long_history_study import COMMON, build, prune_source, scan
from scripts.benchmark_risk_study import OVERLAY
from benchmark.price_indicators import regression_features, minimal_features


ROOT = Path(__file__).resolve().parents[1]
FIELDS = ("clenow_90", "clenow_r2_90", "clenow_mark_90", "clenow_mark_r2_90", "daily_returns_1m")


SIGNALS = '''
def trend_key():
    return TREND_KEY


def fitness_key():
    return FITNESS_KEY
'''


BODIES = {
    "regression_rank": '''
def decide(sample):
    known = [s for s in eligible(sample, [trend_key(), fitness_key()])
             if signal(s, trend_key()) > 0 and signal(s, fitness_key()) >= Decimal(PARAMS["min_r2"])
             and (not PARAMS["inverse_vol"] or s["volatility_63"] is not None and Decimal(s["volatility_63"]) > 0)]
    chosen = sorted(known, key=lambda s: (-signal(s, trend_key()), s["symbol"]))[:PARAMS["count"]]
    raw = {s["symbol"]: 1 / Decimal(s["volatility_63"]) if PARAMS["inverse_vol"] else Decimal(1) for s in chosen}
    total = sum(raw.values())
    return {k: v / total for k, v in raw.items()} if total else {}
''',
    "regression_calm": '''
def decide(sample):
    known = [s for s in eligible(sample, [trend_key(), fitness_key()])
             if signal(s, trend_key()) > 0 and signal(s, fitness_key()) >= Decimal(PARAMS["min_r2"])
             and s["volatility_63"] is not None]
    trend = sorted(known, key=lambda s: (-signal(s, trend_key()), s["symbol"]))
    calm = sorted(known, key=lambda s: (Decimal(s["volatility_63"]), s["symbol"]))
    ranks = {s["symbol"]: i for i, s in enumerate(trend)}
    risk = {s["symbol"]: i for i, s in enumerate(calm)}
    chosen = sorted(known, key=lambda s: (ranks[s["symbol"]] + risk[s["symbol"]], s["symbol"]))[:PARAMS["count"]]
    return positions(chosen)
''',
}

TILT = '''
def base_positions(sample):
    original = unscaled_positions(sample)
    stocks = {s["symbol"]: s for s in sample["stocks"]}
    known = [code for code in original if signal(stocks[code], trend_key()) is not None]
    ordered = sorted(known, key=lambda code: (signal(stocks[code], trend_key()), code))
    weights = {code: original[code]["weight"] if isinstance(original[code], dict) else original[code] for code in known}
    budget = sum(weights.values())
    ties = {}
    for i, code in enumerate(ordered):
        ties.setdefault(signal(stocks[code], trend_key()), []).append(Decimal(i))
    ranks = {value: sum(indices) / len(indices) for value, indices in ties.items()}
    factors = {code: Decimal("0.5") + Decimal("1.5") * ranks[signal(stocks[code], trend_key())] / (len(ordered) - 1)
               if len(ordered) > 1 else Decimal(1) for code in ordered}
    raw = {code: weights[code] * factors[code] for code in known}
    total = sum(raw.values())
    if not total:
        return {}
    scaled = {code: value / total * budget for code, value in raw.items()}
    return {code: {**original[code], "weight": value} if isinstance(original[code], dict) else value
            for code, value in scaled.items()}
'''

GRIDS = {
    "regression_rank": [dict(count=n, min_r2=r, inverse_vol=v, clock=c)
                        for n, r, v, c in product((1, 3, 5), ("0", "0.5"), (False, True), ("strict", "mark"))],
    "regression_calm": [dict(count=n, min_r2=r, clock=c) for n, r, c in product((1, 3, 5), ("0", "0.5"), ("strict", "mark"))],
    "regression_tilt": [dict(base=b, budget=r, target="0.04", power=2, clock=c)
                        for b, r, c in product(("equal", "size", "trend_core_satellite"), (False, True), ("strict", "mark"))],
}


def source_for(family, parameters):
    require(parameters["clock"] in ("strict", "mark"), "未知回归估值口径")
    keys = ("clenow_mark_90", "clenow_mark_r2_90") if parameters["clock"] == "mark" else ("clenow_90", "clenow_r2_90")
    signals = SIGNALS.replace("TREND_KEY", repr(keys[0])).replace("FITNESS_KEY", repr(keys[1]))
    common = COMMON.replace("PARAMETERS", repr(parameters)) + signals
    if family != "regression_tilt":
        return prune_source(common + BODIES[family])
    base = (ROOT / "benchmark" / "strategies" / parameters["base"] / "strategy.py").read_text(encoding="utf-8")
    require(base.count("def decide(sample):") == 1, "基础策略入口不唯一")
    base = base.replace("def decide(sample):", "def unscaled_positions(sample):")
    ending = OVERLAY if parameters["budget"] else "\ndef decide(sample):\n    return base_positions(sample)\n"
    return prune_source(common + base + TILT + ending)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    builder = sub.add_parser("build")
    builder.add_argument("--history", type=Path, required=True)
    builder.add_argument("--calendar", type=Path, required=True)
    builder.add_argument("--minimal", action="store_true", help="仅增加入榜候选需要的回归信号，供发布评审")
    builder.add_argument("--built-on", default=date.today().isoformat())
    for command in (builder, sub.add_parser("scan")):
        command.add_argument("--release", type=Path, required=True)
        command.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "build":
        config = {"version": read_json(args.release / "config.json")["version"] + "-regression-research-v1",
                  "input_features": "regression-trend-research-v1", "base_comparison_tag": "pool-scoring-v6",
                  "signal_source": "https://cmtassociation.org/wp-content/uploads/2025/01/Clenow-Using-Momentum.pdf",
                  "regression_window_sessions": 90, "annualization_sessions": 252,
                  "suspended_window_policy": "strict=null; mark=last_traded_valuation; missing_bar=null"}
        projection, fields = regression_features, FIELDS
        if args.minimal:
            projection, fields = minimal_features, ("daily_returns_1m", "clenow_mark_90")
            config.update(version=read_json(args.release / "config.json")["version"] + "-monthly-risk-regression-v1",
                          input_features="monthly-risk-regression-v1", built_on=date.fromisoformat(args.built_on).isoformat(),
                          suspended_window_policy="last_traded_valuation; missing_bar=null")
        report = build(args.release, args.history, args.calendar, args.out, projection, fields, config)
        print({k: v for k, v in report.items() if k != "raw_hashes"})
    else:
        report = scan(args.release, args.out, GRIDS, source_for)
        print(f"COMPLETED {len(report['trials'])} trials")


if __name__ == "__main__":
    main()
