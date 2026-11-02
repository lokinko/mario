import unittest
from decimal import Decimal
from pathlib import Path

from benchmark.strategy_runner import generate_weights


STRATEGIES = Path(__file__).resolve().parents[1] / "strategies"
NAMES = ("trend_risk_budget", "trend_state_exit", "trend_profit_ladder")


def stock(volatility="0.02", short="0", momentum="0.5", status="trading"):
    return {"symbol": "000001.SZ", "trade_status": status, "momentum_63": momentum,
            "volatility_63": volatility, "return_20": short, "sw_level1_name": "电子"}


def positions(name, stocks, pool="tech"):
    return generate_weights(STRATEGIES / name / "strategy.py", [{"id": "sample", "pool": pool, "stocks": stocks}])


class StrategyMechanismsTest(unittest.TestCase):
    def test_risk_budget_reduces_exposure_and_never_leverages(self):
        high_risk = positions("trend_risk_budget", [stock(volatility="0.064")])
        self.assertEqual(high_risk[0]["weight"], "0.50000000")
        low_risk = positions("trend_risk_budget", [stock(volatility="0.01")])
        self.assertEqual(low_risk[0]["weight"], "1.00000000")
        self.assertEqual(low_risk[0]["grid"], {"step": "0.08", "lots": 2, "initial": 1})
        for missing in (None, "0"):
            with self.subTest(volatility=missing):
                result = positions("trend_risk_budget", [stock(volatility=missing)])
                self.assertEqual(result[0]["weight"], "1.00000000")

    def test_risk_budget_uses_all_selected_stocks_not_only_the_first(self):
        first, second = stock(volatility="0.02"), stock(volatility="0.06")
        second.update(symbol="000002.SZ", sw_level1_name="通信")
        result = positions("trend_risk_budget", [first, second])
        # Average risk is 4%; 3.2% / 4% gives 80% budget, split equally.
        self.assertEqual({p["weight"] for p in result}, {"0.40000000"})
        self.assertEqual(sum(Decimal(p["weight"]) for p in result), Decimal("0.8"))

    def test_negative_short_return_replaces_grid_with_an_exit_only_in_tech(self):
        negative = positions("trend_state_exit", [stock(short="-0.01")])[0]
        self.assertEqual(negative["exit"], {"take_profit": "0.1", "stop_loss": "0.3", "sell_fraction": "1"})
        self.assertNotIn("grid", negative)
        for short in ("0", "0.01", None):
            with self.subTest(short=short):
                unchanged = positions("trend_state_exit", [stock(short=short)])[0]
                self.assertEqual(unchanged["grid"], {"step": "0.08", "lots": 2, "initial": 1})
                self.assertNotIn("exit", unchanged)
        dividend = positions("trend_state_exit", [stock(short="-0.01")], "dividend")[0]
        self.assertEqual(dividend["exit"], {"take_profit": "0.1", "sell_fraction": "1"})

    def test_profit_ladder_scales_grid_clamps_bands_and_starts_fully_invested(self):
        for volatility, expected in (("0.001", "0.04"), ("0.01", "0.08"), ("0.1", "0.3")):
            with self.subTest(volatility=volatility):
                result = positions("trend_profit_ladder", [stock(volatility=volatility)])[0]
                self.assertEqual(Decimal(result["grid"]["step"]), Decimal(expected))
                self.assertEqual(result["grid"]["lots"], 4)
                self.assertEqual(result["grid"]["initial"], 4)
                self.assertNotIn("exit", result)
        missing = positions("trend_profit_ladder", [stock(volatility=None)])[0]
        self.assertNotIn("grid", missing)
        self.assertNotIn("exit", missing)
        dividend = positions("trend_profit_ladder", [stock()], "dividend")[0]
        self.assertEqual(dividend["exit"], {"take_profit": "0.1", "sell_fraction": "1"})

    def test_unavailable_and_empty_pools_leave_cash_and_weak_trends_use_fallback(self):
        for name in NAMES:
            with self.subTest(name=name):
                self.assertEqual(positions(name, []), [])
                self.assertEqual(positions(name, [stock(status="no_bar")]), [])
                self.assertEqual(positions(name, [stock(volatility=None, momentum=None)]), [])
                fallback = positions(name, [stock(momentum="-0.1")])
                self.assertEqual(fallback[0]["symbol"], "000001.SZ")


if __name__ == "__main__":
    unittest.main()
