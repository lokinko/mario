"""离线契约校验与历史时点投影；本模块不访问网络或在线业务代码。"""

from datetime import timedelta
from decimal import Decimal

from .files import (Invalid, day, digest, instant, local_file, number, read_json,
                    require, rows, source, symbol)

TABLES = ("calendar", "instruments", "universe", "prices", "reports")


def close_time(value):
    return instant(value + "T18:00:00+08:00")


def validate_config(config):
    start, end = day(config["start"]), day(config["end"])
    require(day(config["source_start"]) < start <= end, "source_start 必须覆盖前置观察窗口")
    require(config["index"] == "000922.CSI", "本版本只支持中证红利候选池")
    require(config["top_n"] == 20, "本版本固定 top-20")
    require(isinstance(config["lookback_sessions"], int) and config["lookback_sessions"] > 0,
            "观察窗口必须为正整数")
    require(config["horizons"] and len(set(config["horizons"])) == len(config["horizons"])
            and all(type(h) is int and h > 0 for h in config["horizons"]), "预测窗口错误")
    previous = start - timedelta(days=1)
    require(list(config["splits"]) == ["train", "validation", "test"], "必须按时间定义三个 split")
    for bounds in config["splits"].values():
        left, right = map(day, bounds)
        require(left == previous + timedelta(days=1) and left <= right, "split 必须连续且不重叠")
        previous = right
    require(previous == end, "split 必须覆盖完整区间")
    etfs = [symbol(item["symbol"]) for item in config["etfs"]]
    require(len(etfs) >= 2 and len(etfs) == len(set(etfs)), "至少需要两个不同的宽指 ETF")
    require(config["reference_etf"] in etfs, "参照 ETF 必须在配置中")


def select_universe(snapshot):
    return sorted(snapshot["candidates"],
                  key=lambda row: (-number(row["market_cap"], True), row["symbol"]))[:20]


class Dataset:
    def __init__(self, root):
        self.root = root
        self.config = read_json(root / "config.json")
        validate_config(self.config)
        self.tables = {name: rows(root / f"{name}.jsonl") for name in TABLES}
        self.calendar = sorted(row["date"] for row in self.tables["calendar"] if row["is_open"])
        self.prices = {(row["symbol"], row["date"]): row for row in self.tables["prices"]}
        self.instruments = {row["symbol"]: row for row in self.tables["instruments"]}
        self.universes = {row["year"]: row for row in self.tables["universe"]}

    def validate(self):
        errors = []

        def check(label, operation):
            try:
                operation()
            except (ValueError, KeyError, TypeError, ArithmeticError, OSError) as exc:
                errors.append(f"{label}: {exc}")

        for table in TABLES:
            if not self.tables[table]:
                errors.append(f"缺少非空数据表: {table}.jsonl")
        check("交易日历", self._calendar)
        check("证券元数据", self._instruments)
        check("年度股票池", self._universes)
        check("日线价格", self._prices)
        check("财报文件", self._reports)
        if not errors:
            check("完整性", self._coverage)
        return {"valid": not errors, "errors": errors,
                "counts": {name: len(value) for name, value in self.tables.items()},
                "status": "validated" if not errors else "incomplete"}

    def _calendar(self):
        all_dates = sorted(row["date"] for row in self.tables["calendar"])
        require(len(all_dates) == len(set(all_dates)), "日历日期重复")
        dates = set(all_dates)
        for row in self.tables["calendar"]:
            day(row["date"])
            require(type(row["is_open"]) is bool, "日历必须明确开市/闭市")
            source(row["source_uri"])
        expected = day(self.config["source_start"])
        while expected <= day(self.config["end"]):
            require(expected.isoformat() in dates, f"日历缺少日期: {expected}")
            expected += timedelta(days=1)
        require(bool(self.calendar), "没有交易日")

    def _instruments(self):
        require(len(self.instruments) == len(self.tables["instruments"]), "证券重复")
        for row in self.instruments.values():
            symbol(row["symbol"])
            day(row["listed_on"])
            if row.get("delisted_on"):
                require(day(row["delisted_on"]) >= day(row["listed_on"]), "退市日期错误")
            require(row["kind"] in ("stock", "etf"), "证券类型错误")
            source(row["source_uri"])

    def _universes(self):
        years = range(day(self.config["start"]).year, day(self.config["end"]).year + 1)
        require(len(self.universes) == len(self.tables["universe"]), "年度股票池重复")
        for year in years:
            row = self.universes[year]
            require(row["membership_verified"] is True, "需核对历史成分及其生效范围")
            selection = day(row["selection_date"])
            require(selection.year == year - 1, "按上一年末已公开市值选样")
            prior = [d for d in self.calendar if d < f"{year}-01-01"]
            require(prior and row["selection_date"] == prior[-1], "市值必须取上一年最后交易日")
            first = next(d for d in self.calendar if d.startswith(str(year)))
            require(close_time(row["selection_date"]) <= instant(row["available_at"])
                    < instant(first + "T09:30:00+08:00"), "股票池信息时点错误")
            require(instant(row["membership_published_at"]) <= instant(row["available_at"]),
                    "不能使用尚未公开的成分")
            source(row["source_uri"])
            candidates = row["candidates"]
            require(len(candidates) >= 20, "候选不足 20")
            require(row["candidate_count"] == len(candidates), "候选池未完整导入")
            require(len({c["symbol"] for c in candidates}) == len(candidates), "候选重复")
            for candidate in candidates:
                symbol(candidate["symbol"])
                number(candidate["market_cap"], True)
            for candidate in select_universe(row):
                instrument = self.instruments[candidate["symbol"]]
                require(instrument["kind"] == "stock", "候选必须是股票")
                require(instrument["listed_on"] <= row["selection_date"], "上市前不能选入")
                require(not instrument.get("delisted_on") or instrument["delisted_on"] >= first,
                        "已退市证券不能选入新年度")

    def _prices(self):
        require(len(self.prices) == len(self.tables["prices"]), "证券/日期价格重复")
        calendar = set(self.calendar)
        for (code, date), row in self.prices.items():
            require(code in self.instruments and date in calendar, "未知证券或非交易日")
            require(row["status"] in ("trading", "suspended"), "价格状态错误")
            source(row["source_uri"])
            require(instant(row["available_at"]) >= instant(date + "T15:00:00+08:00"),
                    "收盘前不能使用当日日线")
            if row["status"] == "suspended":
                continue
            opening, high, low, closing = [number(row[k], True) for k in ("open", "high", "low", "close")]
            require(low <= min(opening, closing) <= max(opening, closing) <= high, "OHLC 范围错误")
            number(row["volume"])
            number(row["amount"])
            number(row["adj_factor"], True)
            require(row["adjustment_basis"] == "provider_total_return_proxy", "需明确复权收益代理口径")

    def _reports(self):
        seen = set()
        for row in self.tables["reports"]:
            require(row["symbol"] in self.instruments, "财报证券未知")
            period = day(row["period_end"])
            require((period.month, period.day) in ((3, 31), (6, 30), (9, 30), (12, 31)), "报告期错误")
            expected = {3: "q1", 6: "half_year", 9: "q3", 12: "annual"}[period.month]
            require(row["report_type"] == expected, "季度应使用一季报/半年报/三季报/年报原件")
            published, available = instant(row["published_at"]), instant(row["available_at"])
            require(close_time(row["period_end"]) < published <= available, "财报披露/可用时间错误")
            source(row["source_uri"])
            key = (row["symbol"], row["period_end"], row["revision_id"])
            require(key not in seen, "财报版本重复")
            seen.add(key)
            path = local_file(self.root, row["document_path"])
            with path.open("rb") as handle:
                require(path.suffix.lower() == ".pdf" and handle.read(5) == b"%PDF-", "财报不是 PDF")
            require(digest(path) == row["sha256"], "财报 SHA256 不匹配")
            require(row["version_verified"] is True, "需确认原始/修订版本，不能回填最新版")
            require(isinstance(row["metrics"], dict), "结构化财务指标必须是对象")
            for value in row["metrics"].values():
                require(isinstance(value, str) and Decimal(value).is_finite(), "财务指标必须为有限十进制字符串")

    def selected_symbols(self):
        return {c["symbol"] for snapshot in self.universes.values() for c in select_universe(snapshot)}

    def _coverage(self):
        start, end = self.config["source_start"], self.config["end"]
        symbols = self.selected_symbols() | {e["symbol"] for e in self.config["etfs"]}
        missing = []
        for code in sorted(symbols):
            instrument = self.instruments[code]
            for date in self.calendar:
                if max(start, instrument["listed_on"]) <= date <= min(end, instrument.get("delisted_on") or end):
                    if (code, date) not in self.prices:
                        missing.append(f"日线 {code}/{date}")
            if code not in self.selected_symbols():
                require(instrument["kind"] == "etf", "宽指标的必须是 ETF")
                continue
            # 最后年度年报也必须保留，即使次年才发布；时点投影不会提前读取。
            for year in range(day(self.config["start"]).year, day(end).year + 1):
                for suffix in ("03-31", "06-30", "09-30", "12-31"):
                    period = f"{year}-{suffix}"
                    if max(self.config["start"], instrument["listed_on"]) <= period <= min(end, instrument.get("delisted_on") or end):
                        if not any(r["symbol"] == code and r["period_end"] == period for r in self.tables["reports"]):
                            missing.append(f"财报 {code}/{period}")
        require(not missing, f"缺少 {len(missing)} 项，前 10 项: {', '.join(missing[:10])}")

    def price(self, code, date, cutoff=None):
        row = self.prices.get((code, date))
        if not row or row["status"] != "trading":
            return None
        if cutoff is not None and instant(row["available_at"]) > cutoff:
            return None
        return row

    def feature(self, code, date):
        cutoff = close_time(date)
        index = self.calendar.index(date)
        prior_index = index - self.config["lookback_sessions"]
        current = self.price(code, date, cutoff)
        prior = self.price(code, self.calendar[prior_index], cutoff) if prior_index >= 0 else None
        momentum = None
        if current and prior:
            momentum = str(number(current["close"], True) * number(current["adj_factor"], True)
                           / (number(prior["close"], True) * number(prior["adj_factor"], True)) - 1)
        latest = {}
        for report in sorted(self.tables["reports"], key=lambda r: (instant(r["available_at"]), r["revision_id"])):
            if report["symbol"] == code and instant(report["available_at"]) <= cutoff:
                latest[report["period_end"]] = report
        return {"symbol": code, "momentum": momentum,
                "close": current["close"] if current else None,
                "reports": [latest[key] for key in sorted(latest)]}

    def decisions(self):
        dates = [d for d in self.calendar if self.config["start"] <= d <= self.config["end"]]
        by_quarter = {}
        for date in dates:
            parsed = day(date)
            by_quarter[(parsed.year, (parsed.month - 1) // 3)] = date
        return sorted(by_quarter.values())

    def samples(self):
        for date in self.decisions():
            snapshot = self.universes[day(date).year]
            yield {"id": f"{self.config['version']}:{date}", "decision_date": date,
                   "cutoff": close_time(date).isoformat(), "universe_year": snapshot["year"],
                   "selection_source": snapshot["source_uri"],
                   "stocks": [self.feature(c["symbol"], date) for c in select_universe(snapshot)],
                   "etfs": [self.feature(e["symbol"], date) for e in self.config["etfs"]]}

    def labels(self, sample):
        date = sample["decision_date"]
        index = self.calendar.index(date)
        split = next(name for name, (start, end) in self.config["splits"].items() if start <= date <= end)
        boundary = self.config["splits"][split][1]
        for horizon in self.config["horizons"]:
            if index + horizon >= len(self.calendar):
                continue
            entry_date, exit_date = self.calendar[index + 1], self.calendar[index + horizon]
            # 标签的观察终点不得穿过训练/验证/测试边界。
            if exit_date > min(boundary, self.config["end"]):
                continue
            for item in sample["stocks"] + sample["etfs"]:
                code = item["symbol"]
                entry, exit_row = self.price(code, entry_date), self.price(code, exit_date)
                result = None
                if entry and exit_row:
                    result = str(number(exit_row["close"], True) * number(exit_row["adj_factor"], True)
                                 / (number(entry["open"], True) * number(entry["adj_factor"], True)) - 1)
                yield {"sample_id": sample["id"], "symbol": code, "split": split,
                       "horizon": horizon, "entry_date": entry_date, "exit_date": exit_date,
                       "adjusted_return_proxy": result,
                       "status": "observed" if result is not None else "unavailable"}
