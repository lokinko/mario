import json
import re
import tempfile
import unittest
from pathlib import Path

from benchmark.dashboard import dashboard_data, render, write_dashboard
from benchmark.files import Invalid, write_json


def report(scores, benchmark=1.0, ids=("dividend-2015-03-31", "tech-2015-03-31")):
    def stat(values):
        return {"mean": sum(values) / len(values), "median": values[0], "stdev": 0, "count": len(values),
                "bootstrap_95": [min(values), max(values)]}
    periods = [{"sample_id": i, "pool": i.split("-")[0], "decision_date": i.split("-", 1)[1], "entry_date": "2015-04-01",
                "blocked_entries": ["600000.SH"] if n == 0 else [], "score": s,
                "pool_equal_weight_score": benchmark, "csi300_score": 0.5} for n, (i, s) in enumerate(zip(ids, scores))]
    return {"metric": "0.3×R1 + 0.3×R2 + 0.4×R3", "cost_basis": "买入成本", "limitations": ["限制</script>"],
            "score": stat(scores), "pool_equal_weight_score": stat([benchmark] * 2), "csi300_score": stat([0.5] * 2),
            "by_pool": {p["pool"]: {"score": stat([p["score"]]), "pool_equal_weight_score": stat([benchmark]),
                                    "csi300_score": stat([0.5])} for p in periods},
            "periods": periods}


def run(scorecard, title="策略"):
    return {"title": title, "description": "说明", "scorecard": scorecard}


def save(root, name, scorecard, status="scored"):
    (root / name).mkdir(parents=True)
    write_json(root / name / "run.json", {"status": status, "title": f"{name} 标题", "description": "说明",
                                          "error": "静态检查未通过"})
    if status == "scored":
        write_json(root / name / "scorecard.json", scorecard)


class DashboardTest(unittest.TestCase):
    def test_merges_strategies_by_period_with_titles_from_descriptions(self):
        data = dashboard_data({"zeta": run(report([3.0, 4.0]), "末位"), "momentum": run(report([1.0, -2.0])),
                               "equal": run(report([0.0, 1.0]))},
                              {"version": "dividend+tech-2015-2015", "built_on": "2026-10-31", "secret": "x"})
        self.assertEqual([s["key"] for s in data["strategies"]], ["equal", "momentum", "zeta"])
        self.assertEqual(data["strategies"][2]["label"], "末位")
        self.assertEqual(data["periods"][0]["scores"], {"equal": 0.0, "momentum": 1.0, "zeta": 3.0})
        self.assertEqual(data["periods"][0]["blocked"]["momentum"], 1)
        self.assertEqual(data["summaries"]["csi300"]["tech"]["mean"], 0.5)
        self.assertNotIn("secret", data["release"])

    def test_rejects_scorecards_from_different_releases(self):
        with self.assertRaises(Invalid):
            dashboard_data({"equal": run(report([0.0, 1.0])), "size": run(report([0.0], ids=("tech-2015-03-31",)))})
        with self.assertRaises(Invalid):
            dashboard_data({"equal": run(report([0.0, 1.0])), "size": run(report([0.0, 1.0], benchmark=2.0))})

    def test_html_embeds_escaped_data_lists_failures_and_refreshes_in_place(self):
        html = render(dashboard_data({"equal": run(report([0.0, 1.0]))}))
        embedded = re.search(r'<script type="application/json" id="data">(.*?)</script>', html, re.S).group(1)
        self.assertEqual(json.loads(embedded)["limitations"], ["限制</script>"])
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder) / "runs"
            save(base, "equal", report([0.0, 1.0]))
            save(base, "broken", None, status="failed")
            out = base / "dashboard.html"
            write_dashboard(base, out)
            save(base, "size", report([2.0, 1.0]))
            result = write_dashboard(base, out)
            self.assertEqual(result["strategies"], ["equal", "size"])
            text = out.read_text(encoding="utf-8")
            self.assertIn("size 标题", text)
            self.assertIn("静态检查未通过", text)


if __name__ == "__main__":
    unittest.main()
