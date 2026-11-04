"""证明持续评测仅从原始事实生成字段，不借用评分包的预计算输入。"""

import tempfile
import unittest
from decimal import Decimal
from pathlib import Path

from benchmark.evaluation_data import rebuild_evaluation
from benchmark.files import Invalid, digest, read_json, rows, write_json, write_rows
from benchmark.release import verify_release
from benchmark.tests.test_fugui_study import fixture


def seal(root):
    write_json(root / "manifest.json", {"files": {p.relative_to(root).as_posix(): digest(p)
               for p in sorted(root.rglob("*")) if p.is_file() and p.name != "manifest.json"}})


def raw_plan_fixture(base):
    plan, raw = base / "plan", base / "raw"
    (plan / "universe").mkdir(parents=True)
    write_json(plan / "config.json", {"version": "poison-v8", "input_features": "unusable", "pools": {"dividend": [2020]}, "built_on": "2026-11-04"})
    write_json(plan / "universe/dividend-2020.json", {"pool": "dividend", "year": 2020,
        "selection_date": "2019-12-31", "available_at": "2019-12-31T18:00:00+08:00",
        "selected": ["600000.SH"], "candidates": [{"symbol": "600000.SH", "close": "100", "total_shares": "2000000000",
            "shares_notice_date": "2019-01-01", "market_cap": "poison", "level1_name": "行业"}]})
    (plan / "outcomes.jsonl").write_bytes(b'not parsed by the feature builder: secret future returns\n')
    seal(plan)
    days, bars = fixture([100] * 39)
    raw.mkdir()
    write_json(raw / "config.json", {"price_source": "https://www.baostock.com/"})
    write_json(raw / "source.json", {"commit": "fixed-original-source"})
    write_json(raw / "instruments.json", {"600000.SH": {"ipoDate": "1990-01-01", "outDate": "", "code_name": "未来ST退市名称"}})
    write_rows(raw / "calendar.jsonl", [{"calendar_date": d, "is_trading_day": "1"} for d in days])
    for flag in ("1", "3"):
        path = raw / f"prices/600000.SH/adjust{flag}"
        path.mkdir(parents=True)
        write_rows(path / "history.jsonl", list(bars.values()))
    seal(raw)
    return plan, raw


class RawEvaluationTest(unittest.TestCase):
    def test_rebuild_uses_raw_facts_and_only_declared_metrics(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plan, raw = raw_plan_fixture(root)
            source = 'REQUIRES = {"stocks.*.market_cap_at_selection":"decimal", "stocks.*.momentum_63":"decimal?", "stocks.*.fugui.monthly_macd_negative":"boolean?"}\ndef decide(sample):\n    return {}'
            report = rebuild_evaluation(plan, raw, root / "derived", [source])
            result = rows(root / "derived/inputs.jsonl")
            self.assertEqual(len(result), 4)
            for sample in result:
                stock = sample["stocks"][0]
                self.assertEqual(stock["market_cap_at_selection"], "200000000000")
                self.assertEqual(stock["momentum_63"], "0")
                self.assertIn("monthly_macd_negative", stock["fugui"])
                self.assertNotIn("long_history", stock)
                self.assertNotIn("volatility_63", stock)
                self.assertNotIn("code_name", stock)
            self.assertEqual(digest(root / "derived/outcomes.jsonl"), digest(plan / "outcomes.jsonl"))
            self.assertEqual(report["input_origin"], "raw_facts")
            self.assertNotIn("input_features", read_json(root / "derived/config.json"))
            self.assertTrue(verify_release(root / "derived")["valid"])

    def test_precomputed_inputs_are_rejected_and_missing_raw_cannot_fall_back(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plan, raw = raw_plan_fixture(root)
            write_rows(plan / "inputs.jsonl", [{"fugui": {"monthly_macd_negative": True}}])
            seal(plan)
            with self.assertRaisesRegex(Invalid, "预计算"):
                rebuild_evaluation(plan, raw, root / "forbidden", [])
            (plan / "inputs.jsonl").unlink()
            seal(plan)
            (raw / "prices/600000.SH/adjust1/history.jsonl").unlink()
            seal(raw)
            with self.assertRaisesRegex(Invalid, "原始日线缺失"):
                rebuild_evaluation(plan, raw, root / "missing", [])
            self.assertFalse((root / "missing").exists())

    def test_new_indicator_works_without_changing_a_feature_bundle(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plan, raw = raw_plan_fixture(root)
            source = 'REQUIRES = {"stocks.*.long_history.clenow_mark_90":"decimal?"}\ndef decide(sample):\n    return {}'
            rebuild_evaluation(plan, raw, root / "extended", [source])
            stock = rows(root / "extended/inputs.jsonl")[-1]["stocks"][0]
            self.assertEqual(Decimal(stock["long_history"]["clenow_mark_90"]), 0)
            self.assertNotIn("fugui", stock)
