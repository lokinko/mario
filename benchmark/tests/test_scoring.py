import tempfile
import unittest
from datetime import date, timedelta
from decimal import Decimal
from pathlib import Path
from unittest.mock import patch

from benchmark.files import Invalid, rows, write_json
from benchmark.pool_release import (build_pool_release, momentum, outcome_row, quarter_decisions, sample_rows,
                                    volatility)
from benchmark.release import verify_release
from benchmark.scoring import boundaries, can_buy, evaluate_weights, portfolio_segments, score


def bar(price, status="1", **extra):
    value = {"open": price, "high": price, "low": price, "close": price, "preclose": price,
             "tradestatus": status}
    value.update(extra)
    return value


CALENDAR = ["2015-09-30", "2015-10-08", "2015-10-09", "2015-10-15", "2015-10-16", "2015-11-06",
            "2015-11-09", "2016-01-06", "2016-01-07"]


def outcome(stocks, index_path=None):
    return {"sample_id": "s1", "entry_date": "2015-10-08",
            "boundaries": ["2015-10-15", "2015-11-06", "2016-01-06"], "stocks": stocks,
            "index": {"entry_open": "100", "path": index_path or [["2015-10-08", "100"], ["2015-10-15", "110"],
                                                                 ["2015-11-06", "121"], ["2016-01-06", "121"]]}}


class ScoringTest(unittest.TestCase):
    def test_boundaries_count_calendar_days_from_entry_and_require_full_window(self):
        self.assertEqual(boundaries(CALENDAR, "2015-10-08"), ["2015-10-15", "2015-11-06", "2016-01-06"])
        self.assertIsNone(boundaries(CALENDAR[:-2], "2015-10-08"))
        with self.assertRaises(Invalid):
            boundaries(CALENDAR, "2015-10-01")

    def test_cannot_buy_suspended_or_one_price_limit_up_but_can_buy_limit_down(self):
        self.assertFalse(can_buy(None))
        self.assertFalse(can_buy(bar("10", status="0")))
        self.assertFalse(can_buy(bar("11", preclose="10")))
        self.assertTrue(can_buy(bar("9", preclose="10")))
        self.assertTrue(can_buy({"open": "10", "high": "11", "low": "10", "close": "11", "preclose": "10",
                                 "tradestatus": "1"}))

    def test_segments_are_non_overlapping_and_weighted_into_percent_score(self):
        index = {"a": {"buyable": True, "entry_open": "100",
                       "path": [["2015-10-08", "100"], ["2015-10-15", "110"], ["2015-11-06", "121"],
                                ["2016-01-06", "121"]]}}
        returns, blocked = portfolio_segments({"a": Decimal(1)}, outcome(index), cost=Decimal(0))
        self.assertEqual([round(r, 10) for r in returns], [Decimal("0.1"), Decimal("0.1"), Decimal(0)])
        self.assertEqual(blocked, [])
        self.assertAlmostEqual(score(returns), 6.0)

    def test_cash_blocked_entries_costs_and_delisting_valuation(self):
        stocks = {"live": {"buyable": True, "entry_open": "10",
                           "path": [["2015-10-08", "10"], ["2015-10-09", "5"]]},
                  "halted": {"buyable": False, "entry_open": None, "path": []}}
        returns, blocked = portfolio_segments({"live": Decimal("0.5"), "halted": Decimal("0.25")},
                                              outcome(stocks), cost=Decimal("0.01"))
        self.assertEqual(blocked, ["halted"])
        expected_first = Decimal("0.5") + Decimal("0.5") * Decimal("0.99") * Decimal("0.5") - 1
        self.assertEqual(returns[0], expected_first)
        self.assertEqual(returns[1:], [Decimal(0), Decimal(0)])
        with self.assertRaises(Invalid):
            portfolio_segments({"live": Decimal("0.8"), "halted": Decimal("0.3")}, outcome(stocks))

    def test_missing_submission_scores_as_cash_and_invalid_weights_are_rejected(self):
        samples = [{"id": "s1", "pool": "tech", "decision_date": "2015-09-30",
                    "stocks": [{"symbol": "000001.SZ"}, {"symbol": "000002.SZ"}]}]
        stocks = {code: {"buyable": True, "entry_open": "10",
                         "path": [["2015-10-08", "10"], ["2015-10-15", "12"]]} for code in ("000001.SZ", "000002.SZ")}
        report = evaluate_weights(samples, [outcome(stocks)], [])
        self.assertEqual(report["periods"][0]["score"], 0.0)
        self.assertFalse(report["periods"][0]["submitted"])
        self.assertGreater(report["periods"][0]["pool_equal_weight_score"], 0)
        self.assertAlmostEqual(report["periods"][0]["csi300_score"], 6.0)
        for rows in ([{"sample_id": "s1", "symbol": "600000.SH", "weight": "0.1"}],
                     [{"sample_id": "s1", "symbol": "000001.SZ", "weight": "0.6"},
                      {"sample_id": "s1", "symbol": "000002.SZ", "weight": "0.6"}],
                     [{"sample_id": "s1", "symbol": "000001.SZ", "weight": 0.5}],
                     [{"sample_id": "s1", "symbol": "000001.SZ", "weight": "0.1"},
                      {"sample_id": "s1", "symbol": "000001.SZ", "weight": "0.1"}]):
            with self.assertRaises(Invalid):
                evaluate_weights(samples, [outcome(stocks)], rows)


class PoolReleaseTest(unittest.TestCase):
    def test_inputs_exclude_provider_names_and_use_only_past_prices(self):
        calendar = [f"{i:02d}" for i in range(70)]
        adjusted = {"000001.SZ": {day: bar(str(100 + i)) for i, day in enumerate(calendar)}}
        snapshot = {"selection_date": "2014-12-31", "selected": ["000001.SZ"],
                    "candidates": [{"symbol": "000001.SZ", "market_cap": "1", "level1_name": "电子",
                                    "current_name": "退市某某"}]}
        sample = sample_rows("tech", 2015, "65", snapshot, adjusted, adjusted, {}, calendar)
        stock = sample["stocks"][0]
        self.assertNotIn("退市", str(sample))
        self.assertEqual(stock["momentum_63"], str(Decimal(165) / Decimal(102) - 1))
        self.assertIsNone(momentum(adjusted["000001.SZ"], calendar, "62"))
        self.assertEqual(stock["return_20"], str(Decimal(165) / Decimal(145) - 1))
        self.assertIsNotNone(stock["volatility_63"])

    def test_volatility_skips_suspended_days_and_ignores_later_prices(self):
        calendar = [f"{i:02d}" for i in range(80)]
        bars = {day: bar("100" if i % 2 else "110") for i, day in enumerate(calendar)}
        bars["71"] = bar("1")
        swing = (Decimal(110) / 100 - 1 - (Decimal(100) / 110 - 1)) / 2
        self.assertAlmostEqual(float(volatility(bars, calendar, "70", sessions=2)), float(swing), places=12)
        bars["69"] = bar("999", status="0")
        self.assertAlmostEqual(float(volatility(bars, calendar, "70", sessions=2)), 0.05, places=12)
        self.assertEqual(Decimal(volatility(bars, calendar, "70", sessions=1)), 0)
        self.assertIsNone(volatility(bars, calendar, "02", sessions=3))

    def test_quarter_decisions_and_outcome_entry_after_decision(self):
        calendar = ["2015-03-30", "2015-03-31", "2015-04-01", "2015-04-08", "2015-04-30", "2015-06-30",
                    "2015-07-01"]
        self.assertEqual(quarter_decisions(calendar, 2015), ["2015-03-31", "2015-06-30", "2015-07-01"])
        bars = {day: bar("10") for day in calendar}
        index = {**bars, "2015-04-01": bar("11", preclose="10")}
        total_return = {day: str(100 + i) for i, day in enumerate(calendar)}
        sample = {"id": "s", "decision_date": "2015-03-31", "stocks": [{"symbol": "000001.SZ"}]}
        row = outcome_row(sample, {"000001.SZ": bars}, index, total_return, calendar)
        self.assertEqual(row["entry_date"], "2015-04-01")
        self.assertEqual(row["boundaries"], ["2015-04-08", "2015-04-30", "2015-06-30"])
        self.assertTrue(all(day >= "2015-04-01" for day, _ in row["stocks"]["000001.SZ"]["path"]))
        self.assertEqual(Decimal(row["index"]["entry_open"]), Decimal("101") * 11 / 10)
        self.assertEqual(row["index"]["path"][0], ["2015-04-01", "102"])
        self.assertIsNone(outcome_row({**sample, "decision_date": "2015-06-30"}, {"000001.SZ": bars}, index,
                                      total_return, calendar))
        with self.assertRaises(Invalid):
            outcome_row(sample, {"000001.SZ": bars}, index,
                        {d: v for d, v in total_return.items() if d != "2015-04-30"}, calendar)

    def test_combined_release_keeps_pools_separate_and_strips_current_names(self):
        def universe(root, pool, code, extra):
            (root / "universe").mkdir(parents=True)
            write_json(root / "request.json", {"pool": pool, "years": [2015]})
            write_json(root / "universe" / f"{pool}-2015.json", {
                "pool": pool, "year": 2015, "selection_date": "2014-12-31", "selected": [code],
                "candidates": [{"symbol": code, "market_cap": "1", "current_name": "退市某某", **extra}]})

        class Market:
            def __enter__(self):
                return self

            def __exit__(self, *_):
                return False

            def trade_dates(self, start, end):
                days = (date.fromisoformat(start) + timedelta(n) for n in range(700))
                return [{"calendar_date": d.isoformat(), "is_trading_day": "1" if d.weekday() < 5 else "0"}
                        for d in days if d.isoformat() <= end]

            def daily(self, code, start, end, flag="3", index=False):
                return [{"date": row["calendar_date"], **bar("10")} for row in self.trade_dates(start, end)
                        if row["is_trading_day"] == "1"]

        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder)
            universe(base / "tech", "tech", "000001.SZ", {"level1_name": "电子"})
            universe(base / "dividend", "dividend", "600000.SH", {"membership_source": "fund_090010_2014"})
            def closes(code, start, end):
                return [{"date": row["date"], "close": "100"} for row in Market().daily(code, start, end)]

            with patch("benchmark.pool_release.Baostock", Market), \
                    patch("benchmark.pool_release.csi_index_closes", closes):
                summary = build_pool_release([base / "tech", base / "dividend"], base / "release")
                with self.assertRaises(Invalid):
                    build_pool_release([base / "tech", base / "tech"], base / "again")
            self.assertEqual(summary["samples_by_pool"], {"dividend": 4, "tech": 4})
            verify_release(base / "release")
            samples = rows(base / "release" / "inputs.jsonl")
            self.assertEqual({s["id"].split("-")[0] for s in samples}, {"tech", "dividend"})
            self.assertNotIn("退市", (base / "release" / "universe" / "tech-2015.json").read_text(encoding="utf-8"))
            self.assertIsNone([s for s in samples if s["pool"] == "dividend"][0]["stocks"][0]["sw_level1_name"])


if __name__ == "__main__":
    unittest.main()
