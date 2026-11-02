"""富贵策略的信号边界、现金约束与研究输入隔离；仅使用合成数据。"""

import copy
import tempfile
import unittest
from datetime import date, timedelta
from decimal import Decimal
from pathlib import Path

from benchmark.files import Invalid, digest, rows, write_json, write_rows
from benchmark.strategy_runner import check_source, generate_weights
from scripts.benchmark_fugui_study import STRATEGY, boll, build, candidate, features, load_caches, macd


def fixture(month_prices):
    days, bars = [], {}
    day = date(2018, 1, 1)
    while (day.year - 2018) * 12 + day.month - 1 < len(month_prices):
        if day.weekday() < 5:
            key = day.isoformat()
            price = str(month_prices[(day.year - 2018) * 12 + day.month - 1])
            days.append(key)
            bars[key] = {"date": key, "code": "sh.600000", "close": price, "tradestatus": "1"}
        day += timedelta(days=1)
    return days, bars


class FuguiSignalsTest(unittest.TestCase):
    def test_bands_use_population_deviation_and_macd_requires_warmup(self):
        prices = [Decimal(100)] * 20
        self.assertEqual(boll(prices), (Decimal(100), Decimal(100), Decimal(100)))
        self.assertIsNone(boll(prices[:19]))
        self.assertIsNone(macd([Decimal(100)] * 34))
        self.assertEqual(macd([Decimal(100)] * 35), (0, 0, 0))
        prices[-1] = Decimal(120)
        self.assertEqual(boll(prices), (101 - 2 * Decimal(19).sqrt(), Decimal(101), 101 + 2 * Decimal(19).sqrt()))

    def test_future_price_and_future_calendar_do_not_change_any_signal(self):
        days, bars = fixture([100] * 36 + [140, 140, 100])
        cutoff = days[-1]
        expected = features(bars, days, cutoff)
        future = (date.fromisoformat(cutoff) + timedelta(days=10)).isoformat()
        bars[future] = {"close": "999999999999", "tradestatus": "1"}
        self.assertEqual(features(bars, days + [future], cutoff), expected)

    def test_monthly_accumulation_requires_half_price_and_negative_monthly_macd(self):
        prices = [200 * Decimal("0.98") ** i for i in range(36)]
        days, bars = fixture(prices)
        self.assertTrue(features(bars, days, days[-1])["monthly_low"])
        days, bars = fixture([100] * 36)
        self.assertFalse(features(bars, days, days[-1])["monthly_low"])

    def test_only_first_support_retest_counts_and_missing_period_resets_history(self):
        days, bars = fixture([100] * 35 + [140, 140, 100, 100])
        first = max(d for d in days if d[:7] == "2021-02")
        self.assertTrue(features(bars, days, first)["first_pullback"])
        self.assertFalse(features(bars, days, days[-1])["first_pullback"])
        del bars["2021-01-15"]
        result = features(bars, days, first)
        self.assertIsNone(result["monthly_low"])
        self.assertIsNone(result["first_pullback"])
        self.assertEqual(result["months"], 1)
        # 月内有缺行不能靠月底价覆盖；日线已有20个完整观察时仍可计算。
        self.assertIsNotNone(result["daily_lower"])

    def test_known_suspension_uses_prior_price_but_unknown_gap_does_not(self):
        days, bars = fixture([100] * 36)
        cutoff = days[-1]
        reference = features(bars, days, cutoff)
        bars[days[-2]] = {"close": "9999999", "tradestatus": "0"}
        self.assertEqual(features(bars, days, cutoff), reference)
        del bars[days[-2]]
        changed = features(bars, days, cutoff)
        self.assertIsNone(changed["daily_lower"])
        self.assertIsNone(changed["monthly_low"])

    def test_candidate_requires_technical_contract_and_leaves_vacant_slots_in_cash(self):
        self.assertEqual(check_source(STRATEGY), [])
        with tempfile.TemporaryDirectory() as folder:
            code = Path(folder) / "strategy.py"
            code.write_text(STRATEGY, encoding="utf-8")
            stock = {"symbol": "600000.SH", "trade_status": "trading", "market_cap_at_selection": "100000000000",
                     "volatility_63": "0.025", "fugui": {"monthly_low": True, "first_pullback": None,
                     "daily_lower": None, "weekly_lower": None}}
            sample = {"id": "s1", "pool": "dividend", "stocks": [stock]}
            self.assertEqual(generate_weights(code, [sample])[0]["weight"], "0.18000000")
            for key, value in (("market_cap_at_selection", "99999999999"), ("trade_status", "suspended"),
                               ("volatility_63", None), ("volatility_63", "0.0251")):
                wrong = copy.deepcopy(sample)
                wrong["stocks"][0][key] = value
                self.assertEqual(generate_weights(code, [wrong]), [], key)
            stock["fugui"]["monthly_low"] = None
            self.assertEqual(generate_weights(code, [sample]), [])
            stock.pop("fugui")
            with self.assertRaises(Invalid):
                generate_weights(code, [sample])
            sample["pool"] = "tech"
            self.assertEqual(generate_weights(code, [sample]), [])

    def test_pullback_needs_both_lower_bands_and_budget_never_exceeds_ninety_percent(self):
        with tempfile.TemporaryDirectory() as folder:
            code = Path(folder) / "strategy.py"
            code.write_text(STRATEGY, encoding="utf-8")
            stocks = [{"symbol": f"60000{i}.SH", "trade_status": "trading", "market_cap_at_selection": "200000000000",
                       "volatility_63": str(Decimal(i + 1) / 1000), "fugui": {"monthly_low": False,
                       "first_pullback": True, "daily_lower": True, "weekly_lower": True}} for i in range(7)]
            sample = {"id": "s", "pool": "dividend", "stocks": list(reversed(stocks))}
            result = generate_weights(code, [sample])
            self.assertEqual([r["symbol"] for r in result], [f"60000{i}.SH" for i in range(5)])
            self.assertEqual(sum(Decimal(r["weight"]) for r in result), Decimal("0.9"))
            for stock in stocks:
                stock["fugui"]["weekly_lower"] = None
            self.assertEqual(generate_weights(code, [sample]), [])


class FuguiBuildTest(unittest.TestCase):
    def test_build_preserves_all_original_fields_and_label_bytes_and_never_overwrites(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            release, cache, destination = root / "release", root / "cache", root / "enriched"
            release.mkdir()
            history = cache / "raw" / "baostock" / "history"
            history.mkdir(parents=True)
            days, bars = fixture([100] * 36)
            write_json(history / "600000.SH-synthetic-adjust1.json", list(bars.values()))
            write_json(history / "trade_dates-synthetic.json", [{"calendar_date": d, "is_trading_day": "1"} for d in days])
            sample = {"id": "s", "pool": "dividend", "decision_date": days[-1], "stocks": [{"symbol": "600000.SH",
                      "momentum_63": "0", "return_20": "0", "long_history": {"sentinel": "preserve"}}]}
            write_rows(release / "inputs.jsonl", [sample])
            (release / "outcomes.jsonl").write_bytes(b'{"secret":"not a signal"}\n')
            write_json(release / "manifest.json", {"schema_version": 1, "files": {
                p.name: digest(p) for p in release.iterdir()}})
            report = build(release, [cache], destination)
            self.assertTrue(report["outcomes_unchanged"])
            self.assertFalse(report["fundamentals_available"])
            result = rows(destination / "inputs.jsonl")[0]
            result["stocks"][0].pop("fugui")
            self.assertEqual(result, sample)
            with self.assertRaises(Invalid):
                build(release, [cache], destination)
            bars[days[-1]]["close"] = "101"
            write_json(history / "600000.SH-overlap-adjust1.json", [bars[days[-1]]])
            with self.assertRaisesRegex(Invalid, "重叠行情不一致"):
                load_caches([cache], {"600000.SH"})

    def test_candidate_is_generated_outside_formal_strategy_directory(self):
        with tempfile.TemporaryDirectory() as folder:
            out = Path(folder) / "candidates"
            candidate(out)
            self.assertEqual((out / "fugui" / "strategy.py").read_text(encoding="utf-8"), STRATEGY)
            self.assertTrue((out / "fugui" / "strategy.md").read_text(encoding="utf-8").startswith("# 富贵策略"))
            with self.assertRaises(Invalid):
                candidate(out)
