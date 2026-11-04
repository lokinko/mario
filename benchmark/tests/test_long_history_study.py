"""长窗口研究的时点边界；所有行情为合成数据，不依赖网络或真实证券。"""

import copy
import tempfile
import unittest
from datetime import date, timedelta
from decimal import Decimal
from pathlib import Path

from benchmark.files import Invalid, digest, read_json, rows, write_json, write_rows
from benchmark.release import verify_release
from benchmark.price_indicators import long_features as features
from benchmark.strategy_runner import check_source, generate_weights
from scripts.benchmark_long_history_study import FIELDS, GRIDS, build, source_for


def fixture():
    calendar, bars = [], {}
    day = date(2019, 1, 1)
    while day <= date(2021, 12, 31):
        if day.weekday() < 5:
            key = day.isoformat()
            calendar.append(key)
            bars[key] = {"date": key, "code": "sh.600000", "close": str(100 + len(calendar)),
                         "high": str(110 + len(calendar)), "tradestatus": "1"}
        day += timedelta(days=1)
    return calendar, bars


class LongFeatureTest(unittest.TestCase):
    def test_future_prices_and_calendar_cannot_change_signals(self):
        calendar, bars = fixture()
        cutoff = "2020-12-31"
        past_calendar = [d for d in calendar if d <= cutoff]
        past_bars = {d: b for d, b in bars.items() if d <= cutoff}
        expected = features(past_bars, past_calendar, cutoff)
        future = copy.deepcopy(bars)
        for d in future:
            if d > cutoff:
                future[d]["close"] = future[d]["high"] = "999999999999"
        self.assertEqual(features(future, calendar, cutoff), expected)
        self.assertEqual(expected["issues"], {})

    def test_month_end_offsets_and_exact_ten_month_mean(self):
        calendar, bars = fixture()
        result = features(bars, calendar, "2020-12-31")
        p = lambda d: Decimal(bars[d]["close"])
        self.assertEqual(Decimal(result["momentum_6m_skip1"]), p("2020-11-30") / p("2020-05-29") - 1)
        self.assertEqual(Decimal(result["momentum_12m_skip1"]), p("2020-11-30") / p("2019-11-29") - 1)
        self.assertEqual(Decimal(result["momentum_12_2"]), p("2020-11-30") / p("2019-12-31") - 1)
        ends = {}
        for d in calendar:
            if "2020-03" <= d[:7] <= "2020-12":
                ends[d[:7]] = d
        sma = sum(p(d) for d in ends.values()) / 10
        self.assertEqual(Decimal(result["price_to_sma_10m"]), p("2020-12-31") / sma - 1)
        self.assertEqual(Decimal(result["information_discreteness"]), -1)

    def test_zero_returns_count_but_suspended_days_do_not(self):
        calendar, bars = fixture()
        for d in bars:
            bars[d]["close"] = "100"
        bars["2020-11-27"]["close"] = "101"
        bars["2020-11-30"]["close"] = "101"
        a = features(bars, calendar, "2020-12-31")
        count = len([d for d in calendar if "2019-12-31" < d <= "2020-11-30"])
        self.assertEqual(a["fip_observations"], count)
        self.assertEqual(Decimal(a["information_discreteness"]), -Decimal(1) / count)
        bars["2020-01-02"]["tradestatus"] = "0"
        b = features(bars, calendar, "2020-12-31")
        self.assertEqual(b["fip_observations"], count - 1)
        self.assertEqual(Decimal(b["information_discreteness"]), -Decimal(1) / (count - 1))
        bars["2020-11-27"]["close"] = bars["2020-11-30"]["close"] = "99"
        self.assertEqual(Decimal(features(bars, calendar, "2020-12-31")["information_discreteness"]), -Decimal(1) / (count - 1))

    def test_missing_interior_bar_and_suspended_month_end_are_explicit(self):
        calendar, bars = fixture()
        del bars["2020-06-10"]
        result = features(bars, calendar, "2020-12-31")
        self.assertIsNone(result["close_to_high_12m"])
        self.assertIsNone(result["information_discreteness"])
        self.assertIn("information_discreteness", result["issues"])
        self.assertIsNotNone(result["momentum_12m"])
        bars["2020-11-30"]["tradestatus"] = "0"
        result = features(bars, calendar, "2020-12-31")
        self.assertIsNone(result["price_to_sma_10m"])
        self.assertIsNone(result["momentum_12_2"])
        self.assertIsNotNone(result["momentum_12m"])

    def test_new_listing_keeps_missing_history_instead_of_shortening_window(self):
        calendar, bars = fixture()
        bars = {d: b for d, b in bars.items() if d >= "2020-10-01"}
        result = features(bars, calendar, "2020-12-31")
        self.assertTrue(all(result[k] is None for k in FIELDS))
        self.assertEqual(set(result["issues"]), set(FIELDS))


class LongStudyBuildTest(unittest.TestCase):
    def test_build_preserves_original_fields_and_labels_and_rejects_truncation(self):
        calendar, bars = fixture()
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            release, history = root / "release", root / "history"
            release.mkdir()
            (history / "raw").mkdir(parents=True)
            sample = {"id": "synthetic", "decision_date": "2020-12-31", "pool": "tech", "stocks": [
                {"symbol": "600000.SH", "trade_status": "trading", "momentum_63": None, "return_20": None}]}
            write_rows(release / "inputs.jsonl", [sample])
            (release / "outcomes.jsonl").write_bytes(b'{"secret_future_label":"never_a_feature"}\n')
            write_json(release / "manifest.json", {"schema_version": 1, "files": {
                p.name: digest(p) for p in release.iterdir()}})
            request = {"symbol": "600000.SH", "start": calendar[0], "end": calendar[-1]}
            write_json(history / "request.json", {"adjustflag": "1", "source": "https://www.baostock.com/",
                       "release_manifest_sha256": digest(release / "manifest.json"), "requests": [request]})
            raw = history / "raw" / "600000.SH.json"
            write_json(raw, {"request": request, "status": "completed", "rows": list(bars.values())})
            calendar_path = root / "calendar.json"
            write_json(calendar_path, calendar)
            output = root / "output"
            report = build(release, history, calendar_path, output)
            self.assertTrue(report["outcomes_unchanged"])
            self.assertTrue(verify_release(output)["valid"])
            enriched = rows(output / "inputs.jsonl")[0]
            self.assertEqual({k: v for k, v in enriched["stocks"][0].items() if k != "long_history"}, sample["stocks"][0])
            self.assertNotIn("secret_future_label", (output / "inputs.jsonl").read_text())
            with self.assertRaises(Invalid):
                build(release, history, calendar_path, output)
            # 一份状态为 completed 却停在过去的缓存不能生成有效研究切片。
            write_json(raw, {"request": request, "status": "completed", "rows": [b for d, b in bars.items() if d < "2020"]})
            with self.assertRaisesRegex(Invalid, "截断历史"):
                build(release, history, calendar_path, root / "truncated")

    def test_all_candidates_pass_isolation_and_incomplete_history_stays_cash(self):
        sample = {"id": "synthetic", "stocks": [{"symbol": "600000.SH", "trade_status": "trading",
                  "volatility_63": "0.02", "long_history": {k: None for k in FIELDS}}]}
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "strategy.py"
            for family, configs in GRIDS.items():
                source = source_for(family, configs[0])
                self.assertEqual(check_source(source), [], family)
                path.write_text(source, encoding="utf-8")
                self.assertEqual(generate_weights(path, [sample]), [], family)
