"""组合风险、缺失历史及周窗口测试，使用合成价格。"""

import copy
import tempfile
import unittest
from datetime import date, timedelta
from decimal import Decimal
from pathlib import Path

from benchmark.strategy_runner import check_source, generate_weights
from benchmark.files import Invalid
from benchmark.price_indicators import monthly_features, risk_features
from scripts.benchmark_risk_study import GRIDS, source_for


def fixture():
    calendar, bars = [], {}
    day = date(2016, 1, 1)
    while day <= date(2021, 12, 31):
        if day.weekday() < 5:
            key = day.isoformat()
            calendar.append(key)
            bars[key] = {"date": key, "code": "sh.600000", "close": str(100 + len(calendar)),
                         "high": str(110 + len(calendar)), "tradestatus": "1"}
        day += timedelta(days=1)
    return calendar, bars


def stock(symbol, returns):
    return {"symbol": symbol, "trade_status": "trading", "momentum_63": "0.5", "return_20": "0",
            "volatility_63": "0.02", "sw_level1_name": symbol,
            "long_history": {"daily_returns_1m": returns}}


class RiskFeaturesTest(unittest.TestCase):
    def test_three_calendar_years_weekly_frequency_and_future_invariance(self):
        calendar, bars = fixture()
        decision = "2020-12-31"
        result = risk_features(bars, calendar, decision)
        self.assertGreater(Decimal(result["volatility_3y_weekly"]), 0)
        # 改动非周末且非月末的日价，周频波动应完全不变。
        edited = copy.deepcopy(bars)
        edited["2020-12-23"]["close"] = "99999"
        self.assertEqual(risk_features(edited, calendar, decision)["volatility_3y_weekly"], result["volatility_3y_weekly"])
        for day in edited:
            if day > decision:
                edited[day]["close"] = edited[day]["high"] = "999999999"
        past_bars = {d: b for d, b in edited.items() if d <= decision}
        self.assertEqual(risk_features(edited, calendar, decision), risk_features(past_bars, [d for d in calendar if d <= decision], decision))
        # 不足三年、周端点缺行或端点停牌，不能退化成短窗口波动。
        for altered in ({d: b for d, b in bars.items() if d >= "2020"}, copy.deepcopy(bars)):
            if "2019-06-28" in altered:
                altered["2019-06-28"]["tradestatus"] = "0"
            self.assertIsNone(risk_features(altered, calendar, decision)["volatility_3y_weekly"])

    def test_monthly_returns_distinguish_suspension_from_unknown_gap(self):
        calendar, bars = fixture()
        bars["2020-12-02"]["tradestatus"] = "0"
        result = monthly_features(bars, calendar, "2020-12-31")
        daily = result["daily_returns_1m"]
        self.assertEqual(daily["2020-12-02"], "0")
        self.assertEqual(Decimal(daily["2020-12-03"]), Decimal(bars["2020-12-03"]["close"]) / Decimal(bars["2020-12-01"]["close"]) - 1)
        self.assertTrue(all(d.startswith("2020-12-") for d in daily))
        del bars["2020-12-02"]
        result = monthly_features(bars, calendar, "2020-12-31")
        self.assertIsNone(result["daily_returns_1m"]["2020-12-02"])
        self.assertIsNone(result["daily_returns_1m"]["2020-12-03"])
        self.assertIn("daily_returns_1m", result["issues"])


class BasketOverlayTest(unittest.TestCase):
    def decide(self, stocks):
        path = Path(__file__).parents[1] / "strategies" / "trend_basket_risk" / "strategy.py"
        return generate_weights(path, [{"id": "test", "pool": "tech", "stocks": stocks}])

    def test_registered_strategy_requires_explicit_v6_history_contract(self):
        old = stock("600000.SH", {})
        del old["long_history"]
        with self.assertRaisesRegex(Invalid, "long_history"):
            self.decide([old])

    def test_variance_budget_scales_weights_and_keeps_execution_rules(self):
        high = {"2020-12-01": "0.1", "2020-12-02": "-0.1"}
        result = self.decide([stock("600000.SH", high)])
        self.assertEqual(result[0]["weight"], "0.16000000")
        self.assertEqual(result[0]["grid"], {"step": "0.08", "lots": 2, "initial": 1})
        low = {k: "0" for k in high}
        self.assertEqual(self.decide([stock("600000.SH", low)])[0]["weight"], "1.00000000")

    def test_covariance_changes_risk_and_incomplete_vectors_stay_cash(self):
        a = {"2020-12-01": "0.1", "2020-12-02": "-0.1"}
        b = {k: str(-Decimal(v)) for k, v in a.items()}
        # 核心/卫星权重为7/8与1/8；异向波动后日标准差为7.5%，而非个股均值10%。
        result = self.decide([stock("600000.SH", a), stock("600001.SH", b)])
        expected = (Decimal("0.04") / Decimal("0.075")) ** 2
        self.assertAlmostEqual(sum(Decimal(r["weight"]) for r in result), expected, places=7)
        self.assertGreater(expected, Decimal("0.16"))
        broken = copy.deepcopy(a)
        broken["2020-12-01"] = None
        self.assertEqual(self.decide([stock("600000.SH", broken)]), [])
        self.assertEqual(self.decide([stock("600000.SH", a), stock("600001.SH", {})]), [])

    def test_all_other_families_pass_sandbox_on_realistic_synthetic_features(self):
        calendar, bars = fixture()
        item = stock("600000.SH", {})
        item["long_history"] = risk_features(bars, calendar, "2020-12-31")
        item["market_cap_at_selection"] = "10000"
        with tempfile.TemporaryDirectory() as directory:
            for family, configs in GRIDS.items():
                source = source_for(family, configs[0])
                self.assertEqual(check_source(source), [], family)
                path = Path(directory) / "strategy.py"
                path.write_text(source, encoding="utf-8")
                generate_weights(path, [{"id": "test", "pool": "tech", "stocks": [item]}])
