"""富贵策略首轮研究：从既有同源缓存计算技术信号，不将缺失基本面当作满足。"""

import argparse
import copy
import json
import shutil
import sys
from collections import Counter
from datetime import date
from decimal import Decimal
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from benchmark.files import digest, new_directory, read_json, require, rows, write_json, write_rows
from benchmark.pool_release import momentum
from benchmark.release import verify_release


VERSION = "fugui-technical-v1"
FIELDS = ("monthly_low", "first_pullback", "daily_lower", "weekly_lower", "monthly_upper_return")
MINIMAL_FIELDS = FIELDS[:4]


def boll(prices):
    if len(prices) < 20 or any(p is None for p in prices[-20:]):
        return None
    window = prices[-20:]
    middle = sum(window) / 20
    deviation = (sum((p - middle) ** 2 for p in window) / 20).sqrt()
    return middle - 2 * deviation, middle, middle + 2 * deviation


def macd(prices):
    """首个收盘初始化 EMA；至少 35 根月线，固定种子，不按回测表现选择。"""
    if len(prices) < 35 or any(p is None for p in prices):
        return None
    fast = slow = prices[0]
    signal = Decimal(0)
    for price in prices[1:]:
        fast += Decimal(2) / 13 * (price - fast)
        slow += Decimal(2) / 27 * (price - slow)
        diff = fast - slow
        signal += Decimal(2) / 10 * (diff - signal)
    return diff, signal, 2 * (diff - signal)


def features(bars, calendar, decision):
    """只读截至决策日的行情。未知缺行切断历史，明确停牌沿用此前已知收盘。"""
    days = [d for d in calendar if d <= decision]
    require(days and days[-1] == decision, "决策日不在交易日历中")
    monthly, weekly, daily = {}, {}, []
    previous = None
    for day in days:
        bar = bars.get(day)
        if bar is None:
            previous = None
            # 缺行会影响所在周期的完整性，不能只凭月底价格覆盖它。
            monthly[day[:7]] = None
            weekly[date.fromisoformat(day).isocalendar()[:2]] = None
            daily.append(None)
            continue
        if bar["tradestatus"] == "1":
            previous = Decimal(bar["close"])
            require(previous.is_finite() and previous > 0, "无效历史价格")
        week = date.fromisoformat(day).isocalendar()[:2]
        for groups, key in ((monthly, day[:7]), (weekly, week)):
            if key not in groups or groups[key] is not None:
                groups[key] = previous
        daily.append(previous)

    # 月 MACD 使用最后一个未知周期之后的连续月线，不跨缺口初始化。
    months = list(monthly.values())
    start = max((i + 1 for i, p in enumerate(months) if p is None), default=0)
    months = months[start:]
    weeks = list(weekly.values())
    output = {k: None for k in FIELDS}
    output.update(months=len(months), issues={})
    now = daily[-1]
    month_band, week_band, day_band = boll(months), boll(weeks), boll(daily)
    monthly_macd = macd(months)
    if now is not None:
        for key, band in (("daily_lower", day_band), ("weekly_lower", week_band)):
            if band:
                output[key] = now <= band[0] + now * Decimal("0.05")
        if month_band:
            output["monthly_upper_return"] = str(month_band[2] / now - 1)
    if len(months) >= 36 and monthly_macd is not None and now is not None:
        previous_band = boll(months[:-1])
        high, low = max(months[-36:]), min(months[-36:])
        flat = abs(month_band[0] - previous_band[0]) <= now * Decimal("0.02")
        near_low = now <= low + (high - low) * Decimal("0.2")
        underwater = all(value < 0 for value in monthly_macd)
        output["monthly_low"] = now <= high * Decimal("0.5") and near_low and flat and underwater
    if len(months) >= 35 and now is not None:
        # 对20月与30月均线分别找最近6个月的突破；随后第一次进入支撑±5%才算回踩。
        hit = False
        for period in (20, 30):
            averages = [sum(months[i - period + 1:i + 1]) / period if i >= period - 1 else None
                        for i in range(len(months))]
            crosses = [i for i in range(max(period, len(months) - 7), len(months) - 1)
                       if months[i - 1] <= averages[i - 1] and months[i] > averages[i]]
            if not crosses:
                continue
            cross = crosses[-1]
            first = all(months[i] > averages[i] * Decimal("1.05") for i in range(cross + 1, len(months) - 1))
            support = averages[-1]
            hit |= first and support * Decimal("0.95") <= now <= support * Decimal("1.05")
        output["first_pullback"] = hit
    for key in FIELDS:
        if output[key] is None:
            output["issues"][key] = "insufficient_contiguous_history"
    return output


def load_caches(roots, symbols):
    """仅接受 pool-build 使用的 Baostock 后复权档案，重叠价格必须一致。"""
    history = {code: {} for code in symbols}
    calendars, hashes = {}, {}
    for root in roots:
        folder = root / "raw" / "baostock" / "history"
        require(folder.is_dir(), f"缺少行情缓存: {folder}")
        for path in sorted(folder.glob("*.json")):
            if path.name.startswith("trade_dates-"):
                for row in read_json(path):
                    day, opened = row["calendar_date"], row["is_trading_day"]
                    require(day not in calendars or calendars[day] == opened, f"日历冲突: {day}")
                    calendars[day] = opened
            elif path.name.endswith("-adjust1.json") and path.name[:9] in symbols:
                code = path.name[:9]
                for row in read_json(path):
                    number, market = code.split(".")
                    require(row["code"] == f"{market.lower()}.{number}" and row["tradestatus"] in ("0", "1"),
                            f"证券/状态错误: {path}")
                    value = {k: row[k] for k in ("close", "tradestatus")}
                    old = history[code].get(row["date"])
                    require(old is None or old == value, f"重叠行情不一致: {code}/{row['date']}")
                    history[code][row["date"]] = value
            else:
                continue
            hashes[f"{root.name}/{path.relative_to(root).as_posix()}"] = digest(path)
    require(calendars, "缺少独立交易日历")
    require(all(history.values()), "缓存缺少股票历史")
    return history, sorted(d for d, opened in calendars.items() if opened == "1"), hashes


def build(release, roots, destination, minimal=False, built_on=None):
    verify_release(release)
    config = read_json(release / "config.json") if minimal else None
    if minimal:
        require(config.get("input_features") == "monthly-risk-regression-v1", "最小v8必须基于正式v7输入构建")
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
            stock["fugui"] = ({**{k: signal[k] for k in MINIMAL_FIELDS},
                              "issues": {k: v for k, v in signal["issues"].items() if k in MINIMAL_FIELDS}}
                             if minimal else signal)
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
                "input_features": "monthly-risk-regression-fugui-v1", "built_on": built_on,
                "base_comparison_tag": "pool-scoring-v7", "fugui_features": VERSION})
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
    builder.add_argument("--minimal", action="store_true", help="仅加入默认富贵策略需要的四个信号，供v8发布")
    builder.add_argument("--built-on", help="最小发布包必须显式指定构建日期")
    args = parser.parse_args()
    report = build(args.release, args.universe_root, args.out, args.minimal, args.built_on)
    print(json.dumps({k: v for k, v in report.items() if k != "raw_hashes"}, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
