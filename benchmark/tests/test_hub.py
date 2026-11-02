import sys
import tempfile
import types
import unittest
from fnmatch import fnmatch
from pathlib import Path
from unittest.mock import patch

from benchmark.files import Invalid, digest, read_json, write_json
from benchmark.hub import CANARY, dataset_card, fetch, publish

REPORT = {"score": {"mean": 1.5, "bootstrap_95": [0.5, 2.5], "count": 4}, "beat_csi300_periods": 3,
          "pool_equal_weight_score": {"mean": 1.0}, "csi300_score": {"mean": 0.5}}


def release(root):
    root.mkdir()
    write_json(root / "config.json", {"version": "dividend+tech-2015-2015", "built_on": "2026-10-31",
                                      "pools": {"dividend": [2015], "tech": [2015]}, "buy_cost": "0.0013",
                                      "cost_basis": "买入成本"})
    write_json(root / "summary.json", {"samples_by_pool": {"dividend": 2, "tech": 2}})
    (root / "inputs.jsonl").write_text("{}\n", encoding="utf-8")
    (root / "outcomes.jsonl").write_text("{}\n", encoding="utf-8")
    files = {p.name: digest(p) for p in sorted(root.iterdir())}
    write_json(root / "manifest.json", {"schema_version": 1, "files": files})


def runs(root):
    (root / "equal").mkdir(parents=True)
    write_json(root / "equal" / "run.json", {"status": "scored", "title": "全池等权", "description": "说明"})
    write_json(root / "equal" / "scorecard.json", REPORT)


class FakeApi:
    uploads, private = {}, True

    def create_repo(self, repo_id, **_):
        pass

    def repo_info(self, repo_id, **_):
        return types.SimpleNamespace(private=self.private)

    def upload_folder(self, repo_id, folder_path, **_):
        FakeApi.uploads[repo_id] = {p.relative_to(folder_path).as_posix(): p.read_text(encoding="utf-8")
                                    for p in Path(folder_path).rglob("*") if p.is_file()}
        return types.SimpleNamespace(oid=f"sha-{repo_id}")

    def create_tag(self, *_, **__):
        pass


def snapshot_download(repo_id, allow_patterns, local_dir, **_):
    for name, text in FakeApi.uploads[repo_id].items():
        if any(fnmatch(name, pattern) for pattern in allow_patterns):
            target = Path(local_dir) / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(text, encoding="utf-8")
    return local_dir


class HubTest(unittest.TestCase):
    def setUp(self):
        FakeApi.uploads, FakeApi.private = {}, True
        hub = types.SimpleNamespace(HfApi=FakeApi, snapshot_download=snapshot_download)
        self.module = patch.dict(sys.modules, {"huggingface_hub": hub})
        self.module.start()
        self.addCleanup(self.module.stop)

    def test_outcomes_only_go_to_labels_repo_and_card_carries_canary(self):
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder)
            release(base / "release")
            runs(base / "cards")
            result = publish(base / "release", "me/bench", "me/labels", "pool-scoring", base / "cards")
        inputs, labels = FakeApi.uploads["me/bench"], FakeApi.uploads["me/labels"]
        self.assertNotIn("pool-scoring/outcomes.jsonl", inputs)
        self.assertIn("pool-scoring/manifest.json", inputs)
        self.assertEqual(set(labels) - {"README.md"}, {"pool-scoring/outcomes.jsonl", "pool-scoring/scorecards/equal.json"})
        self.assertIn(CANARY, inputs["README.md"])
        self.assertIn("1.50 [0.50, 2.50]", inputs["README.md"])
        self.assertEqual(result["labels"]["commit"], "sha-me/labels")

    def test_monthly_risk_card_describes_cutoff_and_missing_values_only_when_present(self):
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder) / "release"
            release(base)
            old = dataset_card(base, "pool-scoring", "me/labels", {})
            self.assertNotIn("daily_returns_1m", old)
            config = read_json(base / "config.json")
            write_json(base / "config.json", {**config, "input_features": "monthly-risk-v1"})
            card = dataset_card(base, "pool-scoring", "me/labels", {})
            self.assertIn("daily_returns_1m", card)
            self.assertIn("未知缺行保留 null", card)
            self.assertIn("不含决策后的收益", card)

    def test_refuses_public_repo_and_tampered_release(self):
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder)
            release(base / "release")
            runs(base / "cards")
            FakeApi.private = False
            with self.assertRaises(Invalid):
                publish(base / "release", "me/bench", "me/labels", "pool-scoring", base / "cards")
            FakeApi.private = True
            (base / "release" / "inputs.jsonl").write_text("[]\n", encoding="utf-8")
            with self.assertRaises(Invalid):
                publish(base / "release", "me/bench", "me/labels", "pool-scoring", base / "cards")
        self.assertEqual(FakeApi.uploads, {})

    def test_regression_card_declares_valuation_and_fixed_window_without_future_prices(self):
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder) / "release"
            release(base)
            config = read_json(base / "config.json")
            write_json(base / "config.json", {**config, "input_features": "monthly-risk-regression-v1"})
            card = dataset_card(base, "pool-scoring", "me/labels", {})
            self.assertIn("daily_returns_1m", card)
            self.assertIn("clenow_mark_90", card)
            self.assertIn("不缩短90日窗口", card)
            self.assertIn("不携带回归原始价格", card)
            self.assertIn("上市史不足或无法估值仍为 null", card)

    def test_fetch_restores_published_release_without_scorecards_and_never_overwrites(self):
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder)
            release(base / "release")
            (base / "release" / "universe").mkdir()
            runs(base / "cards")
            publish(base / "release", "me/bench", "me/labels", "pool-scoring", base / "cards")
            out = base / "fetched" / "pool-scoring-v9"
            result = fetch("me/bench", "me/labels", "pool-scoring", "pool-scoring-v9", out)
            self.assertEqual(result["status"], "downloaded")
            self.assertEqual(sorted(p.name for p in out.iterdir()),
                             ["config.json", "inputs.jsonl", "manifest.json", "outcomes.jsonl", "summary.json"])
            self.assertEqual(fetch("me/bench", "me/labels", "pool-scoring", "pool-scoring-v9", out)["status"], "cached")
            (out / "outcomes.jsonl").write_text("[]\n", encoding="utf-8")
            with self.assertRaises(Invalid):
                fetch("me/bench", "me/labels", "pool-scoring", "pool-scoring-v9", out)

    def test_fetch_rejects_tampered_download_and_leaves_no_directory(self):
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder)
            release(base / "release")
            runs(base / "cards")
            publish(base / "release", "me/bench", "me/labels", "pool-scoring", base / "cards")
            FakeApi.uploads["me/labels"]["pool-scoring/outcomes.jsonl"] = "[]\n"
            out = base / "fetched" / "release"
            with self.assertRaises(Invalid):
                fetch("me/bench", "me/labels", "pool-scoring", "pool-scoring-v9", out)
            self.assertEqual(list((base / "fetched").iterdir()), [])


if __name__ == "__main__":
    unittest.main()
