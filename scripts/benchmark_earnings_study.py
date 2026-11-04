"""首次定期报告公告反应研究；该日期代理不等同于原文的盈利首次披露时点。"""

import argparse
import sys
from itertools import product
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from benchmark.baostock_market import mario_code
from benchmark.files import digest, read_json, require, rows
from benchmark.public_sources import CNINFO_QUERY, CSI_INDEX_PERF, report_catalog
from scripts.benchmark_long_history_study import COMMON, FIELDS, build, features, prune_source, scan
from scripts.benchmark_risk_study import monthly_features
from benchmark.announcement_indicators import announcement_signal


def first_reports(code, announcements):
    """只用首次完整定期报告事件，修订/更正不变成新盈利公告。原件版本仍未逐项核验。"""
    # 同一个组织标识可能关联更名前的另一证券代码；不能把组织相同当作证券相同。
    own = [r for r in announcements if r.get("secCode") == code[:6]]
    catalog = report_catalog(code, own, "2000-01-01", "2100-12-31")
    first = {}
    for record in sorted(catalog, key=lambda r: (r["available_at"], r["revision_id"])):
        if any(term in record["title"] for term in ("修订", "更正", "更新", "补充")):
            continue
        require(record["period_end"] <= record["available_at"][:10], "报告期晚于公告日")
        first.setdefault(record["period_end"], record)
    return list(first.values())


def build_earnings(release, history, calendar_path, announcements_root, market_path, destination):
    samples = rows(release / "inputs.jsonl")
    symbols = {s["symbol"] for sample in samples for s in sample["stocks"]}
    calendars = read_json(calendar_path)
    calendar = [f"{d[:4]}-{d[4:6]}-{d[6:]}" if len(d) == 8 else d for d in calendars]
    calendar_set = set(calendar)
    market_record = read_json(market_path)
    require(market_record["status"] == "completed", "市场对照采集未完成")
    require(market_record["source"] == CSI_INDEX_PERF and market_record["request"]["code"] == "H00300", "必须使用独立中证全收益行情")
    market_rows = market_record["rows"]
    require(len({r["date"] for r in market_rows}) == len(market_rows), "市场对照日期重复")
    market = {r["date"]: r["close"] for r in market_rows}
    requests = read_json(announcements_root.parent / "announcement-request.json")
    require(requests["source"] == CNINFO_QUERY, "公告请求必须来自已声明的巨潮适配器")
    require(requests["release_manifest_sha256"] == digest(release / "manifest.json"), "公告采集不属于该切片")
    requested = {r["symbol"]: r for r in requests["requests"]}
    catalogs, hashes, rejected = {}, {}, {}
    for code in sorted(symbols):
        path = announcements_root / f"{code}.json"
        record = read_json(path)
        require(record["status"] == "completed" and record["request"] == requested[code]
                and record["source"] == CNINFO_QUERY, f"公告采集未完成或请求/来源不符: {code}")
        catalogs[code] = first_reports(code, record["announcements"])
        hashes[code] = digest(path)
        foreign = [{"revision_id": r["announcementId"], "security_code": r.get("secCode"),
                    "issue": "foreign_security_in_org_query"} for r in record["announcements"] if r.get("secCode") != code[:6]]
        if foreign:
            rejected[code] = foreign

    def project(bars, days, decision):
        code = mario_code(next(iter(bars.values()))["code"])
        output = features(bars, days, decision)
        monthly = monthly_features(bars, days, decision)
        output["daily_returns_1m"] = monthly["daily_returns_1m"]
        output["issues"].update(monthly["issues"])
        value, event, issue = announcement_signal(bars, days, decision, catalogs[code], market)
        output.update(report_car3=value, report_car3_event=event)
        if issue:
            output["issues"]["report_car3"] = issue
        return output

    config = {"version": read_json(release / "config.json")["version"] + "-report-car3-research-v1",
              "input_features": "report-car3-research-v1", "announcement_proxy": "first_full_periodic_report",
              "announcement_source": CNINFO_QUERY, "announcement_source_hashes": hashes,
              "rejected_foreign_announcements": rejected, "base_comparison_tag": "pool-scoring-v6",
              "market_source": CSI_INDEX_PERF, "market_source_sha256": digest(market_path),
              "market_non_calendar_rows": sum(r["date"] not in calendar_set for r in market_rows),
              "market_missing_sessions": [d for d in calendar if market_record["request"]["start"] <= d <= market_record["request"]["end"] and d not in market]}
    return build(release, history, calendar_path, destination, project, (*FIELDS, "daily_returns_1m", "report_car3"), config)


BODIES = {
    "report_car3_rank": '''
def decide(sample):
    known = eligible(sample, ["report_car3"])
    chosen = sorted((s for s in known if not PARAMS["positive"] or signal(s, "report_car3") > 0),
                    key=lambda s: (-signal(s, "report_car3"), s["symbol"]))[:PARAMS["count"]]
    raw = {s["symbol"]: Decimal(s["market_cap_at_selection"]) if PARAMS["capitalization"] else Decimal(1) for s in chosen}
    total = sum(raw.values())
    return {k: v / total for k, v in raw.items()} if total else {}
''',
    "report_car3_calm": '''
def decide(sample):
    known = [s for s in eligible(sample, ["report_car3"]) if s["volatility_63"] is not None
             and (not PARAMS["positive"] or signal(s, "report_car3") > 0)]
    surprise = sorted(known, key=lambda s: (-signal(s, "report_car3"), s["symbol"]))
    calm = sorted(known, key=lambda s: (Decimal(s["volatility_63"]), s["symbol"]))
    ranks = {s["symbol"]: i for i, s in enumerate(surprise)}
    risk = {s["symbol"]: i for i, s in enumerate(calm)}
    chosen = sorted(known, key=lambda s: (ranks[s["symbol"]] + risk[s["symbol"]], s["symbol"]))[:PARAMS["count"]]
    return positions(chosen)
''',
    "report_car3_price_buckets": '''
def decide(sample):
    known = eligible(sample, ["report_car3", "momentum_12_2"])
    ordered = sorted(known, key=lambda s: (signal(s, "momentum_12_2"), s["symbol"]))
    groups = PARAMS["buckets"]
    result = {}
    for i in range(groups):
        group = ordered[len(ordered) * i // groups:len(ordered) * (i + 1) // groups]
        chosen = sorted((s for s in group if not PARAMS["positive"] or signal(s, "report_car3") > 0),
                        key=lambda s: (-signal(s, "report_car3"), s["symbol"]))[:PARAMS["count"]]
        if chosen:
            result.update(positions(chosen, Decimal(1) / groups))
    return result
''',
}
GRIDS = {
    "report_car3_rank": [dict(count=n, positive=p, capitalization=c) for n, p, c in product((1, 3, 6), (True, False), (True, False))],
    "report_car3_calm": [dict(count=n, positive=p) for n, p in product((1, 3, 6), (True, False))],
    "report_car3_price_buckets": [dict(buckets=b, count=n, positive=p) for b, n, p in product((2, 3, 5), (1, 2), (True, False))],
}


def source_for(family, parameters):
    return prune_source(COMMON.replace("PARAMETERS", repr(parameters)) + BODIES[family])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    builder = sub.add_parser("build")
    builder.add_argument("--history", type=Path, required=True)
    builder.add_argument("--calendar", type=Path, required=True)
    builder.add_argument("--announcements", type=Path, required=True)
    builder.add_argument("--market", type=Path, required=True)
    for command in (builder, sub.add_parser("scan")):
        command.add_argument("--release", type=Path, required=True)
        command.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "build":
        report = build_earnings(args.release, args.history, args.calendar, args.announcements, args.market, args.out)
        print({k: v for k, v in report.items() if k != "raw_hashes"})
    else:
        report = scan(args.release, args.out, GRIDS, source_for)
        print(f"COMPLETED {len(report['trials'])} trials")


if __name__ == "__main__":
    main()
