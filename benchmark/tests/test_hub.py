import sys
import tempfile
import types
import unittest
from fnmatch import fnmatch
from pathlib import Path
from unittest.mock import patch

from benchmark.files import Invalid, digest, read_json, write_json
from benchmark.hub import (CANARY, dataset_card, fetch, prepare_daily_prices, publish,
                           publish_daily_prices)
from benchmark.release import verify_release

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
    uploads, private, last_upload = {}, True, {}

    def create_repo(self, repo_id, **_):
        pass

    def repo_info(self, repo_id, **_):
        return types.SimpleNamespace(private=self.private, sha=f"sha-{repo_id}")

    def file_exists(self, repo_id, name, **_):
        return name in FakeApi.uploads.get(repo_id, {})

    def upload_folder(self, repo_id, folder_path, path_in_repo="", **kwargs):
        FakeApi.last_upload = kwargs
        prefix = path_in_repo + "/" if path_in_repo else ""
        FakeApi.uploads.setdefault(repo_id, {}).update(
            {prefix + p.relative_to(folder_path).as_posix(): p.read_text(encoding="utf-8")
             for p in Path(folder_path).rglob("*") if p.is_file()})
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
        FakeApi.uploads, FakeApi.private, FakeApi.last_upload = {}, True, {}
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

    def test_fugui_card_preserves_old_feature_notes_and_labels_partial_test(self):
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder) / "release"
            release(base)
            config = read_json(base / "config.json")
            write_json(base / "config.json", {**config, "input_features": "monthly-risk-regression-fugui-v1"})
            card = dataset_card(base, "pool-scoring", "me/labels", {})
            for text in ("daily_returns_1m", "clenow_mark_90", "stock.fugui", "monthly_low", "first_pullback",
                         "历史不足或未知缺行保留 null", "不是完整高股息国企策略复现", "fugui-study.json"):
                self.assertIn(text, card)

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

    def test_daily_restore_is_pinned_and_publish_preserves_scoring_files(self):
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder)
            release(base / "release")
            runs(base / "cards")
            universe = base / "release/universe"
            universe.mkdir()
            write_json(universe / "tech-2025.json", {"pool": "tech", "year": 2025, "selected": ["000001.SZ"]})
            manifest = read_json(base / "release/manifest.json")
            manifest["files"]["universe/tech-2025.json"] = digest(universe / "tech-2025.json")
            write_json(base / "release/manifest.json", manifest)
            publish(base / "release", "me/bench", "me/labels", "pool-scoring", base / "cards")
            calls = []

            def download(**kwargs):
                calls.append(kwargs)
                return snapshot_download(**kwargs)

            with patch("benchmark.hub.huggingface", return_value=types.SimpleNamespace(HfApi=FakeApi, snapshot_download=download)):
                source, previous, parent, provenance = prepare_daily_prices(
                    "me/bench", "me/labels", "v7", "daily-prices", base / "first")
                self.assertIsNone(previous)
                self.assertEqual(provenance["commit"], "sha-me/bench")
                self.assertEqual(calls[0]["revision"], "sha-me/bench")
                self.assertTrue((source / "universe/tech-2025.json").exists())
                self.assertFalse((source / "outcomes.jsonl").exists())
                # 独立日线发布包，不要求先评分，也不会改写评分数据集卡。
                daily = base / "daily"
                daily.mkdir()
                write_json(daily / "config.json", {"last_session": "2026-11-02"})
                write_json(daily / "manifest.json", {"files": {"config.json": digest(daily / "config.json")}})
                old = dict(FakeApi.uploads["me/labels"])
                publish_daily_prices(daily, "me/labels", "daily-prices", parent)
                self.assertEqual(FakeApi.last_upload["parent_commit"], "sha-me/labels")
                self.assertTrue(all(FakeApi.uploads["me/labels"][k] == v for k, v in old.items()))
                _, previous, _, _ = prepare_daily_prices("me/bench", "me/labels", "v7", "daily-prices", base / "next")
                self.assertTrue(verify_release(previous)["valid"])
                self.assertEqual(calls[-1]["revision"], "sha-me/labels")

    def test_daily_refuses_public_repo_and_scoring_path(self):
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder)
            with self.assertRaises(Invalid):
                prepare_daily_prices("me/bench", "me/labels", "v7", "pool-scoring", base)
            FakeApi.private = False
            with self.assertRaises(Invalid):
                prepare_daily_prices("me/bench", "me/labels", "v7", "daily-prices", base)
            self.assertEqual(FakeApi.uploads, {})

    def test_daily_rejects_changed_snapshot_and_upload_conflict(self):
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder)
            FakeApi.uploads["me/bench"] = {
                "pool-scoring/manifest.json": '{"files":{"universe/tech-2025.json":"bad-hash"}}',
                "pool-scoring/universe/tech-2025.json": '{}'}
            with self.assertRaisesRegex(Invalid, "哈希"):
                prepare_daily_prices("me/bench", "me/labels", "v7", "daily-prices", base / "source")
            release(base / "daily")
            config = base / "daily/config.json"
            write_json(config, {"last_session": "2026-11-02"})
            manifest = read_json(base / "daily/manifest.json")
            manifest["files"]["config.json"] = digest(config)
            write_json(base / "daily/manifest.json", manifest)
            with patch.object(FakeApi, "upload_folder", side_effect=RuntimeError("parent commit conflict")):
                with self.assertRaisesRegex(RuntimeError, "conflict"):
                    publish_daily_prices(base / "daily", "me/labels", "daily-prices", "old-sha")
            self.assertNotIn("me/labels", FakeApi.uploads)


if __name__ == "__main__":
    unittest.main()
