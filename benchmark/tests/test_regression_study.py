"""合成价格核验回归、时点、窗口和权重倾斜，不使用真实收益作断言。"""

import copy
import tempfile
import unittest
from decimal import Decimal, localcontext
from pathlib import Path

from benchmark.files import Invalid
from benchmark.strategy_runner import check_source, generate_weights
from scripts.benchmark_regression_study import GRIDS, regression_features, source_for
from benchmark.tests.test_long_history_study import fixture


def stock(code, score, risk="0.02", returns=None):
    return {"symbol": code, "trade_status": "trading", "momentum_63": "0.2", "return_20": "0.05",
            "volatility_63": risk, "market_cap_at_selection": "100", "sw_level1_name": code,
            "long_history": {"clenow_90": score, "clenow_r2_90": "0.8",
                             "clenow_mark_90": score, "clenow_mark_r2_90": "0.8",
                             "daily_returns_1m": returns or {"2020-12-01": "0", "2020-12-02": "0"}}}


class RegressionStudyTest(unittest.TestCase):
    def test_exponential_path_and_price_units_have_analytic_score(self):
        days, bars = fixture()
        with localcontext() as context:
            context.prec = 50
            for i, day in enumerate(days[-90:]):
                bars[day]["close"] = str((Decimal("0.002") * i).exp())
            expected = ((Decimal("0.002") * 252).exp() - 1) * 100
            result = regression_features(bars, days, days[-1])
            self.assertLess(abs(Decimal(result["clenow_90"]) - expected), Decimal("1e-34"))
            self.assertGreater(Decimal(result["clenow_r2_90"]), Decimal("0.99999999999999999999999"))
            for day in days[-90:]:
                bars[day]["close"] = str(Decimal(bars[day]["close"]) * 10000)
            scaled = regression_features(bars, days, days[-1])
            self.assertEqual(scaled["clenow_90"], result["clenow_90"])

    def test_same_endpoint_jump_is_penalized_and_constant_prices_are_zero(self):
        days, bars = fixture()
        with localcontext() as context:
            context.prec = 50
            for i, day in enumerate(days[-90:]):
                bars[day]["close"] = str((Decimal("0.002") * i).exp())
        smooth = regression_features(bars, days, days[-1])
        for day in days[-90:-1]:
            bars[day]["close"] = "1"
        jump = regression_features(bars, days, days[-1])
        self.assertLess(Decimal(jump["clenow_90"]), Decimal(smooth["clenow_90"]))
        self.assertLess(Decimal(jump["clenow_r2_90"]), Decimal("0.1"))
        bars[days[-1]]["close"] = "1"
        flat = regression_features(bars, days, days[-1])
        self.assertEqual(Decimal(flat["clenow_90"]), 0)
        self.assertEqual(Decimal(flat["clenow_r2_90"]), 0)

    def test_future_mutation_cannot_change_decision_features(self):
        days, bars = fixture()
        cutoff = "2020-12-31"
        expected = regression_features(bars, days, cutoff)
        for day in days:
            if day > cutoff:
                bars[day]["close"] = "NaN"
        self.assertEqual(regression_features(bars, days, cutoff), expected)

    def test_incomplete_and_suspended_windows_are_null_without_extension(self):
        days, bars = fixture()
        for change in ("short", "missing", "suspended"):
            modified = copy.deepcopy(bars)
            if change == "short":
                modified = {d: modified[d] for d in days[-89:]}
            elif change == "missing":
                del modified[days[-45]]
            else:
                modified[days[-45]]["tradestatus"] = "0"
            result = regression_features(modified, days, days[-1])
            self.assertIsNone(result["clenow_90"])
            self.assertIsNone(result["clenow_r2_90"])
            self.assertIn("clenow_90", result["issues"])
            if change != "suspended":
                self.assertIsNone(result["clenow_mark_90"])
            else:
                self.assertIsNotNone(result["clenow_mark_90"])
        bars[days[-1]]["close"] = "0"
        with self.assertRaises(Invalid):
            regression_features(bars, days, days[-1])

    def test_only_declared_suspensions_use_prior_trade_and_do_not_read_suspended_quote(self):
        days, bars = fixture()
        day, previous = days[-45], days[-46]
        expected_bars = copy.deepcopy(bars)
        expected_bars[day]["close"] = expected_bars[previous]["close"]
        expected = regression_features(expected_bars, days, days[-1])["clenow_90"]
        bars[day].update(tradestatus="0", close="NaN")
        actual = regression_features(bars, days, days[-1])
        self.assertEqual(actual["clenow_mark_90"], expected)
        self.assertIsNone(actual["clenow_90"])

    def run_candidate(self, family, params, stocks, pool="tech"):
        source = source_for(family, params)
        self.assertEqual(check_source(source), [])
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "strategy.py"
            path.write_text(source, encoding="utf-8")
            return generate_weights(path, [{"id": "test", "pool": pool, "stocks": stocks}])

    def test_all_parameter_candidates_pass_isolation_and_missing_signals_keep_cash(self):
        for family, configs in GRIDS.items():
            for params in configs:
                with self.subTest(family=family, params=params):
                    result = self.run_candidate(family, params, [stock("600000.SH", "10")])
                    self.assertGreater(sum(Decimal(r["weight"]) for r in result), 0)
                    self.assertEqual(self.run_candidate(family, params, [stock("600000.SH", None)]), [])

    def test_tilt_scales_only_known_budget_and_single_position_has_no_artificial_tilt(self):
        params = dict(base="equal", budget=False, target="0.04", power=2, clock="strict")
        a, b = stock("600000.SH", "-1"), stock("600001.SH", "10")
        result = self.run_candidate("regression_tilt", params, [a, b])
        self.assertEqual([r["weight"] for r in result], ["0.20000000", "0.80000000"])
        b["long_history"]["clenow_90"] = None
        result = self.run_candidate("regression_tilt", params, [a, b])
        self.assertEqual([r["weight"] for r in result], ["0.50000000"])
        b["long_history"]["clenow_90"] = "-1"
        result = self.run_candidate("regression_tilt", params, [a, b])
        self.assertEqual([r["weight"] for r in result], ["0.50000000", "0.50000000"])

    def test_budget_is_recomputed_after_tilt_and_exit_contract_survives(self):
        params = dict(base="equal", budget=True, target="0.04", power=2, clock="strict")
        a = stock("600000.SH", "-1", returns={"2020-12-01": "0.1", "2020-12-02": "-0.1"})
        b = stock("600001.SH", "10", returns={"2020-12-01": "-0.1", "2020-12-02": "0.1"})
        # 等权时风险相消，倾斜为20/80后日波动6%，方差预算4/9。
        result = self.run_candidate("regression_tilt", params, [a, b])
        self.assertAlmostEqual(sum(Decimal(r["weight"]) for r in result), Decimal(4) / 9, places=7)
        params.update(base="trend_core_satellite", budget=False)
        result = self.run_candidate("regression_tilt", params, [a], pool="dividend")
        self.assertEqual(result[0]["exit"], {"take_profit": "0.1", "sell_fraction": "1"})

    def test_selected_candidate_only_needs_minimal_upgrade_and_refuses_missing_new_field(self):
        params = dict(base="trend_core_satellite", budget=True, target="0.04", power=2, clock="mark")
        item = stock("600000.SH", "10")
        item["long_history"] = {k:v for k,v in item["long_history"].items() if k in ("clenow_mark_90", "daily_returns_1m")}
        self.assertEqual(self.run_candidate("regression_tilt", params, [item])[0]["weight"], "1.00000000")
        del item["long_history"]["clenow_mark_90"]
        with self.assertRaises(Invalid):
            self.run_candidate("regression_tilt", params, [item])

    def test_registered_strategy_requires_new_contract_and_retains_take_profit(self):
        path = Path(__file__).resolve().parents[1] / "strategies/trend_regression_tilt/strategy.py"
        item = stock("600000.SH", "10")
        item["long_history"] = {k:v for k,v in item["long_history"].items() if k in ("clenow_mark_90", "daily_returns_1m")}
        sample = {"id": "test", "pool": "dividend", "stocks": [item]}
        result = generate_weights(path, [sample])
        self.assertEqual(result[0]["weight"], "1.00000000")
        self.assertEqual(result[0]["exit"], {"take_profit": "0.1", "sell_fraction": "1"})
        del item["long_history"]["clenow_mark_90"]
        with self.assertRaises(Invalid):
            generate_weights(path, [sample])
