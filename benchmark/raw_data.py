"""原始研究数据的来源适配与归档；不计算策略指标、不读取结果标签。"""

from pathlib import Path

from .files import (day, digest, instant, new_directory, number, read_json, require, rows,
                    symbol, write_json, write_rows)
from .release import verify_release

PRICE_BASIS = "baostock_backward_adjusted"


class RawData:
    """指标计算只依赖此读取接口；供应商格式在导入时消化，不进入策略。"""

    def __init__(self, history, calendar, provenance, price_basis=PRICE_BASIS):
        require(calendar and calendar == sorted(set(calendar)), "交易日历必须升序且不重复")
        for value in calendar:
            day(value)
        self.history, self.calendar = history, calendar
        self.provenance, self.price_basis = provenance, price_basis
        for code, bars in history.items():
            symbol(code)
            for date, bar in bars.items():
                day(date)
                require(bar.get("tradestatus") in ("0", "1"), f"交易状态未知: {code}/{date}")
                if bar["tradestatus"] == "1":
                    number(bar["close"], positive=True)
                if "available_at" in bar:
                    require(instant(bar["available_at"]) >= instant(date + "T15:00:00+08:00"),
                            f"日线可得时间早于收盘: {code}/{date}")

    def at(self, code, decision, cutoff=None):
        """缺行保持缺行；已晚于截止时间的记录不会进入计算器。"""
        require(code in self.history, f"missing_raw_security: {code}")
        boundary = instant(cutoff or decision + "T18:00:00+08:00")
        require(boundary <= instant(decision + "T18:00:00+08:00"), "截止时间晚于决策时点")
        days = [d for d in self.calendar if d <= decision]
        require(days and days[-1] == decision, f"决策日不在交易日历中: {decision}")
        bars = {d: row for d, row in self.history[code].items() if d <= decision
                and instant(row.get("available_at", d + "T18:00:00+08:00")) <= boundary}
        return bars, days


def import_raw(destination, *, roots=(), history=None, calendar_path=None, symbols=None):
    """把两种现有来源缓存归档一次，此后增加指标不必重新采集。"""
    require(bool(roots) != (history is not None), "选择 universe-root 或 history/calendar 一种来源")
    if roots:
        require(symbols, "导入股票池缓存必须指定证券范围")
        bars, calendar, hashes = load_caches(roots, set(symbols))
        provenance = {"adapter": "baostock_universe_cache", "raw_hashes": hashes}
    else:
        require(calendar_path is not None, "history 导入需要独立交易日历")
        request = read_json(history / "request.json")
        codes = set(symbols or [r["symbol"] for r in request["requests"]])
        request, bars, hashes = load_history(history, codes)
        calendar = read_json(calendar_path)
        calendar = [f"{d[:4]}-{d[4:6]}-{d[6:]}" if len(d) == 8 else d for d in calendar]
        provenance = {"adapter": "baostock_history_cache", "request": request, "raw_hashes": hashes,
                      "calendar_sha256": digest(calendar_path)}
    raw = RawData(bars, calendar, provenance)
    with new_directory(destination) as stage:
        (stage / "bars").mkdir()
        for code, records in sorted(raw.history.items()):
            write_rows(stage / "bars" / f"{code}.jsonl", [{**row, "date": d} for d, row in sorted(records.items())])
        write_json(stage / "calendar.json", raw.calendar)
        write_json(stage / "sources.json", {"price_basis": raw.price_basis, "provenance": provenance,
                   "securities": sorted(bars), "availability_policy": "explicit_or_session_18h",
                   "pit_verified": False})
        write_json(stage / "manifest.json", {"files": {p.relative_to(stage).as_posix(): digest(p)
                   for p in sorted(stage.rglob("*")) if p.is_file()}})
    return {"securities": len(bars), "sessions": len(calendar), "raw_manifest_sha256": digest(destination / "manifest.json")}


def read_raw(root):
    root = Path(root)
    verify_release(root)
    metadata = read_json(root / "sources.json")
    history = {}
    for code in metadata["securities"]:
        symbol(code)
        records = rows(root / "bars" / f"{code}.jsonl")
        require(records and len({r["date"] for r in records}) == len(records), f"空或重复日线: {code}")
        history[code] = {r["date"]: r for r in records}
    return RawData(history, read_json(root / "calendar.json"),
                   {"raw_manifest_sha256": digest(root / "manifest.json"), **metadata}, metadata["price_basis"])


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
                    value = dict(row)
                    old = history[code].get(row["date"])
                    require(old is None or all(old[k] == v for k, v in value.items() if k in old),
                            f"重叠行情不一致: {code}/{row['date']}")
                    history[code][row["date"]] = {**(old or {}), **value}
            else:
                continue
            hashes[f"{root.name}/{path.relative_to(root).as_posix()}"] = digest(path)
    require(calendars, "缺少独立交易日历")
    require(all(history.values()), "缓存缺少股票历史")
    return history, sorted(d for d, opened in calendars.items() if opened == "1"), hashes


def load_history(history, symbols):
    request = read_json(history / "request.json")
    require(request["adjustflag"] == "1" and request["source"] == "https://www.baostock.com/",
            "研究仅接受已对账的 Baostock 后复权缓存")
    all_bars, hashes = {}, {}
    requested = {r["symbol"]: r for r in request["requests"]}
    require(symbols <= requested.keys(), "缓存请求不含全部历史股票")
    for symbol in sorted(symbols):
        path = history / "repaired" / f"{symbol}.json"
        if not path.exists():
            path = history / "raw" / f"{symbol}.json"
        record = read_json(path)
        require(record["status"] == "completed" and record["request"] == requested[symbol], f"采集未完成: {symbol}")
        data = record["rows"]
        require(data and len({r["date"] for r in data}) == len(data), f"空记录或重复日线: {symbol}")
        code, market = symbol.split(".")
        require(all(r["code"] == f"{market.lower()}.{code}" and
                    requested[symbol]["start"] <= r["date"] <= requested[symbol]["end"] and
                    r["tradestatus"] in ("0", "1") for r in data), f"日线代码/日期/状态不符: {symbol}")
        all_bars[symbol] = {r["date"]: r for r in data}
        hashes[path.relative_to(history).as_posix()] = digest(path)
    return request, all_bars, hashes


def verify_sources(ledger, root):
    """人工核验的语义与原件哈希分开；本函数仅验证档案身份，不声称自动理解PDF。"""
    root = root.resolve()
    for source in ledger["sources"].values():
        path = (root / source["document_path"]).resolve()
        require(path.is_relative_to(root), "原件路径越出证据目录")
        require(digest(path) == source["sha256"], "财报原件哈希不符")
        require(source["source_uri"].startswith("https://") and source["pages"]
                and all(type(p) is int and p > 0 for p in source["pages"]), "缺少来源和物理页码")
    for record in ledger["cumulative"] + ledger["capital_proofs"]:
        require(record["sources"], "财务事实缺少原件证据")
        for identity in record["sources"]:
            require(identity in ledger["sources"], "未知证据标识")
            require(instant(record["available_at"]) >= instant(ledger["sources"][identity]["available_at"]),
                    "事实可得时间早于其证据")
