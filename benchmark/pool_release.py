"""从一个或多个年度股票池提案构建可评分切片：时点输入与未来结果分开写入不可覆盖目录。

输入与随包快照只含证券代码与决策时点前的数据，不含供应商当前简称（会泄露 ST、退市等结局）。
"""

from datetime import date
from decimal import Decimal

from .baostock_market import SOURCE as BAOSTOCK, Baostock
from .collect import retry
from .files import digest, new_directory, read_json, require, write_json, write_rows
from .market_cap import cached
from .public_sources import CSI_INDEX_PERF, csi_index_closes
from .scoring import BUY_COST, COST_BASIS, SEGMENT_DAYS, SEGMENT_WEIGHTS, boundaries, can_buy, can_sell

POOLS = ("dividend", "tech")
INDEX = "000300.SH"
TOTAL_RETURN_INDEX = "H00300"
LOOKBACK = 63
SHORT_LOOKBACK = 20


def quarter_decisions(calendar, year):
    output = {}
    for day in calendar:
        if day.startswith(str(year)):
            output[(int(day[5:7]) - 1) // 3] = day
    return [output[q] for q in sorted(output)]


def momentum(bars, calendar, day, sessions=LOOKBACK):
    index = calendar.index(day)
    if index < sessions:
        return None
    now, before = bars.get(day), bars.get(calendar[index - sessions])
    if not now or not before or "0" in (now.get("tradestatus", "1"), before.get("tradestatus", "1")):
        return None
    return str(Decimal(now["close"]) / Decimal(before["close"]) - 1)


def volatility(bars, calendar, day, sessions=LOOKBACK):
    """最近 sessions 个有成交日的日收益率总体标准差（不年化）；停牌日跳过，不足时为空。"""
    closes = [Decimal(bars[d]["close"]) for d in calendar[:calendar.index(day) + 1]
              if d in bars and bars[d].get("tradestatus", "1") == "1"]
    if len(closes) <= sessions:
        return None
    returns = [closes[i] / closes[i - 1] - 1 for i in range(len(closes) - sessions, len(closes))]
    average = sum(returns) / sessions
    return str((sum((r - average) ** 2 for r in returns) / sessions).sqrt())


def sample_rows(pool, year, decision, snapshot, adjusted, raw, index_bars, calendar):
    by_symbol = {row["symbol"]: row for row in snapshot["candidates"]}
    stocks = []
    for code in snapshot["selected"]:
        bar = raw[code].get(decision)
        stocks.append({"symbol": code, "close": bar["close"] if bar else None,
                       "trade_status": ("trading" if bar["tradestatus"] == "1" else "suspended") if bar else "no_bar",
                       "momentum_63": momentum(adjusted[code], calendar, decision),
                       "return_20": momentum(adjusted[code], calendar, decision, SHORT_LOOKBACK),
                       "volatility_63": volatility(adjusted[code], calendar, decision),
                       "market_cap_at_selection": by_symbol[code]["market_cap"],
                       "selection_date": snapshot["selection_date"],
                       "sw_level1_name": by_symbol[code].get("level1_name")})
    index_bar = index_bars.get(decision)
    return {"id": f"{pool}-{decision}", "pool": pool, "universe_year": year, "decision_date": decision,
            "cutoff": f"{decision}T18:00:00+08:00", "stocks": stocks,
            "csi300": {"close": index_bar["close"] if index_bar else None,
                       "momentum_63": momentum(index_bars, calendar, decision)}}


def outcome_row(sample, adjusted, index_bars, total_return, calendar):
    """个股用后复权价格（分红与送转再投资）；对照用全收益指数，两边都计入分红。

    全收益指数只有收盘价，建仓价按前一交易日全收益收盘 × 沪深300 价格指数当日开盘/前收折算。
    个股路径每行为 [日期, 收盘, 开盘, 能否卖出]，供卖出规则逐日执行。
    """
    later = [d for d in calendar if d > sample["decision_date"]]
    if not later:
        return None
    entry = later[0]
    edges = boundaries(calendar, entry)
    if not edges or entry not in index_bars:
        return None
    previous = calendar[calendar.index(entry) - 1]
    window = [d for d in calendar if previous <= d <= edges[-1]]
    require(all(d in total_return for d in window) and previous in index_bars,
            f"{TOTAL_RETURN_INDEX} 在 {previous}–{edges[-1]} 缺少收盘点位")

    def path(bars):
        return [[d, bars[d]["close"], bars[d]["open"], can_sell(bars[d])]
                for d in calendar if entry <= d <= edges[-1] and d in bars]

    stocks = {}
    for stock in sample["stocks"]:
        bars = adjusted[stock["symbol"]]
        bar = bars.get(entry)
        stocks[stock["symbol"]] = {"buyable": can_buy(bar), "entry_open": bar["open"] if can_buy(bar) else None,
                                   "path": path(bars)}
    entry_open = (Decimal(total_return[previous]) * Decimal(index_bars[entry]["open"])
                  / Decimal(index_bars[previous]["close"]))
    return {"sample_id": sample["id"], "entry_date": entry, "boundaries": edges, "stocks": stocks,
            "index": {"symbol": TOTAL_RETURN_INDEX, "basis": "total_return", "entry_open": str(entry_open),
                      "path": [[d, total_return[d]] for d in window if d >= entry]}}


def build_pool_release(universe_roots, destination):
    """universe_roots：各股票池的采集目录；每个池子的行情缓存留在自己的目录里。"""
    pools = {}
    for root in universe_roots:
        request = read_json(root / "request.json")
        require(request["pool"] in POOLS, f"未知股票池类型: {request['pool']}")
        require(request["pool"] not in pools, f"股票池重复: {request['pool']}")
        pools[request["pool"]] = (root, request["years"])
    calendars, adjusted, raw, index_bars, total_return, snapshots = {}, {}, {}, {}, {}, {}
    with Baostock() as market:
        for pool, (root, years) in pools.items():
            raw_dir = root / "raw" / "baostock" / "history"
            for year in years:
                snapshot = snapshots[pool, year] = read_json(root / "universe" / f"{pool}-{year}.json")
                require(snapshot["pool"] == pool and snapshot["year"] == year, f"{pool}-{year} 快照与目录不符")
                start, end = f"{year - 1}-08-01", f"{year + 1}-05-31"
                days = cached(raw_dir / f"trade_dates-{start}-{end}.json", lambda: market.trade_dates(start, end))
                calendars[year] = [row["calendar_date"] for row in days if row["is_trading_day"] == "1"]
                for code in snapshot["selected"]:
                    for flag, store in (("1", adjusted), ("3", raw)):
                        rows = cached(raw_dir / f"{code}-{start}-{end}-adjust{flag}.json",
                                      lambda: market.daily(code, start, end, flag))
                        store.setdefault(code, {}).update({row["date"]: row for row in rows})
                rows = cached(raw_dir / f"{INDEX}-{start}-{end}.json",
                              lambda: market.daily(INDEX, start, end, "3", index=True))
                index_bars.update({row["date"]: row for row in rows})
                rows = cached(raw_dir / f"{TOTAL_RETURN_INDEX}-{start}-{end}.json",
                              lambda: retry(lambda: csi_index_closes(TOTAL_RETURN_INDEX, start, end)))
                total_return.update({row["date"]: row["close"] for row in rows})
    samples, outcomes, skipped = [], [], []
    for (pool, year), snapshot in sorted(snapshots.items()):
        for decision in quarter_decisions(calendars[year], year):
            sample = sample_rows(pool, year, decision, snapshot, adjusted, raw, index_bars, calendars[year])
            outcome = outcome_row(sample, adjusted, index_bars, total_return, calendars[year])
            if outcome is None:
                skipped.append({"sample_id": sample["id"], "reason": "insufficient_future_window"})
                continue
            samples.append(sample)
            outcomes.append(outcome)
    years = sorted({year for _, year in snapshots})
    config = {"version": f"{'+'.join(sorted(pools))}-{years[0]}-{years[-1]}",
              "pools": {pool: pool_years for pool, (_, pool_years) in sorted(pools.items())},
              "segments_days": list(SEGMENT_DAYS), "segment_weights": [str(w) for w in SEGMENT_WEIGHTS],
              "buy_cost": str(BUY_COST), "cost_basis": COST_BASIS, "reference_index": INDEX,
              "reference_total_return_index": TOTAL_RETURN_INDEX, "reference_total_return_source": CSI_INDEX_PERF,
              "momentum_lookback_sessions": LOOKBACK, "short_return_lookback_sessions": SHORT_LOOKBACK,
              "volatility_lookback_sessions": LOOKBACK, "price_source": BAOSTOCK,
              "stock_price_basis": "Baostock 后复权（adjustflag=1）：现金分红、送转、配股均按再投资折算，未计红利税",
              "universe_membership_verified": False, "built_on": date.today().isoformat()}
    with new_directory(destination) as stage:
        write_json(stage / "config.json", config)
        write_rows(stage / "inputs.jsonl", samples)
        write_rows(stage / "outcomes.jsonl", outcomes)
        (stage / "universe").mkdir()
        for (pool, year), snapshot in sorted(snapshots.items()):
            candidates = [{k: v for k, v in row.items() if k != "current_name"} for row in snapshot["candidates"]]
            write_json(stage / "universe" / f"{pool}-{year}.json", {**snapshot, "candidates": candidates})
        counts = {}
        for sample in samples:
            counts[sample["pool"]] = counts.get(sample["pool"], 0) + 1
        summary = {"samples": len(samples), "samples_by_pool": counts, "skipped": skipped,
                   "pools": config["pools"], "release_ready": False}
        write_json(stage / "summary.json", summary)
        files = {p.relative_to(stage).as_posix(): digest(p) for p in sorted(stage.rglob("*")) if p.is_file()}
        write_json(stage / "manifest.json", {"schema_version": 1, "files": files})
    return summary
