"""合成公告与价格验证时点、证券身份和三日窗口；不访问公开接口。"""

import copy
import tempfile
import unittest
from datetime import datetime
from decimal import Decimal
from pathlib import Path

from benchmark.announcement_indicators import announcement_signal
from benchmark.strategy_runner import check_source, generate_weights
from scripts.benchmark_earnings_study import GRIDS, first_reports, source_for


def announcement(identity="a", day="2020-03-05", title="2019年年度报告", code="600000"):
    instant = datetime.fromisoformat(day + "T00:00:00+08:00")
    return {"secCode": code, "announcementTitle": title, "announcementId": identity,
            "announcementTime": int(instant.timestamp() * 1000), "adjunctUrl": f"finalpage/{day}/{identity}.PDF"}


def fixture():
    days = ["2020-03-02", "2020-03-03", "2020-03-04", "2020-03-05", "2020-03-06",
            "2020-03-09", "2020-03-10", "2020-03-11", "2020-03-12", "2020-03-13"]
    bars = {d: {"close": str(100 + i * 10), "tradestatus": "1"} for i, d in enumerate(days)}
    market = {d: "100" for d in days}
    return days, bars, market


class EarningsTimingTest(unittest.TestCase):
    def test_original_report_wins_and_foreign_security_and_revision_do_not_create_events(self):
        events = first_reports("600000.SH", [announcement("later", "2020-03-10", title="2019年年度报告（修订版）"),
                announcement("foreign", "2020-03-04", code="600001"), announcement("original"),
                announcement("summary", "2020-03-04", title="2019年年度报告摘要")])
        self.assertEqual(len(events), 1)
        self.assertEqual(events[0]["revision_id"], "original")
        self.assertEqual(events[0]["available_at"], "2020-03-05T23:59:59+08:00")
        self.assertFalse(events[0]["version_verified"])

    def test_date_only_publication_and_incomplete_future_window_are_unavailable(self):
        days, bars, market = fixture()
        events = first_reports("600000.SH", [announcement()])
        value, event, issue = announcement_signal(bars, days, "2020-03-05", events, market)
        self.assertIsNone(value)
        self.assertIsNone(event)
        self.assertEqual(issue, "no_prior_original_report")
        value, event, issue = announcement_signal(bars, days, "2020-03-06", events, market)
        self.assertIsNone(value)
        self.assertEqual(issue, "announcement_window_not_yet_observed")
        self.assertNotIn("window_end", event)

    def test_car_is_sum_of_three_excess_returns_across_weekend(self):
        days, bars, market = fixture()
        events = first_reports("600000.SH", [announcement()])
        value, event, issue = announcement_signal(bars, days, "2020-03-10", events, market)
        expected = sum((Decimal(bars[b]["close"]) / Decimal(bars[a]["close"]) - 1
                        for a, b in zip(days[2:5], days[3:6])))
        self.assertEqual(Decimal(value), expected)
        self.assertEqual(event["effective_session"], "2020-03-06")
        self.assertEqual(event["window_end"], "2020-03-09")
        self.assertIsNone(issue)

    def test_future_prices_and_announcements_do_not_change_decision_signal(self):
        days, bars, market = fixture()
        events = first_reports("600000.SH", [announcement()])
        expected = announcement_signal(bars, days, "2020-03-10", events, market)
        for d in days:
            if d > "2020-03-10":
                bars[d]["close"] = market[d] = "99999999"
        future = announcement("future", "2020-04-30", title="2020年第一季度报告")
        events = first_reports("600000.SH", [announcement(), future])
        self.assertEqual(announcement_signal(bars, days, "2020-03-10", events, market), expected)

    def test_missing_or_suspended_window_and_stale_event_are_explicit(self):
        days, bars, market = fixture()
        events = first_reports("600000.SH", [announcement()])
        suspended, missing = copy.deepcopy(bars), copy.deepcopy(bars)
        suspended["2020-03-06"]["tradestatus"] = "0"
        del missing["2020-03-06"]
        for changed in (suspended, missing):
            value, _, issue = announcement_signal(changed, days, "2020-03-10", events, market)
            self.assertIsNone(value)
            self.assertEqual(issue, "missing_or_suspended_announcement_window")
        del market["2020-03-04"]
        self.assertEqual(announcement_signal(bars, days, "2020-03-10", events, market)[2], "missing_or_suspended_announcement_window")
        self.assertEqual(announcement_signal(bars, days, "2020-12-01", events, market)[2], "stale_report_event")

    def test_candidates_pass_sandbox_and_keep_missing_event_budgets_as_cash(self):
        sample = {"id": "test", "stocks": [{"symbol": "600000.SH", "trade_status": "trading",
                  "volatility_63": "0.02", "market_cap_at_selection": "100",
                  "long_history": {"report_car3": None, "momentum_12_2": "0.1"}}]}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "strategy.py"
            for family, configs in GRIDS.items():
                source = source_for(family, configs[0])
                self.assertEqual(check_source(source), [], family)
                path.write_text(source, encoding="utf-8")
                self.assertEqual(generate_weights(path, [sample]), [], family)
                active = copy.deepcopy(sample)
                active["stocks"][0]["long_history"]["report_car3"] = "0.1"
                result = generate_weights(path, [active])
                self.assertGreater(sum(Decimal(r["weight"]) for r in result), 0)
                self.assertLessEqual(sum(Decimal(r["weight"]) for r in result), 1)
