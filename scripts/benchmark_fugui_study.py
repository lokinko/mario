"""富贵策略首轮研究：从既有同源缓存计算技术信号，不将缺失基本面当作满足。"""

import argparse
import copy
import json
import shutil
import sys
from collections import Counter
from datetime import date
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from benchmark.files import digest, new_directory, read_json, require, rows, write_json, write_rows
from benchmark.pool_release import momentum
from benchmark.release import verify_release
from benchmark.technical_indicators import fugui_features as features
from benchmark.raw_data import load_caches


VERSION = "fugui-technical-v1"
FIELDS = ("monthly_low", "first_pullback", "daily_lower", "weekly_lower", "monthly_upper_return")
MINIMAL_FIELDS = FIELDS[:4]


def build(release, roots, destination, minimal=False, built_on=None):
    verify_release(release)
    config = read_json(release / "config.json") if minimal else None
    if minimal:
        require(built_on is not None, "最小发布包必须显式指定 built_on")
        built_on = date.fromisoformat(built_on).isoformat()
    samples = rows(release / "inputs.jsonl")
    history, calendar, hashes = load_caches(roots, {s["symbol"] for sample in samples for s in sample["stocks"]})
    enriched, coverage, comparisons = copy.deepcopy(samples), Counter(), Counter()
    cache = {}
    for sample in enriched:
        decision = sample["decision_date"]
        for stock in sample["stocks"]:
            bars = history[stock["symbol"]]
            for field, sessions in (("return_20", 20), ("momentum_63", 63)):
                if stock[field] is not None:
                    require(momentum(bars, calendar, decision, sessions) == stock[field],
                            f"共同字段不一致: {sample['id']}/{stock['symbol']}/{field}")
                    comparisons[field] += 1
            key = stock["symbol"], decision
            if key not in cache:
                cache[key] = features(bars, calendar, decision)
            signal = cache[key]
            stock["fugui"] = {**stock.get("fugui", {}), **({**{k: signal[k] for k in MINIMAL_FIELDS},
                              "issues": {k: v for k, v in signal["issues"].items() if k in MINIMAL_FIELDS}}
                             if minimal else signal)}
            coverage.update(f"{k}_known" for k in (MINIMAL_FIELDS if minimal else FIELDS) if stock["fugui"][k] is not None)
            coverage.update(k for k in MINIMAL_FIELDS if stock["fugui"][k] is True)
    with new_directory(destination) as stage:
        for path in release.rglob("*"):
            relative = path.relative_to(release)
            if path.is_file() and relative.as_posix() not in ("manifest.json", "inputs.jsonl"):
                (stage / relative).parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(path, stage / relative)
        write_rows(stage / "inputs.jsonl", enriched)
        if minimal:
            write_json(stage / "config.json", {**config, "version": config["version"] + "-fugui-v1",
                "built_on": built_on, "fugui_features": VERSION})
        report = {"research_only": not minimal, "feature_version": VERSION, "source_manifest_sha256": digest(release / "manifest.json"),
                  "raw_hashes": hashes, "coverage": dict(coverage), "common_fields": dict(comparisons),
                  "fundamentals_available": False, "outcomes_unchanged": digest(stage / "outcomes.jsonl") == digest(release / "outcomes.jsonl")}
        write_json(stage / "fugui-study.json", report)
        write_json(stage / "manifest.json", {"schema_version": 1, "files": {
            p.relative_to(stage).as_posix(): digest(p) for p in sorted(stage.rglob("*")) if p.is_file()}})
    verify_release(destination)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    builder = sub.add_parser("build")
    builder.add_argument("--release", type=Path, required=True)
    builder.add_argument("--universe-root", type=Path, action="append", required=True)
    builder.add_argument("--out", type=Path, required=True)
    builder.add_argument("--minimal", action="store_true", help="仅加入默认富贵策略需要的四个信号，不限定基础切片版本")
    builder.add_argument("--built-on", help="最小发布包必须显式指定构建日期")
    args = parser.parse_args()
    report = build(args.release, args.universe_root, args.out, args.minimal, args.built_on)
    print(json.dumps({k: v for k, v in report.items() if k != "raw_hashes"}, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
