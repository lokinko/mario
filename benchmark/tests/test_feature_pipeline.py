"""原始来源、指标扩展与正式策略消费的行为契约；测试数据全部合成。"""

import ast
import copy
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from benchmark.feature_pipeline import (INDICATORS, Indicator, build_features, catalog,
                                         enrich, merge_projection, value_at)
from benchmark.files import Invalid, digest, read_json, rows, write_json, write_rows
from benchmark.input_contract import requirements, validate_inputs
from benchmark.raw_data import RawData, import_raw, read_raw
from benchmark.strategy_runner import bench_run, generate_weights
from benchmark.tests.test_fugui_study import fixture
from benchmark.tests.test_strategy_runner import release, strategy


def source_fixture(root):
    days, bars = fixture([100] * 36 + [90, 80, 70])
    folder = root / "raw/baostock/history"
    folder.mkdir(parents=True)
    write_json(folder / "600000.SH-test-adjust1.json", list(bars.values()))
    write_json(folder / "trade_dates-test.json", [{"calendar_date": d, "is_trading_day": "1"} for d in days])
    return days, bars


class FeaturePipelineTest(unittest.TestCase):
    def test_additive_indicator_and_new_strategy_need_no_bundle_version(self):
        days, bars = fixture([100] * 39)
        raw = RawData({"600000.SH": bars}, days, {"source": "synthetic"})
        sample = {"id": "s", "decision_date": days[-1], "stocks": [{"symbol": "600000.SH", "future_extension": {"x": 1}}]}

        def custom(bars, calendar, decision):
            return {"custom": bars[decision]["close"]}

        definitions = (*INDICATORS, Indicator("extension.custom", "decimal?", "adjusted_price", custom))
        result, report = enrich([sample], raw, ["fugui.monthly_macd_negative", "extension.custom"], definitions)
        self.assertIs(result[0]["stocks"][0]["fugui"]["monthly_macd_negative"], False)
        self.assertEqual(result[0]["stocks"][0]["extension"]["custom"], "100")
        self.assertEqual(result[0]["stocks"][0]["future_extension"], {"x": 1})
        self.assertEqual(sample["stocks"][0], {"symbol": "600000.SH", "future_extension": {"x": 1}})
        self.assertEqual(set(report["calculated"]), {"fugui.monthly_macd_negative", "extension.custom"})
        validate_inputs(result, {"stocks.*.extension.custom": "decimal?"})
        self.assertEqual(merge_projection({"extension": True, "issues": {"extension": "keep", "known": "old"}},
                                         {"known": False, "issues": {}}),
                         {"extension": True, "known": False, "issues": {"extension": "keep"}})

    def test_missing_dependency_fails_before_subprocess_but_null_is_valid(self):
        with tempfile.TemporaryDirectory() as folder:
            code = Path(folder) / "strategy.py"
            code.write_text('REQUIRES = {"stocks.*.fugui.monthly_macd_negative": "boolean?"}\ndef decide(sample):\n    return {}\n', encoding="utf-8")
            samples = [{"id": "v8-row", "stocks": [{"symbol": "600000.SH", "fugui": {"monthly_low": False}}]}]
            with patch("benchmark.strategy_runner.subprocess.run") as process:
                with self.assertRaisesRegex(Invalid, "missing_indicator.*monthly_macd_negative"):
                    generate_weights(code, samples)
                process.assert_not_called()
            samples[0]["stocks"][0]["fugui"]["monthly_macd_negative"] = None
            self.assertEqual(generate_weights(code, samples), [])
            samples[0]["stocks"][0]["fugui"]["monthly_macd_negative"] = "False"
            with self.assertRaisesRegex(Invalid, "invalid_indicator_type"):
                generate_weights(code, samples)

    def test_raw_projection_excludes_future_and_late_records_and_keeps_gaps(self):
        days, bars = fixture([100] * 39)
        decision = days[-2]
        sample = {"id": "s", "decision_date": decision, "stocks": [{"symbol": "600000.SH"}]}
        expected, _ = enrich([sample], RawData({"600000.SH": bars}, days, {}), ["fugui.monthly_macd_negative"])
        bars[days[-1]]["close"] = "999999"
        result, _ = enrich([sample], RawData({"600000.SH": bars}, days, {}), ["fugui.monthly_macd_negative"])
        self.assertEqual(result, expected)
        bars[decision]["available_at"] = days[-1] + "T18:00:00+08:00"
        projected, sessions = RawData({"600000.SH": bars}, days, {}).at("600000.SH", decision)
        self.assertNotIn(decision, projected)
        self.assertEqual(sessions[-1], decision)
        result, _ = enrich([sample], RawData({"600000.SH": bars}, days, {}), ["fugui.monthly_macd_negative"])
        signal = result[0]["stocks"][0]["fugui"]
        self.assertIsNone(signal["monthly_macd_negative"])
        self.assertEqual(signal["issues"]["monthly_macd_negative"], "insufficient_contiguous_history")

    def test_unknown_indicator_duplicate_definition_and_wrong_basis_are_explicit(self):
        days, bars = fixture([100] * 36)
        sample = {"id": "s", "decision_date": days[-1], "stocks": [{"symbol": "600000.SH"}]}
        with self.assertRaisesRegex(Invalid, "unknown_indicator"):
            enrich([sample], RawData({"600000.SH": bars}, days, {}), ["future.unknown"])
        with self.assertRaisesRegex(Invalid, "incompatible_price_basis"):
            enrich([sample], RawData({"600000.SH": bars}, days, {}, "unreconciled_other_provider"), ["return_20"])
        with self.assertRaisesRegex(Invalid, "重复"):
            catalog((*INDICATORS, INDICATORS[0]))
        sample["stocks"][0]["return_20"] = "1"
        with self.assertRaisesRegex(Invalid, "raw_input_mismatch"):
            enrich([sample], RawData({"600000.SH": bars}, days, {}), ["fugui.monthly_macd_negative"])

    def test_import_once_reuse_for_any_snapshot_and_preserve_labels_and_extensions(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            days, _ = source_fixture(root / "cache")
            imported = root / "raw"
            import_raw(imported, roots=[root / "cache"], symbols={"600000.SH"})
            source = root / "arbitrary-name"
            source.mkdir()
            write_json(source / "config.json", {"built_on": "2026-11-04", "version": "anything", "input_features": "unknown-extra-bundle"})
            write_rows(source / "inputs.jsonl", [{"id": "s", "decision_date": days[-1], "stocks": [{"symbol": "600000.SH",
                       "long_history": {"extension": "preserve"}, "fugui": {"extra": True, "monthly_low": None}}]}])
            (source / "outcomes.jsonl").write_bytes(b'{"secret_future_label":"never used"}\n')
            write_json(source / "manifest.json", {"files": {p.name: digest(p) for p in source.iterdir()}})
            first, second = root / "first", root / "second"
            build_features(source, imported, first, ["fugui.monthly_macd_negative"])
            build_features(first, imported, second, ["long_history.daily_returns_1m"])
            stock = rows(second / "inputs.jsonl")[0]["stocks"][0]
            self.assertEqual(stock["long_history"]["extension"], "preserve")
            self.assertIsNone(stock["fugui"]["monthly_low"])
            self.assertIs(stock["fugui"]["extra"], True)
            self.assertTrue(value_at(stock, "fugui.monthly_macd_negative")[0])
            self.assertEqual(digest(second / "outcomes.jsonl"), digest(source / "outcomes.jsonl"))
            self.assertEqual(set(read_json(second / "features.json")["indicators"]),
                             {"fugui.monthly_macd_negative", "long_history.daily_returns_1m"})
            self.assertNotIn("input_features", read_json(second / "config.json"))
            with self.assertRaises(Invalid):
                build_features(source, imported, first, [])
            (imported / "bars/600000.SH.jsonl").write_text("{}\n", encoding="utf-8")
            with self.assertRaises(Invalid):
                read_raw(imported)

    def test_on_demand_evaluation_and_cache_include_raw_content(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            release(root / "release")
            # Existing sample has no month returns; use one shared raw source for both securities.
            samples = rows(root / "release/inputs.jsonl")
            for stock in samples[0]["stocks"]:
                stock.pop("momentum_63")
                stock.pop("return_20")
            write_rows(root / "release/inputs.jsonl", samples)
            write_json(root / "release/manifest.json", {"files": {p.name: digest(p) for p in (root / "release").iterdir() if p.name != "manifest.json"}})
            strategy(root / "strategies", "dependent", 'REQUIRES = {"stocks.*.long_history.daily_returns_1m": "returns"}\ndef decide(sample):\n    return {}\n')
            bars = {"2015-09-29": {"close": "100", "tradestatus": "1"}, "2015-09-30": {"close": "101", "tradestatus": "1"}}
            raw = RawData({"000001.SZ": copy.deepcopy(bars), "000002.SZ": copy.deepcopy(bars)}, sorted(bars), {})
            failed = bench_run(root / "strategies", root / "release", root / "runs")
            self.assertEqual(failed[0]["status"], "failed")
            for status in ("scored", "cached"):
                self.assertEqual(bench_run(root / "strategies", root / "release", root / "runs", raw=raw)[0]["status"], status)
            before = read_json(root / "runs/dependent/run.json")["input_sha256"]
            raw.history["000001.SZ"]["2015-09-30"]["close"] = "102"
            self.assertEqual(bench_run(root / "strategies", root / "release", root / "runs", raw=raw)[0]["status"], "scored")
            self.assertNotEqual(read_json(root / "runs/dependent/run.json")["input_sha256"], before)

    def test_all_formal_strategies_declare_inputs_and_calculators_have_no_io_dependencies(self):
        root = Path(__file__).parents[1]
        for path in (root / "strategies").glob("*/strategy.py"):
            source = path.read_text(encoding="utf-8")
            self.assertIn("REQUIRES =", source, path)
            requirements(source)
        forbidden = {"scripts", "subprocess", "socket", "urllib", "requests", "pathlib"}
        for name in ("price_indicators.py", "technical_indicators.py", "financial_indicators.py", "announcement_indicators.py"):
            tree = ast.parse((root / name).read_text(encoding="utf-8"))
            for node in ast.walk(tree):
                if isinstance(node, ast.Import):
                    self.assertFalse({n.name.split(".")[0] for n in node.names} & forbidden)
                if isinstance(node, ast.ImportFrom) and not node.level:
                    self.assertNotIn(node.module.split(".")[0], forbidden)
                if isinstance(node, ast.Call) and isinstance(node.func, ast.Name):
                    self.assertNotIn(node.func.id, {"open", "read_json", "rows"})
