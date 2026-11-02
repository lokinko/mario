import tempfile
import unittest
from datetime import date, timedelta
from pathlib import Path
from unittest.mock import patch

from benchmark.daily_prices import update_prices
from benchmark.files import Invalid, read_json, rows, write_json
from benchmark.release import verify_release


def calendar(start, end):
    current = date.fromisoformat(start)
    result = []
    while current <= date.fromisoformat(end):
        result.append({"calendar_date": current.isoformat(), "is_trading_day": "1" if current.weekday() < 5 else "0"})
        current += timedelta(days=1)
    return result


class Market:
    missing, duplicate, wrong_code, delisted, correction = None, False, False, "", False
    calls = []

    def __enter__(self):
        return self

    def __exit__(self, *_):
        pass

    def trade_dates(self, start, end):
        return calendar(start, end)

    def stock_basics(self):
        return [{"code": code, "ipoDate": "2000-01-01", "outDate": self.delisted if code == "sh.600000" else ""}
                for code in ("sh.600000", "sz.000001")]

    def daily(self, code, start, end, flag, index=False):
        self.calls.append((code, start, end, flag))
        values = []
        for entry in calendar(start, end):
            day = entry["calendar_date"]
            if entry["is_trading_day"] != "1" or day == self.missing:
                continue
            close = "11" if self.correction else "10"
            row = {"date": day, "code": "wrong" if self.wrong_code else code[-2:].lower() + "." + code[:6],
                   "open": "10", "high": "12", "low": "9", "close": close, "preclose": "10",
                   "volume": "100", "amount": "1000"}
            if not index:
                row.update({"tradestatus": "0" if day == "2026-10-28" else "1", "isST": "0"})
            values.append(row)
        return values + values[:1] if self.duplicate else values


def total_return(_, start, end):
    return [{"date": r["calendar_date"], "close": "100"} for r in calendar(start, end) if r["is_trading_day"] == "1"]


class DailyPricesTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source = self.root / "source"
        (self.source / "universe").mkdir(parents=True)
        for pool, code in (("dividend", "600000.SH"), ("tech", "000001.SZ")):
            write_json(self.source / "universe" / f"{pool}-2025.json",
                       {"pool": pool, "year": 2025, "selected": [code]})
        Market.missing, Market.duplicate, Market.wrong_code, Market.delisted, Market.correction = None, False, False, "", False
        Market.calls = []
        for name, value in (("Baostock", Market), ("csi_index_closes", total_return)):
            patcher = patch(f"benchmark.daily_prices.{name}", value)
            patcher.start()
            self.addCleanup(patcher.stop)

    def build(self, name="updated", previous=None, end="2026-11-02"):
        out = self.root / name
        result = update_prices(self.source, previous, out, "2026-10-26", end,
                               {"commit": "source-sha"}, overlap_days=2)
        return out, result

    def test_collects_both_bases_and_indices_with_suspensions_and_no_weekend_padding(self):
        out, result = self.build()
        self.assertTrue(verify_release(out)["valid"])
        self.assertEqual((result["stocks"], result["series"], result["last_session"]), (2, 6, "2026-11-02"))
        bars = rows(out / "prices/600000.SH/adjust1/2026.jsonl")
        self.assertEqual(len(bars), 6)
        self.assertEqual(bars[2]["tradestatus"], "0")
        self.assertTrue((out / "prices/600000.SH/adjust3/2026.jsonl").exists())
        self.assertTrue((out / "prices/H00300/adjusttotal_return/2026.jsonl").exists())
        self.assertEqual(read_json(out / "config.json")["pools"], {"dividend": [2025], "tech": [2025]})

    def test_incremental_refresh_replaces_corrections_and_retains_prior_rows(self):
        previous, _ = self.build("previous", end="2026-10-30")
        Market.calls = []
        Market.correction = True
        out, _ = self.build(previous=previous)
        bars = rows(out / "prices/600000.SH/adjust1/2026.jsonl")
        self.assertEqual([r["close"] for r in bars], ["10", "10", "11", "11", "11", "11"])
        self.assertEqual(Market.calls[0][1], "2026-10-28")
        stable, _ = self.build("stable", previous=out)
        repeat, _ = self.build("repeat", previous=stable)
        self.assertEqual((stable / "manifest.json").read_bytes(), (repeat / "manifest.json").read_bytes())

    def test_missing_latest_bar_does_not_publish_or_modify_previous(self):
        previous, _ = self.build("previous", end="2026-10-30")
        before = (previous / "manifest.json").read_bytes()
        Market.missing = "2026-11-02"
        with self.assertRaisesRegex(Invalid, "日线未到齐"):
            self.build(previous=previous)
        self.assertFalse((self.root / "updated").exists())
        self.assertEqual((previous / "manifest.json").read_bytes(), before)

    def test_missing_refetch_bar_cannot_be_hidden_by_cached_bar(self):
        previous, _ = self.build("previous", end="2026-10-30")
        Market.missing = "2026-10-30"
        with self.assertRaisesRegex(Invalid, "日线未到齐"):
            self.build(previous=previous, end="2026-10-30")

    def test_rejects_duplicates_and_other_security_and_cleans_staging(self):
        for attr in ("duplicate", "wrong_code"):
            with self.subTest(attr=attr):
                setattr(Market, attr, True)
                with self.assertRaises(Invalid):
                    self.build()
                setattr(Market, attr, False)
                self.assertFalse(list(self.root.glob(".benchmark-*")))

    def test_delisted_security_does_not_require_prices_after_delisting(self):
        Market.delisted = "2026-10-29"
        out, _ = self.build()
        bars = rows(out / "prices/600000.SH/adjust1/2026.jsonl")
        self.assertEqual(bars[-1]["date"], "2026-10-28")

    def test_refuses_date_rollback_and_corrupted_previous_snapshot(self):
        previous, _ = self.build("previous")
        with self.assertRaisesRegex(Invalid, "回退"):
            self.build(previous=previous, end="2026-10-30")
        (previous / "coverage.json").write_text("[]\n")
        with self.assertRaisesRegex(Invalid, "改动"):
            self.build(previous=previous)

    def test_requires_both_pools(self):
        (self.source / "universe/tech-2025.json").unlink()
        with self.assertRaisesRegex(Invalid, "同时包含"):
            self.build()


if __name__ == "__main__":
    unittest.main()
