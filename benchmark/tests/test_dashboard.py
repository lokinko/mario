import json
import random
import re
import shutil
import subprocess
import tempfile
import threading
import unittest
from pathlib import Path
from urllib.error import HTTPError
from urllib.request import Request, urlopen

from benchmark.dashboard import DASHBOARD_FILES, SCRIPT, STATS, dashboard_data, dashboard_server, render, write_dashboard
from benchmark.files import Invalid, write_json, write_rows
from benchmark.scoring import series_stats, total_score


def report(scores, benchmark=1.0, ids=("dividend-2015-03-31", "tech-2015-03-31")):
    def stat(values):
        return {"mean": sum(values) / len(values), "median": values[0], "stdev": 0, "count": len(values),
                "bootstrap_95": [min(values), max(values)]}
    def series(values):
        return {"score": stat([v + 2.5 if v > 0 else v for v in values]), "return_score": stat(values),
                "win_rate": sum(v > 0 for v in values) / len(values)}

    def stats(values):
        return {f"{prefix}{key}": value for prefix, part in (("", values), ("pool_equal_weight_", [benchmark] * len(values)),
                                                             ("csi300_", [0.5] * len(values)))
                for key, value in series(part).items()}

    def total(v):
        return v + 2.5 if v > 0 else v

    periods = [{"sample_id": i, "pool": i.split("-")[0], "decision_date": i.split("-", 1)[1], "entry_date": "2015-04-01",
                "blocked_entries": ["600000.SH"] if n == 0 else [], "exits": [], "segment_returns": [0.01, 0.0, -0.02],
                "grid_trades": [{"symbol": "600000.SH", "buys": 2, "sells": 1}] if n == 1 else [], "operations": [], "return_score": s,
                "score": total(s), "pool_equal_weight_return_score": benchmark, "pool_equal_weight_score": total(benchmark),
                "csi300_return_score": 0.5, "csi300_score": total(0.5)}
               for n, (i, s) in enumerate(zip(ids, scores))]
    return {"metric": "总分", "cost_basis": "买入成本", "limitations": ["限制</script>"], **stats(scores),
            "by_pool": {p["pool"]: stats([p["return_score"]]) for p in periods}, "periods": periods}


def run(scorecard, title="策略", weights=()):
    return {"title": title, "description": "说明", "scorecard": scorecard, "weights": list(weights)}


def save(root, name, scorecard, status="scored"):
    (root / name).mkdir(parents=True)
    write_json(root / name / "run.json", {"status": status, "title": f"{name} 标题", "description": "说明",
                                          "error": "静态检查未通过"})
    if status == "scored":
        write_json(root / name / "scorecard.json", scorecard)
        write_rows(root / name / "weights.jsonl", [])


class DashboardTest(unittest.TestCase):
    def test_merges_strategies_by_period_with_titles_from_descriptions(self):
        data = dashboard_data({"zeta": run(report([3.0, 4.0]), "末位"), "momentum": run(report([1.0, -2.0])),
                               "equal": run(report([0.0, 1.0]))},
                              {"version": "dividend+tech-2015-2015", "built_on": "2026-10-31", "secret": "x"})
        self.assertEqual([s["key"] for s in data["strategies"]], ["equal", "momentum", "zeta"])
        self.assertEqual(data["strategies"][2]["label"], "末位")
        self.assertEqual(data["periods"][0]["scores"], {"equal": 0.0, "momentum": 1.0, "zeta": 3.0})
        self.assertEqual(data["periods"][0]["blocked"]["momentum"], ["600000.SH"])
        self.assertEqual([p["grid_trades"]["momentum"] for p in data["periods"]], [0, 3])
        self.assertEqual(data["periods"][1]["benchmarks"], {"pool_equal_weight": 1.0, "csi300": 0.5})
        self.assertEqual(data["stats"], {"win_rate_weight": 2.5, "rounds": 2000, "seed": 20261031})
        self.assertEqual([s["key"] for s in data["segments"]], ["all", "early"])
        self.assertNotIn("secret", data["release"])
        self.assertEqual({s["key"]: s["baseline"] for s in data["strategies"]},
                         {"equal": True, "momentum": True, "zeta": False})
        self.assertTrue(all(b["description"] for b in data["benchmarks"]))

    def test_splits_years_into_segments_and_carries_holdings_with_exit_rules(self):
        reordered = dashboard_data({"trend": run(report([1.0, 2.0], ids=("tech-2019-03-31", "dividend-2015-03-31")))})
        self.assertEqual([(p["id"], p["order"]) for p in reordered["periods"]],
                         [("dividend-2015-03-31", 1), ("tech-2019-03-31", 0)])
        ids = ("dividend-2015-03-31", "tech-2019-03-31")
        weights = [{"sample_id": ids[1], "symbol": "000001.SZ", "weight": "0.5", "exit": {"take_profit": "0.08"}},
                   {"sample_id": ids[1], "symbol": "000002.SZ", "weight": "0.5"}]
        data = dashboard_data({"trend": run(report([-1.0, 2.0], ids=ids), weights=weights)})
        self.assertEqual(data["segments"], [{"key": "all", "label": "全部年份"}, {"key": "early", "label": "2015"},
                                            {"key": "late", "label": "2019"}])
        self.assertEqual([p["segment"] for p in data["periods"]], ["early", "late"])
        self.assertEqual(data["periods"][0]["holdings"]["trend"], [])
        self.assertEqual(data["periods"][1]["holdings"]["trend"],
                         [["000001.SZ", "0.5", {"take_profit": "0.08"}], ["000002.SZ", "0.5", None]])

    def test_rejects_scorecards_from_different_releases(self):
        with self.assertRaises(Invalid):
            dashboard_data({"equal": run(report([0.0, 1.0])), "size": run(report([0.0], ids=("tech-2015-03-31",)))})
        with self.assertRaises(Invalid):
            dashboard_data({"equal": run(report([0.0, 1.0])), "size": run(report([0.0, 1.0], benchmark=2.0))})

    def test_per_period_operations_are_carried_and_old_reports_require_rescoring(self):
        card = report([0.0, 1.0])
        trade = {"symbol": "600000.SH", "side": "sell", "position_fraction": "0.5", "cash_fraction": "0.1"}
        card["periods"][0]["operations"] = [trade]
        data = dashboard_data({"trend": run(card)})
        self.assertEqual(data["periods"][0]["operations"]["trend"], [trade])
        card["periods"][0].pop("operations")
        with self.assertRaisesRegex(Invalid, "重新运行 bench-run"):
            dashboard_data({"trend": run(card)})

    @unittest.skipUnless(shutil.which("node"), "需要 Node.js 验证操作比重展示")
    def test_operation_display_distinguishes_budget_initial_buy_and_current_share_ratios(self):
        helpers = SCRIPT[SCRIPT.index("function initialWeight("):SCRIPT.index("function detail()")]
        definitions = "\n".join(line for line in SCRIPT.splitlines() if line.startswith(("const pct =", "const esc =", "const RULES =")))
        harness = definitions + "\n" + helpers + """
const rule = {step: '0.08', lots: 2, initial: 1};
const trade = {symbol: '<script>', side: 'sell', reason: 'take_profit', execution_date: '2026-01-02',
  signal_date: '2026-01-01', position_fraction: '0.5', cash_fraction: '0.12', budget_fraction: '0.2', remaining_value_fraction: '0.12'};
console.log(JSON.stringify({initial: initialWeight('0.2', rule), grid: ruleText(rule, '0.2'),
  exit: ruleText({take_profit:'0.1', sell_fraction:'0.5'}, '0.2'), html:operationsTable([trade])}));
"""
        done = subprocess.run(["node", "-e", harness], capture_output=True, text=True, check=True)
        rendered = json.loads(done.stdout)
        self.assertEqual(rendered["initial"], 0.1)
        self.assertIn("首买预算的 50%", rendered["grid"])
        self.assertIn("期初资产的 10%", rendered["grid"])
        self.assertIn("卖出该股持股数量的 50%", rendered["exit"])
        self.assertIn("成交资金/期初资产", rendered["html"])
        self.assertIn("卖出当时持股的 50%", rendered["html"])
        self.assertIn("&lt;script&gt;", rendered["html"])
        self.assertNotIn("<script>", rendered["html"])

    def test_page_and_script_carry_no_data_and_data_file_lists_failures(self):
        files = render(dashboard_data({"equal": run(report([0.0, 1.0]))}))
        self.assertEqual(set(files), set(DASHBOARD_FILES))
        self.assertEqual(json.loads(files["dashboard.json"])["limitations"], ["限制</script>"])
        self.assertNotIn("限制</script>", files["dashboard.html"] + files["dashboard.js"])
        self.assertIsNone(re.search(r"<script(?![^>]*\bsrc=)", files["dashboard.html"]), "页面不能有内联脚本，否则被 CSP 拦截")
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder) / "runs"
            save(base, "equal", report([0.0, 1.0]))
            save(base, "broken", None, status="failed")
            write_dashboard(base, base)
            save(base, "size", report([2.0, 1.0]))
            result = write_dashboard(base, base)
            self.assertEqual(result["strategies"], ["equal", "size"])
            data = json.loads((base / "dashboard.json").read_text(encoding="utf-8"))
            self.assertIn("size 标题", [s["label"] for s in data["strategies"]])
            self.assertIn("静态检查未通过", data["failures"][0]["error"])
            self.assertFalse(list(base.glob("*.part")))

    @unittest.skipUnless(shutil.which("node"), "需要 Node.js 运行看板统计脚本")
    def test_page_statistics_match_scoring_for_any_period_subset(self):
        generator = random.Random(7)
        samples = [[round(generator.uniform(-15, 20), 6) for _ in range(n)] for n in (1, 2, 3, 37, 148)]
        samples += [[0.0, 0.0, 1.5], [-0.1] * 5]
        harness = STATS + "\nconst input = JSON.parse(require('fs').readFileSync(0, 'utf8'));\n" \
            "console.log(JSON.stringify(input.samples.map(v => summarize(v, input.params, true))));"
        params = dashboard_data({"equal": run(report([0.0, 1.0]))})["stats"]
        done = subprocess.run(["node", "-e", harness], input=json.dumps({"samples": samples, "params": params}),
                              capture_output=True, text=True, check=True)
        for values, page in zip(samples, json.loads(done.stdout), strict=True):
            expected = series_stats([{"return_score": v, "score": total_score(v)} for v in values], [""])
            score = expected["score"]
            self.assertEqual(page["count"], score["count"])
            self.assertEqual(page["win_rate"], expected["win_rate"])
            self.assertEqual(page["median"], score["median"])
            self.assertAlmostEqual(page["mean"], score["mean"], places=12)
            self.assertAlmostEqual(page["return_mean"], expected["return_score"]["mean"], places=12)
            self.assertEqual(page["ci"], score["bootstrap_95"])

    def test_server_uses_deployed_paths_marks_data_live_and_hides_other_files(self):
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder)
            with self.assertRaises(Invalid):
                dashboard_server(base, 0)
            save(base, "equal", report([0.0, 1.0]))
            write_dashboard(base, base)
            with dashboard_server(base, 0) as server:
                worker = threading.Thread(target=server.serve_forever, daemon=True)
                worker.start()
                root = f"http://127.0.0.1:{server.server_port}"
                try:
                    with urlopen(f"{root}/dashboard", timeout=5) as response:
                        self.assertIn('src="dashboard.js"', response.read().decode())
                    with urlopen(f"{root}/dashboard.js", timeout=5) as response:
                        self.assertEqual(response.headers["Content-Type"], "text/javascript")
                    with urlopen(Request(f"{root}/api/benchmark/dashboard", method="HEAD"), timeout=5) as response:
                        self.assertEqual(response.headers["X-Bench-Live"], "1")
                        self.assertTrue(response.headers["Last-Modified"])
                    with self.assertRaises(HTTPError) as missing:
                        urlopen(f"{root}/equal/weights.jsonl", timeout=5)
                    self.assertEqual(missing.exception.code, 404)
                finally:
                    server.shutdown()


if __name__ == "__main__":
    unittest.main()
