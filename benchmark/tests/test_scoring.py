import unittest
from decimal import Decimal

from benchmark.files import Invalid
from benchmark.pool_release import momentum, outcome_row, quarter_decisions, sample_rows
from benchmark.scoring import (baseline_weights, boundaries, can_buy, evaluate_weights, portfolio_segments,
                               score)


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

    def test_baselines_round_weights_down_and_skip_unlisted(self):
        samples = [{"id": "s1", "stocks": [
            {"symbol": f"00000{i}.SZ", "trade_status": "trading", "momentum_63": str(i),
             "market_cap_at_selection": str(10 - i)} for i in range(1, 4)]
            + [{"symbol": "000009.SZ", "trade_status": "no_bar", "momentum_63": None,
                "market_cap_at_selection": "99"}]}]
        equal = list(baseline_weights(samples, "equal"))
        self.assertEqual(len(equal), 3)
        self.assertLessEqual(sum(Decimal(r["weight"]) for r in equal), 1)
        self.assertEqual([r["symbol"] for r in baseline_weights(samples, "momentum", top=1)], ["000003.SZ"])
        self.assertEqual([r["symbol"] for r in baseline_weights(samples, "size", top=1)], ["000001.SZ"])
        self.assertEqual(list(baseline_weights(samples, "cash")), [])


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

    def test_quarter_decisions_and_outcome_entry_after_decision(self):
        calendar = ["2015-03-30", "2015-03-31", "2015-04-01", "2015-04-08", "2015-04-30", "2015-06-30",
                    "2015-07-01"]
        self.assertEqual(quarter_decisions(calendar, 2015), ["2015-03-31", "2015-06-30", "2015-07-01"])
        bars = {day: bar("10") for day in calendar}
        sample = {"id": "s", "decision_date": "2015-03-31", "stocks": [{"symbol": "000001.SZ"}]}
        row = outcome_row(sample, {"000001.SZ": bars}, bars, calendar)
        self.assertEqual(row["entry_date"], "2015-04-01")
        self.assertEqual(row["boundaries"], ["2015-04-08", "2015-04-30", "2015-06-30"])
        self.assertTrue(all(day >= "2015-04-01" for day, _ in row["stocks"]["000001.SZ"]["path"]))
        self.assertIsNone(outcome_row({**sample, "decision_date": "2015-06-30"}, {"000001.SZ": bars}, bars, calendar))


if __name__ == "__main__":
    unittest.main()
