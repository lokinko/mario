"""买卖比重的分母、部分卖出后的剩余资产及逐笔成交记录。"""

import tempfile
import unittest
from decimal import Decimal
from pathlib import Path

from benchmark.files import Invalid
from benchmark.scoring import evaluate_weights, exit_rule, grid_rule, portfolio_segments, sell_cost
from benchmark.strategy_runner import generate_weights


def day(day, close, open_price=None, sellable=True, buyable=True):
    return [day, close, open_price or close, sellable, buyable]


def outcome(path, buyable=True):
    return {"sample_id": "s", "entry_date": "2015-10-08",
            "boundaries": ["2015-10-15", "2015-11-06", "2016-01-06"],
            "stocks": {"600000.SH": {"buyable": buyable, "entry_open": "10", "path": path}},
            "index": {"entry_open": "10", "path": path}}


class OperationRatiosTest(unittest.TestCase):
    def test_partial_exit_sells_once_and_keeps_marking_unsold_shares(self):
        path = [day("2015-10-08", "11"), day("2015-10-09", "12"), day("2015-10-15", "12"),
                day("2015-11-06", "20"), day("2016-01-06", "10")]
        operations = []
        returns, _, sold, _ = portfolio_segments({"600000.SH": Decimal(1)}, outcome(path),
            exits={"600000.SH": exit_rule({"take_profit": "0.1", "sell_fraction": "0.5"})},
            cost=Decimal(0), operations=operations)
        proceeds = Decimal("0.6") * (1 - sell_cost("2015-10-09"))
        values = [Decimal(1), proceeds + Decimal("0.6"), proceeds + 1, proceeds + Decimal("0.5")]
        self.assertEqual(returns, [values[i + 1] / values[i] - 1 for i in range(3)])
        self.assertEqual(len(sold), 1)
        self.assertEqual(sold[0]["sell_fraction"], "0.5")
        self.assertEqual([t["side"] for t in operations], ["buy", "sell"])
        self.assertEqual(operations[1]["position_fraction"], "0.5")
        self.assertEqual(Decimal(operations[1]["cash_fraction"]), Decimal("0.6"))
        self.assertEqual(Decimal(operations[1]["remaining_value_fraction"]), Decimal("0.6"))

    def test_deferred_partial_exit_records_actual_execution_day_and_no_phantom_sale(self):
        path = [day("2015-10-08", "12"), day("2015-10-09", "13", sellable=False),
                day("2015-10-15", "14", "13.5"), day("2016-01-06", "14")]
        operations = []
        rule = {"600000.SH": exit_rule({"take_profit": "0.1", "sell_fraction": "0.25"})}
        portfolio_segments({"600000.SH": Decimal("0.4")}, outcome(path), exits=rule, operations=operations)
        self.assertEqual(operations[1]["execution_date"], "2015-10-15")
        self.assertEqual(operations[1]["signal_date"], "2015-10-08")
        self.assertEqual(Decimal(operations[1]["position_fraction"]), Decimal("0.25"))
        stuck = [row[:3] + [False, row[4]] for row in path]
        operations = []
        _, _, sold, _ = portfolio_segments({"600000.SH": Decimal("0.4")}, outcome(stuck), exits=rule,
                                          operations=operations)
        self.assertIsNone(sold[0]["sell_date"])
        self.assertEqual(len(operations), 1)

    def test_grid_buy_uses_fixed_budget_fraction_but_sell_uses_current_shares(self):
        path = [day("2015-10-08", "10"), day("2015-10-09", "9"), day("2015-10-15", "9"),
                day("2015-11-06", "10.1"), day("2015-11-09", "10.1"), day("2016-01-06", "10.1")]
        operations = []
        portfolio_segments({"600000.SH": Decimal("0.4")}, outcome(path),
            grids={"600000.SH": grid_rule({"step": "0.1", "lots": 4, "initial": 2})},
            cost=Decimal(0), operations=operations)
        self.assertEqual([t["side"] for t in operations], ["buy", "buy", "sell"])
        self.assertEqual([Decimal(t["cash_fraction"]) for t in operations[:2]], [Decimal("0.2"), Decimal("0.1")])
        self.assertTrue(all(Decimal(t["budget_fraction"]) == Decimal("0.4") for t in operations))
        self.assertAlmostEqual(float(operations[-1]["position_fraction"]), 1 / 3)
        self.assertEqual(operations[-1]["signal_date"], "2015-11-06")
        self.assertEqual(operations[-1]["execution_date"], "2015-11-09")

    def test_report_has_per_period_operations_and_blocked_orders_are_not_trades(self):
        path = [day("2015-10-08", "10"), day("2015-10-15", "10"), day("2016-01-06", "10")]
        sample = {"id": "s", "pool": "tech", "decision_date": "2015-09-30", "stocks": [{"symbol": "600000.SH"}]}
        weights = [{"sample_id": "s", "symbol": "600000.SH", "weight": "0.2"}]
        report = evaluate_weights([sample], [outcome(path)], weights)
        trade = report["periods"][0]["operations"][0]
        self.assertEqual(trade["symbol"], "600000.SH")
        self.assertEqual(trade["cash_fraction"], "0.2")
        self.assertIsNone(trade["position_fraction"])
        self.assertEqual(Decimal(trade["remaining_value_fraction"]), Decimal("0.19974"))
        blocked = evaluate_weights([sample], [outcome(path, buyable=False)], weights)["periods"][0]
        self.assertEqual(blocked["operations"], [])
        self.assertEqual(blocked["blocked_entries"], ["600000.SH"])
        self.assertEqual(evaluate_weights([sample], [outcome(path)], [])["periods"][0]["operations"], [])

    def test_sell_fraction_requires_a_trigger_and_a_valid_proportion(self):
        for value in ("0", "-0.1", "1.01", "NaN", "Infinity", True, 0.5):
            with self.subTest(value=value), self.assertRaises(Invalid):
                exit_rule({"take_profit": "0.1", "sell_fraction": value})
        with self.assertRaises(Invalid):
            exit_rule({"sell_fraction": "0.5"})
        self.assertEqual(exit_rule({"take_profit": "0.1"})["sell_fraction"], Decimal(1))

    def test_strategy_output_exposes_default_ratio_and_preserves_explicit_partial_ratio(self):
        with tempfile.TemporaryDirectory() as folder:
            code = Path(folder) / "strategy.py"
            for fraction in (None, "0.25"):
                rule = {"take_profit": "0.1"}
                if fraction:
                    rule["sell_fraction"] = fraction
                code.write_text(f'def decide(sample):\n    return {{"600000.SH": {{"weight": "0.2", "exit": {rule!r}}}}}\n')
                result = generate_weights(code, [{"id": "s"}])
                self.assertEqual(result[0]["exit"]["sell_fraction"], fraction or "1")
