import unittest
from decimal import Decimal
from pathlib import Path

from benchmark.strategy_runner import generate_weights


STRATEGIES = Path(__file__).resolve().parents[1] / "strategies"
CONFIGURATIONS = {
    "trend_core_satellite": ("0.25", "0.15"),
    "trend_single": ("0", "0.15"),
}


def stock(index, momentum, volatility, short="0", industry="电子", status="trading"):
    return {"symbol": f"00000{index}.SZ", "trade_status": status, "momentum_63": momentum,
            "volatility_63": volatility, "return_20": short, "sw_level1_name": industry}


def positions(name, stocks, pool="tech"):
    samples = [{"id": "test", "pool": pool, "stocks": stocks}]
    return {row["symbol"]: row for row in generate_weights(STRATEGIES / name / "strategy.py", samples)}


class TrendSleevesTest(unittest.TestCase):
    def test_core_respects_industries_while_satellite_uses_total_momentum(self):
        stocks = [stock(1, "0.2", "0.01", "0.1"), stock(2, "0.4", "0.02", "0.4"),
                  stock(3, "0.3", "0.03", "-0.1", "通信"),
                  stock(4, "0.6", "0.04", "0.6", "计算机")]
        for name, (core, _) in CONFIGURATIONS.items():
            with self.subTest(name=name):
                result = positions(name, stocks)
                budget = Decimal(core)
                self.assertEqual(Decimal(result["000002.SZ"]["weight"]), 1 - budget)
                for code in ("000001.SZ", "000003.SZ", "000004.SZ"):
                    if budget:
                        self.assertLessEqual(Decimal(result[code]["weight"]), budget / 3)
                        self.assertLess(budget / 3 - Decimal(result[code]["weight"]), Decimal("0.00000001"))
                    else:
                        self.assertNotIn(code, result)
                self.assertLessEqual(sum(Decimal(p["weight"]) for p in result.values()), 1)

    def test_overlapping_sleeves_merge_before_rounding_and_keep_one_execution_rule(self):
        stocks = [stock(1, "0.4", "0.01"), stock(2, "0.2", "0.02", industry="通信"),
                  stock(3, "0.3", "0.03", industry="计算机")]
        for name, (core, _) in CONFIGURATIONS.items():
            with self.subTest(name=name):
                result = positions(name, stocks)
                expected = 1 - Decimal(core) + Decimal(core) / 3
                actual = Decimal(result["000001.SZ"]["weight"])
                self.assertLessEqual(actual, expected)
                self.assertLess(expected - actual, Decimal("0.00000001"))
                self.assertEqual(result["000001.SZ"]["grid"], {"step": "0.08", "lots": 2, "initial": 1})
                self.assertNotIn("exit", result["000001.SZ"])
                dividend = positions(name, stocks, "dividend")
                self.assertTrue(all(p["exit"] == {"take_profit": "0.1"} for p in dividend.values()))
                self.assertTrue(all("grid" not in p for p in dividend.values()))

    def test_strict_threshold_fallback_missing_data_and_empty_pool(self):
        for name, (core, threshold) in CONFIGURATIONS.items():
            with self.subTest(name=name):
                # A threshold equality must not beat a genuinely qualifying stock.
                stocks = [stock(1, threshold, "0.001"), stock(2, "0.5", "0.02")]
                result = positions(name, stocks)
                self.assertGreaterEqual(Decimal(result["000002.SZ"]["weight"]), 1 - Decimal(core))
                weak = [stock(1, "-0.1", "0.02"), stock(2, None, "0.01"),
                        stock(3, "-0.2", None), stock(4, "1", "0.001", status="no_bar")]
                result = positions(name, weak)
                self.assertEqual({c: p["weight"] for c, p in result.items()},
                                 {"000001.SZ": "0.50000000", "000002.SZ": "0.50000000"})
                self.assertEqual(positions(name, [stock(1, None, None)]), {})
                self.assertEqual(positions(name, []), {})

    def test_ties_are_independent_of_input_order_and_unknown_volatility_has_no_grid(self):
        stocks = [stock(1, "0.4", None), stock(2, "0.4", None)]
        for name in CONFIGURATIONS:
            with self.subTest(name=name):
                first = positions(name, stocks)
                self.assertEqual(first, positions(name, list(reversed(stocks))))
                self.assertIn("000001.SZ", first)
                self.assertTrue(all("grid" not in p and "exit" not in p for p in first.values()))


if __name__ == "__main__":
    unittest.main()
