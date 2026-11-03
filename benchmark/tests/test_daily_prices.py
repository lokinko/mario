import io
import json
import tempfile
import unittest
from contextlib import redirect_stdout
from datetime import date, datetime, timedelta, timezone
from pathlib import Path
from unittest.mock import patch

from benchmark.daily_prices import PRICE_BASIS, latest_quote, update_prices
from benchmark.files import Invalid, digest, read_json, rows, write_json
from benchmark.index_membership import collect_memberships
from benchmark.release import verify_release
from benchmark.tests.test_index_membership import workbook_content


def calendar():
    return [(date(2026, 10, 26) + timedelta(days=i)).isoformat() for i in range(8)
            if (date(2026, 10, 26) + timedelta(days=i)).weekday() < 5]


def history(code, close=10, closed=False):
    days = calendar() + (["2026-10-31"] if closed else [])
    quotes = {key: [close] * len(days) for key in ("open", "high", "low", "close")}
    quotes["volume"] = [100] * len(days)
    return {"meta": {"symbol": code.replace(".SH", ".SS")},
            "timestamp": [int(datetime.fromisoformat(d).replace(tzinfo=timezone.utc).timestamp()) for d in days],
            "indicators": {"quote": [quotes], "adjclose": [{"adjclose": [close - 1] * len(days)}]}}


class DailyPricesTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source = self.root / "source"
        (self.source / "universe").mkdir(parents=True)
        for pool, code in (("dividend", "600000.SH"), ("tech", "000001.SZ")):
            write_json(self.source / "universe" / f"{pool}-2025.json", {"pool": pool, "year": 2025, "selected": [code]})
        self.close, self.fail, self.closed = 10, set(), False
        self.calls = []

        def fetch(code, start, end):
            self.calls.append((code, start, end))
            if code in self.fail:
                raise ValueError("source unavailable")
            value = history(code, self.close, self.closed)
            return "https://example.com/history", {"chart": {"result": [value]}}, value

        for name, value in (("yahoo_history", fetch), ("download", lambda _: json.dumps(calendar()).encode())):
            patcher = patch(f"benchmark.daily_prices.{name}", value)
            patcher.start()
            self.addCleanup(patcher.stop)
        patcher = patch("benchmark.collect.time.sleep")
        patcher.start()
        self.addCleanup(patcher.stop)
        quiet = redirect_stdout(io.StringIO())
        quiet.__enter__()
        self.addCleanup(quiet.__exit__, None, None, None)

    def build(self, name="updated", previous=None, cache=None, refresh=False):
        out = self.root / name
        result = update_prices(self.source, previous, out, "2026-10-26", "2026-11-02",
                               {"commit": "source"}, workers=2, cache_root=cache, refresh_history=refresh)
        return out, result

    def test_archives_provider_basis_adjusted_close_and_raw_response_separately(self):
        out, result = self.build()
        self.assertTrue(verify_release(out)["valid"])
        self.assertEqual(result["with_history"], 2)
        bars = rows(out / "prices/600000.SH.jsonl")
        self.assertEqual((bars[0]["close"], bars[0]["adjusted_close"]), ("10", "9"))
        self.assertEqual(bars[0]["price_basis"], PRICE_BASIS)
        self.assertNotIn("tradestatus", bars[0])
        self.assertTrue((out / "sources/600000.SH.json").exists())

    def test_normal_daily_refresh_does_not_splice_recent_prices_into_history(self):
        previous, _ = self.build("previous")
        self.close = 20
        out, _ = self.build(previous=previous)
        self.assertEqual(rows(out / "prices/600000.SH.jsonl"), rows(previous / "prices/600000.SH.jsonl"))
        self.assertEqual(read_json(out / "sources/600000.SH.json"), read_json(previous / "sources/600000.SH.json"))

    def test_compact_calendar_dates_are_supported(self):
        with patch("benchmark.daily_prices.download", return_value=json.dumps([d.replace("-", "") for d in calendar()]).encode()):
            out, _ = self.build()
        self.assertTrue(verify_release(out)["valid"])

    def test_replaces_complete_history_when_provider_revises_adjustment_scale(self):
        previous, _ = self.build("previous")
        self.close = 20
        out, _ = self.build(previous=previous, refresh=True)
        self.assertTrue(all(r["close"] == "20" for r in rows(out / "prices/600000.SH.jsonl")))

    def test_unavailable_source_preserves_previous_prices_and_reports_its_scope(self):
        previous, _ = self.build("previous")
        self.fail = {"600000.SH"}
        out, result = self.build(previous=previous)
        report = read_json(out / "coverage.json")
        issue = next(r for r in report["securities"] if r["symbol"] == "600000.SH")
        self.assertEqual(issue["issue_code"], "public_history_unavailable")
        self.assertEqual(result["source_unavailable"], 1)
        self.assertEqual(rows(out / "prices/600000.SH.jsonl"), rows(previous / "prices/600000.SH.jsonl"))

    def test_closed_source_observations_are_preserved_outside_trading_series(self):
        self.closed = True
        out, _ = self.build()
        self.assertEqual(len(rows(out / "prices/600000.SH.jsonl")), 6)
        excluded = rows(out / "source-observations/600000.SH.jsonl")
        self.assertEqual(excluded[0]["issue_codes"], ["calendar_closed_observation"])

    def test_cache_only_resumes_initial_collection_and_corruption_never_becomes_prices(self):
        cache = self.root / "cache"
        first, _ = self.build("first", cache=cache)
        self.calls = []
        self.build("resumed", cache=cache)
        self.assertFalse(self.calls)
        self.build("refresh", previous=first, cache=cache)
        self.assertEqual(len(self.calls), 2)
        path = cache / "600000.SH.json"
        record = read_json(path)
        record["response"]["chart"]["result"][0]["meta"]["symbol"] = "wrong"
        write_json(path, record)
        bad, result = self.build("bad", cache=cache)
        self.assertEqual(result["source_unavailable"], 1)
        self.assertFalse((bad / "prices/600000.SH.jsonl").exists())

    def test_complete_index_members_are_added_without_changing_annual_pools(self):
        with patch("benchmark.index_membership.sheet_rows", side_effect=json.loads), patch(
                "benchmark.index_membership.download",
                side_effect=lambda url, **_: workbook_content(url.rsplit("/", 1)[-1][:6])):
            collect_memberships(self.source, "2026-11-03")
            out, result = self.build()
        self.assertEqual(result["stocks"], 76)
        for index in ("930050", "000016"):
            self.assertEqual(result["indices"][index], {"members": 50, "with_history": 50, "at_last_session": 50, "daily_bars_at_last_session": 50, "quotes_at_last_session": 0})
        self.assertEqual(read_json(out / "config.json")["pools"], {"dividend": [2025], "tech": [2025]})

    def test_refuses_old_price_basis_and_modified_release(self):
        previous, _ = self.build("previous")
        config = read_json(previous / "config.json")
        write_json(previous / "config.json", {**config, "schema_version": 1})
        manifest = read_json(previous / "manifest.json")
        manifest["files"]["config.json"] = digest(previous / "config.json")
        write_json(previous / "manifest.json", manifest)
        with self.assertRaisesRegex(Invalid, "价格口径不同"):
            self.build(previous=previous)

    def test_latest_quote_stays_separate_when_current_daily_bar_is_missing(self):
        value = history("600000.SH")
        for key in ("open", "high", "low", "close"):
            value["indicators"]["quote"][0][key][-1] = None
        value["meta"].update(regularMarketPrice=12, regularMarketTime=value["timestamp"][-1], currency="CNY")
        with patch("benchmark.daily_prices.yahoo_history", side_effect=lambda c, *_: (
                "https://example.com/history", {"chart": {"result": [value if c == "600000.SH" else history(c)]}}, {})):
            out, _ = self.build()
        issue = next(r for r in read_json(out / "coverage.json")["securities"] if r["symbol"] == "600000.SH")
        self.assertEqual(issue["last_observed"], "2026-10-30")
        self.assertEqual(issue["last_price_date"], "2026-11-02")
        self.assertEqual(issue["latest_quote"]["price"], "12")
        self.assertEqual(rows(out / "prices/600000.SH.jsonl")[-1]["date"], "2026-10-30")
        quote = issue["latest_quote"]
        source = read_json(out / quote["source_record"])[quote["source_key"]]
        self.assertEqual(source["response"]["regularMarketPrice"], 12)

    def test_quote_outside_cutoff_or_on_closed_day_is_not_used(self):
        value = history("600000.SH")
        value["meta"].update(regularMarketPrice=12)
        for date_text in ("2026-11-03", "2026-10-31"):
            value["meta"]["regularMarketTime"] = int(datetime.fromisoformat(date_text).replace(tzinfo=timezone.utc).timestamp())
            record = {"response": {"chart": {"result": [value]}}}
            self.assertIsNone(latest_quote("600000.SH", record, "2026-11-02", set(calendar())))

    def test_retired_history_reference_remains_separate_from_new_provider_rows(self):
        self.fail = {"600000.SH"}
        historical = {"commit": "frozen", "securities": {"600000.SH": {
            "outDate": "2026-10-30", "series": {"3": {"last_observed": "2026-10-29"}}}}}
        out = self.root / "referenced"
        result = update_prices(self.source, None, out, "2026-10-26", "2026-11-02", {}, historical=historical)
        issue = next(r for r in read_json(out / "coverage.json")["securities"] if r["symbol"] == "600000.SH")
        self.assertEqual(result["available_history"], 2)
        self.assertEqual(result["unavailable_active"], 0)
        self.assertTrue(issue["historical_retired"])
        self.assertIsNone(issue["last_observed"])
        self.assertFalse((out / "prices/600000.SH.jsonl").exists())
        self.assertEqual(read_json(out / "historical-reference.json")["commit"], "frozen")
        historical["securities"]["600000.SH"]["outDate"] = ""
        result = update_prices(self.source, None, self.root / "active", "2026-10-26", "2026-11-02", {}, historical=historical)
        self.assertEqual(result["unavailable_active"], 1)

    def test_unknown_calendar_tail_is_not_fabricated_as_closed_days(self):
        with patch("benchmark.daily_prices.download", return_value=json.dumps(calendar()[:-1]).encode()):
            with self.assertRaisesRegex(Invalid, "日历原件未覆盖"):
                self.build()
        self.assertFalse((self.root / "updated").exists())

    def test_rebuild_retains_prior_symbols_and_refuses_date_rollback(self):
        provenance = {"rebuild": {"start": "2026-10-26", "requested_end": "2026-11-02", "symbols": ["600001.SH"]}}
        out = self.root / "rebuilt"
        result = update_prices(self.source, None, out, "2026-10-26", "2026-11-02", provenance)
        self.assertEqual(result["stocks"], 3)
        self.assertTrue((out / "prices/600001.SH.jsonl").exists())
        with self.assertRaisesRegex(Invalid, "重建不能"):
            update_prices(self.source, None, self.root / "rollback", "2026-10-27", "2026-11-02", provenance)

    def test_other_symbol_response_does_not_become_the_requested_security(self):
        value = history("000001.SZ")
        with patch("benchmark.daily_prices.yahoo_history", return_value=(
                "https://example.com/history", {"chart": {"result": [value]}}, value)):
            out, result = self.build()
        self.assertEqual(result["with_history"], 1)
        self.assertFalse((out / "prices/600000.SH.jsonl").exists())


if __name__ == "__main__":
    unittest.main()
