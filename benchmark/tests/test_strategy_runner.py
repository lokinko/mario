import shutil
import tempfile
import unittest
from decimal import Decimal
from pathlib import Path

from benchmark.files import Invalid, digest, read_json, write_json, write_rows
from benchmark.strategy_runner import (bench_run, check_source, completed_runs, discover, failed_runs,
                                       generate_weights)

BUILTIN = Path(__file__).resolve().parents[1] / "strategies"


def stock(code, momentum=None, cap="1", status="trading", volatility=None, short=None):
    return {"symbol": code, "trade_status": status, "momentum_63": momentum, "market_cap_at_selection": cap,
            "volatility_63": volatility, "return_20": short}


def picks(name, stocks):
    return [row["symbol"] for row in generate_weights(BUILTIN / name / "strategy.py", [{"id": "s1", "stocks": stocks}])]


def path(*prices):
    return [[day, price] for day, price in zip(("2015-10-08", "2015-10-15", "2015-11-06", "2016-01-06"), prices)]


def release(root):
    root.mkdir()
    write_json(root / "config.json", {"version": "tech-2015-2015", "built_on": "2026-10-31", "pools": {"tech": [2015]}})
    write_rows(root / "inputs.jsonl", [{"id": "s1", "pool": "tech", "decision_date": "2015-09-30",
                                        "stocks": [stock("000001.SZ", "0.2"), stock("000002.SZ", "-0.1")]}])
    write_rows(root / "outcomes.jsonl", [{
        "sample_id": "s1", "entry_date": "2015-10-08", "boundaries": ["2015-10-15", "2015-11-06", "2016-01-06"],
        "stocks": {"000001.SZ": {"buyable": True, "entry_open": "10", "path": path("10", "11", "11", "12")},
                   "000002.SZ": {"buyable": True, "entry_open": "10", "path": path("10", "9", "9", "9")}},
        "index": {"entry_open": "100", "path": path("100", "100", "100", "100")}}])
    write_json(root / "manifest.json", {"schema_version": 1,
                                        "files": {p.name: digest(p) for p in sorted(root.iterdir())}})


def strategy(root, name, code, title="测试策略"):
    folder = root / name
    folder.mkdir(parents=True, exist_ok=True)
    (folder / "strategy.md").write_text(f"# {title}\n\n规则说明。\n", encoding="utf-8")
    (folder / "strategy.py").write_text(code, encoding="utf-8")


class BuiltinStrategyTest(unittest.TestCase):
    def test_every_builtin_strategy_passes_checks_and_has_description(self):
        for name, folder in discover(BUILTIN).items():
            self.assertEqual(check_source((folder / "strategy.py").read_text(encoding="utf-8")), [], name)
            self.assertTrue((folder / "strategy.md").read_text(encoding="utf-8").startswith("# "), name)

    def test_baselines_round_weights_down_and_skip_unlisted(self):
        stocks = [stock(f"00000{i}.SZ", str(i), str(10 - i)) for i in range(1, 4)] + [
            stock("000009.SZ", cap="99", status="no_bar")]
        equal = generate_weights(BUILTIN / "equal" / "strategy.py", [{"id": "s1", "stocks": stocks}])
        self.assertEqual({r["weight"] for r in equal}, {"0.33333333"})
        self.assertEqual(len(equal), 3)
        self.assertEqual(picks("momentum", stocks)[:1], ["000003.SZ"])
        self.assertEqual(picks("size", stocks)[0], "000001.SZ")
        self.assertEqual(picks("cash", stocks), [])

    def test_trend_rules_keep_only_rising_stocks_then_rank(self):
        stocks = [stock("000001.SZ", "0.2", volatility="0.03", short="0.05"),
                  stock("000002.SZ", "0.1", volatility="0.01", short="0.08"),
                  stock("000003.SZ", "0.3", volatility="0.02", short="-0.04"),
                  stock("000004.SZ", "-0.1", volatility="0.001", short="-0.2"),
                  stock("000005.SZ", "0", volatility="0.001", short="-0.3"),
                  stock("000006.SZ", None, volatility="0.001", short="-0.3"),
                  stock("000007.SZ", "0.5", short="0.01")]
        self.assertEqual(picks("trend", stocks), ["000001.SZ", "000002.SZ", "000003.SZ", "000007.SZ"])
        self.assertEqual(picks("trend_lowvol", stocks), ["000002.SZ", "000003.SZ", "000001.SZ"])
        self.assertEqual(picks("trend_pullback", stocks), ["000003.SZ", "000007.SZ", "000001.SZ", "000002.SZ"])
        falling = [stock("000001.SZ", "-0.2", volatility="0.01", short="0")]
        for name in ("trend", "trend_lowvol", "trend_pullback"):
            self.assertEqual(picks(name, falling), [])


class SandboxTest(unittest.TestCase):
    def test_static_check_blocks_io_imports_and_reflection(self):
        self.assertEqual(check_source("import math\nfrom decimal import Decimal\ndef decide(sample):\n    return {}\n"), [])
        for source in ("import os\ndef decide(s): return {}", "from pathlib import Path\ndef decide(s): return {}",
                       "def decide(s): return open('outcomes.jsonl')", "def decide(s): return ().__class__",
                       "def decide(s): return __builtins__", "def decide(s): return eval('1')",
                       "from . import x\ndef decide(s): return {}", "def other(s): return {}", "def decide(s) return"):
            self.assertTrue(check_source(source), source)

    def test_runtime_import_guard_and_bad_returns_fail(self):
        with tempfile.TemporaryDirectory() as folder:
            code = Path(folder) / "strategy.py"
            samples = [{"id": "s1", "stocks": []}]
            for source in ("def decide(s):\n    import os\n    return {}\n", "def decide(s):\n    return [1]\n",
                           "def decide(s):\n    return {'000001.SZ': True}\n", "def decide(s):\n    raise ValueError('x')\n"):
                code.write_text(source, encoding="utf-8")
                with self.assertRaises(Invalid):
                    generate_weights(code, samples)
            code.write_text("def decide(s):\n    return {'000001.SZ': 1 / 3, '000002.SZ': 0}\n", encoding="utf-8")
            self.assertEqual(generate_weights(code, samples), [{"sample_id": "s1", "symbol": "000001.SZ",
                                                                 "weight": "0.33333333"}])


class BenchRunTest(unittest.TestCase):
    def test_scores_caches_reruns_changed_code_and_cleans_removed(self):
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder)
            release(base / "release")
            strategy(base / "s", "rising", "def decide(s):\n    return {'000001.SZ': 1}\n", "只买上涨")
            strategy(base / "s", "broken", "import os\ndef decide(s):\n    return {}\n")
            first = bench_run(base / "s", base / "release", base / "out")
            self.assertEqual({r["strategy"]: r["status"] for r in first}, {"broken": "failed", "rising": "scored"})
            runs = completed_runs(base / "out")
            self.assertEqual(list(runs), ["rising"])
            self.assertEqual(runs["rising"]["title"], "只买上涨")
            first_segment, last_segment = Decimal("0.9987") * Decimal("1.1") - 1, Decimal(12) / 11 - 1
            self.assertAlmostEqual(runs["rising"]["scorecard"]["score"]["mean"],
                                   float(100 * (Decimal("0.3") * first_segment + Decimal("0.4") * last_segment)), places=6)
            self.assertIn("os", failed_runs(base / "out")["broken"])

            (base / "s" / "rising" / "strategy.md").write_text("# 新标题\n\n说明。\n", encoding="utf-8")
            again = bench_run(base / "s", base / "release", base / "out")
            self.assertEqual({r["strategy"]: r["status"] for r in again}["rising"], "cached")
            self.assertEqual(completed_runs(base / "out")["rising"]["title"], "新标题")

            strategy(base / "s", "rising", "def decide(s):\n    return {'000002.SZ': 1}\n")
            shutil.rmtree(base / "s" / "broken")
            changed = bench_run(base / "s", base / "release", base / "out")
            self.assertEqual([(r["strategy"], r["status"]) for r in changed], [("rising", "scored")])
            self.assertFalse((base / "out" / "broken").exists())
            self.assertLess(read_json(base / "out" / "rising" / "scorecard.json")["score"]["mean"], 0)

    def test_rejects_bad_strategy_names(self):
        with tempfile.TemporaryDirectory() as folder:
            strategy(Path(folder), "Bad-Name", "def decide(s):\n    return {}\n")
            with self.assertRaises(Invalid):
                discover(Path(folder))


if __name__ == "__main__":
    unittest.main()
