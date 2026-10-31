"""从年度股票池提案构建可评分的小切片：时点输入与未来结果分开写入不可覆盖目录。

输入只含证券代码与决策时点前的数据，不含供应商当前简称（会泄露 ST、退市等结局）。
"""

from datetime import date
from decimal import Decimal

from .baostock_market import SOURCE as BAOSTOCK, Baostock
from .files import digest, new_directory, read_json, require, write_json, write_rows
from .scoring import BUY_COST, COST_BASIS, SEGMENT_DAYS, SEGMENT_WEIGHTS, boundaries, can_buy
from .tech_universe import cached

INDEX = "000300.SH"
LOOKBACK = 63


def quarter_decisions(calendar, year):
    output = {}
    for day in calendar:
        if day.startswith(str(year)):
            output[(int(day[5:7]) - 1) // 3] = day
    return [output[q] for q in sorted(output)]


def momentum(bars, calendar, day):
    index = calendar.index(day)
    if index < LOOKBACK:
        return None
    now, before = bars.get(day), bars.get(calendar[index - LOOKBACK])
    if not now or not before or "0" in (now.get("tradestatus", "1"), before.get("tradestatus", "1")):
        return None
    return str(Decimal(now["close"]) / Decimal(before["close"]) - 1)


def sample_rows(pool, year, decision, snapshot, adjusted, raw, index_bars, calendar):
    by_symbol = {row["symbol"]: row for row in snapshot["candidates"]}
    stocks = []
    for code in snapshot["selected"]:
        bar = raw[code].get(decision)
        stocks.append({"symbol": code, "close": bar["close"] if bar else None,
                       "trade_status": ("trading" if bar["tradestatus"] == "1" else "suspended") if bar else "no_bar",
                       "momentum_63": momentum(adjusted[code], calendar, decision),
                       "market_cap_at_selection": by_symbol[code]["market_cap"],
                       "selection_date": snapshot["selection_date"],
                       "sw_level1_name": by_symbol[code]["level1_name"]})
    index_bar = index_bars.get(decision)
    return {"id": f"{pool}-{decision}", "pool": pool, "universe_year": year, "decision_date": decision,
            "cutoff": f"{decision}T18:00:00+08:00", "stocks": stocks,
            "csi300": {"close": index_bar["close"] if index_bar else None,
                       "momentum_63": momentum(index_bars, calendar, decision)}}


def outcome_row(sample, adjusted, index_bars, calendar):
    later = [d for d in calendar if d > sample["decision_date"]]
    if not later:
        return None
    entry = later[0]
    edges = boundaries(calendar, entry)
    if not edges or entry not in index_bars:
        return None

    def path(bars):
        return [[d, bars[d]["close"]] for d in calendar if entry <= d <= edges[-1] and d in bars]

    stocks = {}
    for stock in sample["stocks"]:
        bars = adjusted[stock["symbol"]]
        bar = bars.get(entry)
        stocks[stock["symbol"]] = {"buyable": can_buy(bar), "entry_open": bar["open"] if can_buy(bar) else None,
                                   "path": path(bars)}
    return {"sample_id": sample["id"], "entry_date": entry, "boundaries": edges, "stocks": stocks,
            "index": {"symbol": INDEX, "entry_open": index_bars[entry]["open"], "path": path(index_bars)}}


def build_pool_release(universe_root, destination, pool="tech"):
    request = read_json(universe_root / "request.json")
    require(request["pool"] == pool, "股票池类型与采集目录不符")
    years = request["years"]
    raw_dir = universe_root / "raw" / "baostock" / "history"
    snapshots = {year: read_json(universe_root / "universe" / f"{pool}-{year}.json") for year in years}
    with Baostock() as market:
        calendars, adjusted, raw, index_bars = {}, {}, {}, {}
        for year in years:
            start, end = f"{year - 1}-08-01", f"{year + 1}-05-31"
            days = cached(raw_dir / f"trade_dates-{start}-{end}.json", lambda: market.trade_dates(start, end))
            calendars[year] = [row["calendar_date"] for row in days if row["is_trading_day"] == "1"]
            for code in snapshots[year]["selected"]:
                for flag, store in (("1", adjusted), ("3", raw)):
                    rows = cached(raw_dir / f"{code}-{start}-{end}-adjust{flag}.json",
                                  lambda: market.daily(code, start, end, flag))
                    store.setdefault(code, {}).update({row["date"]: row for row in rows})
            rows = cached(raw_dir / f"{INDEX}-{start}-{end}.json", lambda: market.daily(INDEX, start, end, "3", index=True))
            index_bars.update({row["date"]: row for row in rows})
    samples, outcomes, skipped = [], [], []
    for year in years:
        for decision in quarter_decisions(calendars[year], year):
            sample = sample_rows(pool, year, decision, snapshots[year], adjusted, raw, index_bars, calendars[year])
            outcome = outcome_row(sample, adjusted, index_bars, calendars[year])
            if outcome is None:
                skipped.append({"sample_id": sample["id"], "reason": "insufficient_future_window"})
                continue
            samples.append(sample)
            outcomes.append(outcome)
    config = {"version": f"{pool}-slice-{'-'.join(map(str, years))}", "pool": pool, "years": years,
              "segments_days": list(SEGMENT_DAYS), "segment_weights": [str(w) for w in SEGMENT_WEIGHTS],
              "buy_cost": str(BUY_COST), "cost_basis": COST_BASIS, "reference_index": INDEX,
              "momentum_lookback_sessions": LOOKBACK, "price_source": BAOSTOCK,
              "universe_membership_verified": False, "built_on": date.today().isoformat()}
    with new_directory(destination) as stage:
        write_json(stage / "config.json", config)
        write_rows(stage / "inputs.jsonl", samples)
        write_rows(stage / "outcomes.jsonl", outcomes)
        summary = {"samples": len(samples), "skipped": skipped, "years": years, "pool": pool,
                   "release_ready": False}
        write_json(stage / "summary.json", summary)
        files = {p.relative_to(stage).as_posix(): digest(p) for p in sorted(stage.rglob("*")) if p.is_file()}
        write_json(stage / "manifest.json", {"schema_version": 1, "files": files})
    return summary
