import sys
import tempfile
import types
import unittest
from pathlib import Path
from unittest.mock import patch

from benchmark.files import Invalid, digest, write_json
from benchmark.hub import CANARY, publish

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


class HubTest(unittest.TestCase):
    def setUp(self):
        FakeApi.uploads, FakeApi.private = {}, True
        self.module = patch.dict(sys.modules, {"huggingface_hub": types.SimpleNamespace(HfApi=FakeApi)})
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


if __name__ == "__main__":
    unittest.main()
