"""离线长窗口信号研究；不采集网络数据、不上传，也不修改正式 v5。

build 从独立日线缓存生成不可覆盖的研究切片；scan 比较规则并仅输出临时候选。
研究者可读取结果标签评分；信号计算与生成仓位的子进程均不读取标签。
"""

import argparse
import ast
import copy
import json
import shutil
import sys
from collections import Counter
from decimal import ROUND_DOWN, Decimal
from functools import lru_cache
from itertools import product
from pathlib import Path
from statistics import mean

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from benchmark.files import digest, new_directory, read_json, require, rows, write_json, write_rows
from benchmark.pool_release import momentum
from benchmark.release import verify_release
from benchmark.scoring import portfolio_segments
from benchmark.price_indicators import long_features as features
from benchmark.raw_data import load_history


FIELDS = ("momentum_12m", "momentum_6m_skip1", "momentum_12m_skip1", "momentum_12_2",
          "price_to_sma_10m", "close_to_high_12m", "close_to_max_close_12m", "information_discreteness")


def build(release, history, calendar_path, destination, projection=features, fields=FIELDS, config_updates=None):
    verify_release(release)
    samples = rows(release / "inputs.jsonl")
    symbols = {s["symbol"] for sample in samples for s in sample["stocks"]}
    request, all_bars, hashes = load_history(history, symbols)
    calendar = read_json(calendar_path)
    calendar = [f"{d[:4]}-{d[4:6]}-{d[6:]}" if len(d) == 8 else d for d in calendar]
    require(calendar == sorted(set(calendar)), "交易日历必须升序且不重复")
    comparisons = Counter()
    for code, bars in all_bars.items():
        start, end = min(bars), max(bars)
        require(all(d in bars for d in calendar if start <= d <= end), f"历史内部缺少交易日: {code}")
    enriched = copy.deepcopy(samples)
    coverage = Counter()
    cache = {}
    for sample in enriched:
        decision = sample["decision_date"]
        for stock in sample["stocks"]:
            bars = all_bars[stock["symbol"]]
            require(stock["trade_status"] != "trading" or decision in bars, "截断历史未到已知交易决策日")
            for field, sessions in (("return_20", 20), ("momentum_63", 63)):
                if stock[field] is not None:
                    require(momentum(bars, calendar, decision, sessions) == stock[field], f"共同字段不一致: {sample['id']} {stock['symbol']} {field}")
                    comparisons[field] += 1
            key = stock["symbol"], decision
            if key not in cache:
                cache[key] = projection(bars, calendar, decision)
            stock["long_history"] = {**stock.get("long_history", {}), **cache[key]}
            coverage.update(k for k in fields if stock["long_history"][k] is not None)
    with new_directory(destination) as stage:
        # 标签只在所有特征生成后复制；不读其中的未来路径，也不从中取历史行情。
        for path in release.rglob("*"):
            relative = path.relative_to(release)
            if path.is_file() and relative.as_posix() not in ("manifest.json", "inputs.jsonl"):
                (stage / relative).parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(path, stage / relative)
        write_rows(stage / "inputs.jsonl", enriched)
        if config_updates:
            write_json(stage / "config.json", {**read_json(stage / "config.json"), **config_updates})
        report = {"research_only": True, "source_manifest_sha256": digest(release / "manifest.json"),
                  "calendar_sha256": digest(calendar_path), "raw_hashes": hashes,
                  "source_rows": sum(map(len, all_bars.values())), "samples": len(enriched),
                  "stock_decisions": sum(len(s["stocks"]) for s in enriched),
                  "common_field_comparisons": dict(comparisons), "feature_coverage": dict(coverage),
                  "outcomes_unchanged": digest(stage / "outcomes.jsonl") == digest(release / "outcomes.jsonl")}
        write_json(stage / "long-history-study.json", report)
        write_json(stage / "manifest.json", {"schema_version": 1, "files": {
            p.relative_to(stage).as_posix(): digest(p) for p in sorted(stage.rglob("*")) if p.is_file()}})
    verify_release(destination)
    return report


COMMON = '''from decimal import Decimal
PARAMS = PARAMETERS


def signal(stock, key):
    value = stock["long_history"][key]
    return None if value is None else Decimal(value)


def eligible(sample, keys):
    return [s for s in sample["stocks"] if s["trade_status"] != "no_bar"
            and all(signal(s, k) is not None for k in keys)]


def positions(chosen, budget=Decimal(1)):
    return {s["symbol"]: budget / len(chosen) for s in chosen} if chosen else {}
'''

BODIES = {
    "dual_momentum_12m": '''
def decide(sample):
    known = eligible(sample, ["momentum_12m"])
    chosen = sorted(known, key=lambda s: (-signal(s, "momentum_12m"), s["symbol"]))[:PARAMS["count"]]
    return {s["symbol"]: Decimal(1) / PARAMS["count"] for s in chosen if signal(s, "momentum_12m") > 0}
''',
    "sma_10m_filter": '''
def decide(sample):
    known = eligible(sample, ["price_to_sma_10m"])
    # 各股票固定1/20预算，不把低于均线者的风险预算重新分配给上涨股票。
    return {s["symbol"]: Decimal(1) / len(sample["stocks"]) for s in known if signal(s, "price_to_sma_10m") > 0}
''',
    "high_52week": '''
def decide(sample):
    known = eligible(sample, [PARAMS["field"]])
    chosen = sorted(known, key=lambda s: (-signal(s, PARAMS["field"]), s["symbol"]))[:PARAMS["count"]]
    return positions(chosen)
''',
    "frog_in_pan": '''
def decide(sample):
    known = eligible(sample, ["momentum_12_2", "information_discreteness"])
    winners = sorted(known, key=lambda s: (-signal(s, "momentum_12_2"), s["symbol"]))
    count = max(1, int(Decimal(len(winners)) * Decimal(PARAMS["winner_fraction"])))
    winners = [s for s in winners[:count] if signal(s, "momentum_12_2") > 0]
    chosen = sorted(winners, key=lambda s: (signal(s, "information_discreteness"), s["symbol"]))[:PARAMS["count"]]
    return positions(chosen)
''',
    "high_calm_rank": '''
def decide(sample):
    known = [s for s in eligible(sample, [PARAMS["field"]]) if s["volatility_63"] is not None]
    near = sorted(known, key=lambda s: (-signal(s, PARAMS["field"]), s["symbol"]))
    calm = sorted(known, key=lambda s: (Decimal(s["volatility_63"]), s["symbol"]))
    ranks = {s["symbol"]: i for i, s in enumerate(near)}
    risk = {s["symbol"]: i for i, s in enumerate(calm)}
    chosen = sorted(known, key=lambda s: (ranks[s["symbol"]] + risk[s["symbol"]], s["symbol"]))[:PARAMS["count"]]
    return positions(chosen)
''',
}

GRIDS = {
    "dual_momentum_12m": [{"count": n} for n in (1, 3, 5, 6, 10)],
    "sma_10m_filter": [{}],
    "high_52week": [dict(count=n, field=f) for n, f in product((1, 3, 5, 6, 10), ("close_to_high_12m", "close_to_max_close_12m"))],
    "frog_in_pan": [dict(count=n, winner_fraction=f) for n, f in product((1, 2, 3), ("0.2", "0.3", "0.5"))],
    "high_calm_rank": [dict(count=n, field=f) for n, f in product((1, 3, 5, 6, 10), ("close_to_high_12m", "close_to_max_close_12m"))],
}


def source_for(family, parameters):
    return prune_source(COMMON.replace("PARAMETERS", repr(parameters)) + BODIES[family])


def prune_source(source):
    # 删除未使用的公共函数，失败原型只写入研究输出，不加入正式策略目录。
    tree = ast.parse(source)
    definitions = {n.name: n for n in tree.body if isinstance(n, ast.FunctionDef)}
    needed = {"decide"}
    while True:
        used = {n.id for name in needed for n in ast.walk(definitions[name]) if isinstance(n, ast.Name)}
        more = needed | (used & definitions.keys())
        if needed == more:
            break
        needed = more
    lines = source.splitlines()
    for node in reversed(tree.body):
        if isinstance(node, ast.FunctionDef) and node.name not in needed:
            del lines[node.lineno - 1:node.end_lineno]
    return "\n".join(lines).strip() + "\n"


def scan(release, destination, grids=GRIDS, source_builder=source_for):
    verify_release(release)
    samples = rows(release / "inputs.jsonl")
    outcomes = {r["sample_id"]: r for r in rows(release / "outcomes.jsonl")}

    @lru_cache(None)
    def marks(i, code, rule):
        exit_json, grid_json = rule
        exits = {code: {k: v if k == "max_hold_days" else Decimal(v) for k, v in json.loads(exit_json).items()}} if exit_json else None
        grids = {code: {k: Decimal(v) if k == "step" else v for k, v in json.loads(grid_json).items()}} if grid_json else None
        segments = portfolio_segments({code: Decimal(1)}, outcomes[samples[i]["id"]], exits, grids)[0]
        accumulated, values = Decimal(1), []
        for value in segments:
            accumulated *= 1 + value
            values.append(float(accumulated))
        return values

    def evaluate(source):
        namespace = {}
        exec(compile(source, "strategy.py", "exec"), namespace)
        scores = []
        for i, sample in enumerate(samples):
            values = [1., 1., 1.]
            for code, position in namespace["decide"](sample).items():
                position = position if isinstance(position, dict) else {"weight": position}
                weight = float(Decimal(position["weight"]).quantize(Decimal("0.00000001"), rounding=ROUND_DOWN))
                rule = (json.dumps(position["exit"], sort_keys=True) if "exit" in position else "",
                        json.dumps(position["grid"], sort_keys=True) if "grid" in position else "")
                for j, mark in enumerate(marks(i, code, rule)):
                    values[j] += weight * (mark - 1)
            scores.append(100 * (.3 * (values[0] - 1) + .3 * (values[1] / values[0] - 1) + .4 * (values[2] / values[1] - 1)))
        return scores

    def stats(values):
        return {"score": mean(v + 2.5 * (v > 0) for v in values), "returns": mean(values), "win": mean(v > 0 for v in values)}

    trials, winners = [], []
    with new_directory(destination) as stage:
        for family, parameters in grids.items():
            records = []
            for params in parameters:
                source = source_builder(family, params)
                returns = evaluate(source)
                record = {"family": family, "parameters": params, "full": stats(returns),
                          "early": stats([v for s, v in zip(samples, returns) if s["decision_date"] < "2018"]),
                          "late": stats([v for s, v in zip(samples, returns) if s["decision_date"] >= "2018"])}
                records.append(record)
                trials.append(record)
            winner = max(records, key=lambda r: r["full"]["score"])
            winners.append(winner)
            candidate = stage / "candidates" / family
            candidate.mkdir(parents=True)
            (candidate / "strategy.py").write_text(source_builder(family, winner["parameters"]), encoding="utf-8")
            (candidate / "strategy.md").write_text(f"# 长窗口规则适配：{family}\n\n仅供研究，季度股票长仓，不是原论文完整组合。参数按148期全段选择，不是样本外。\n", encoding="utf-8")
            print(json.dumps(winner), flush=True)
        report = {"release_sha256": digest(release / "manifest.json"), "trials": trials,
                  "winners": sorted(winners, key=lambda r: r["full"]["score"], reverse=True)}
        write_json(stage / "results.json", report)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    builder = sub.add_parser("build")
    builder.add_argument("--history", type=Path, required=True)
    builder.add_argument("--calendar", type=Path, required=True)
    for command in (builder, sub.add_parser("scan")):
        command.add_argument("--release", type=Path, required=True)
        command.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "build":
        report = build(args.release, args.history, args.calendar, args.out)
        print(json.dumps({k: v for k, v in report.items() if k != "raw_hashes"}, ensure_ascii=False, indent=2))
    else:
        report = scan(args.release, args.out)
        print(f"COMPLETED {len(report['trials'])} trials")


if __name__ == "__main__":
    main()
