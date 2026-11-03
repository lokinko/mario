import sys
import tempfile
import types
import unittest
import hashlib
import subprocess
from fnmatch import fnmatch
from pathlib import Path
from unittest.mock import Mock, patch

from benchmark.files import Invalid, digest, read_json, write_json
from benchmark.hub import (CANARY, dataset_card, fetch, prepare_daily_prices, publish,
                           publish_daily_prices, promote_daily_branch)
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
    branches, heads, created_branch = {}, {}, {}

    def create_repo(self, repo_id, **_):
        pass

    def repo_info(self, repo_id, **_):
        return types.SimpleNamespace(private=self.private, sha=self.heads.get(repo_id, f"sha-{repo_id}"))

    def create_branch(self, repo_id, branch, revision, **_):
        FakeApi.created_branch = {"branch": branch, "revision": revision}
        FakeApi.branches[repo_id, branch] = dict(FakeApi.uploads.get(repo_id, {}))

    def delete_branch(self, repo_id, branch, **_):
        del FakeApi.branches[repo_id, branch]

    def upload_file(self, repo_id, path_or_fileobj, path_in_repo, revision, **_):
        FakeApi.branches[repo_id, revision][path_in_repo] = path_or_fileobj.decode()
        return types.SimpleNamespace(oid="sha-attrs")

    def list_repo_tree(self, repo_id, path_in_repo, revision, **_):
        for name, value in FakeApi.branches[repo_id, revision].items():
            if name.startswith(path_in_repo + "/"):
                content = value.encode()
                yield types.SimpleNamespace(path=name, lfs=None,
                    blob_id=hashlib.sha1(f"blob {len(content)}\0".encode() + content).hexdigest())

    @staticmethod
    def promote(api, repo, branch, parent, staged_commit):
        if api.repo_info(repo).sha != parent:
            raise Invalid("main advanced")
        FakeApi.uploads[repo] = dict(FakeApi.branches[repo, branch])
        FakeApi.heads[repo] = "sha-promoted"
        return "sha-promoted"

    def file_exists(self, repo_id, name, **_):
        return name in FakeApi.uploads.get(repo_id, {})

    def list_repo_files(self, repo_id, **_):
        return list(FakeApi.uploads.get(repo_id, {}))

    def upload_folder(self, repo_id, folder_path, path_in_repo="", revision=None, delete_patterns=None, **kwargs):
        FakeApi.last_upload = kwargs
        prefix = path_in_repo + "/" if path_in_repo else ""
        target = FakeApi.branches[repo_id, revision] if revision else FakeApi.uploads.setdefault(repo_id, {})
        if delete_patterns:
            for name in list(target):
                if name.startswith(prefix):
                    del target[name]
        target.update(
            {prefix + p.relative_to(folder_path).as_posix(): p.read_text(encoding="utf-8")
             for p in Path(folder_path).rglob("*") if p.is_file()})
        return types.SimpleNamespace(oid=revision if revision else f"sha-{repo_id}")

    def create_tag(self, *_, **__):
        pass


def snapshot_download(repo_id, allow_patterns, local_dir, **_):
    for name, text in FakeApi.uploads[repo_id].items():
        if any(fnmatch(name, pattern) for pattern in allow_patterns):
            target = Path(local_dir) / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(text, encoding="utf-8")
    return local_dir


def hf_hub_download(repo_id, filename, local_dir, **_):
    target = Path(local_dir) / filename
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(FakeApi.uploads[repo_id][filename], encoding="utf-8")
    return str(target)


class HubTest(unittest.TestCase):
    def setUp(self):
        FakeApi.uploads, FakeApi.private, FakeApi.last_upload = {}, True, {}
        FakeApi.branches, FakeApi.heads, FakeApi.created_branch = {}, {}, {}
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)

        def download(repo_id, filename, local_dir=None, **kwargs):
            if filename == ".gitattributes":
                path = Path(temp.name) / "attrs"
                path.write_text(FakeApi.uploads.get(repo_id, {}).get(filename, ""))
                return str(path)
            return hf_hub_download(repo_id, filename, local_dir, **kwargs)

        self.attrs = download
        hub = types.SimpleNamespace(HfApi=FakeApi, snapshot_download=snapshot_download, hf_hub_download=download)
        self.module = patch.dict(sys.modules, {"huggingface_hub": hub})
        self.module.start()
        self.addCleanup(self.module.stop)
        promoter = patch("benchmark.hub.promote_daily_branch", side_effect=FakeApi.promote)
        self.promoter = promoter.start()
        self.addCleanup(promoter.stop)

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

            with patch("benchmark.hub.huggingface", return_value=types.SimpleNamespace(
                    HfApi=FakeApi, snapshot_download=download, hf_hub_download=self.attrs)):
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
                FakeApi.uploads["me/labels"]["daily-prices/retired-layout.jsonl"] = "old layout"
                publish_daily_prices(daily, "me/labels", "daily-prices", parent)
                self.assertEqual(FakeApi.created_branch["revision"], "sha-me/labels")
                self.assertFalse(FakeApi.branches)
                self.assertNotIn("daily-prices/retired-layout.jsonl", FakeApi.uploads["me/labels"])
                self.assertTrue(all(FakeApi.uploads["me/labels"][k] == v for k, v in old.items()))
                direct = Mock(wraps=hf_hub_download)
                adapter = types.SimpleNamespace(HfApi=FakeApi, snapshot_download=download, hf_hub_download=direct)
                with patch("benchmark.hub.huggingface", return_value=adapter):
                    _, previous, _, _ = prepare_daily_prices("me/bench", "me/labels", "v7", "daily-prices", base / "next")
                self.assertEqual({call.kwargs["revision"] for call in direct.call_args_list}, {"sha-promoted"})
                self.assertEqual({call.kwargs["filename"] for call in direct.call_args_list},
                                 {"daily-prices/manifest.json", "daily-prices/config.json"})
                self.assertTrue(verify_release(previous)["valid"])
                self.assertTrue(all(call["repo_id"] == "me/bench" for call in calls))

                for name, value in (("daily-prices/config.json", None), ("daily-prices/extra.json", "{}")):
                    with self.subTest(name=name):
                        original = FakeApi.uploads["me/labels"].copy()
                        if value is None:
                            del FakeApi.uploads["me/labels"][name]
                        else:
                            FakeApi.uploads["me/labels"][name] = value
                        with self.assertRaisesRegex(Invalid, "远端日线归档与 manifest 不一致"):
                            prepare_daily_prices("me/bench", "me/labels", "v7", "daily-prices", base / name.replace("/", "_"))
                        FakeApi.uploads["me/labels"] = original

    def test_daily_refuses_public_repo_and_scoring_path(self):
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder)
            with self.assertRaises(Invalid):
                prepare_daily_prices("me/bench", "me/labels", "v7", "pool-scoring", base)
            FakeApi.private = False
            with self.assertRaises(Invalid):
                prepare_daily_prices("me/bench", "me/labels", "v7", "daily-prices", base)
            self.assertEqual(FakeApi.uploads, {})

    def test_explicit_rebuild_preserves_scope_and_audit_without_trusting_missing_prices(self):
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder)
            source = base / "source"
            source.mkdir()
            (source / "universe").mkdir()
            write_json(source / "universe/tech-2025.json", {"selected": ["000001.SZ"]})
            write_json(source / "manifest.json", {"files": {
                "universe/tech-2025.json": digest(source / "universe/tech-2025.json")}})
            FakeApi().upload_folder("me/bench", source, "pool-scoring")
            daily = base / "daily"
            daily.mkdir()
            write_json(daily / "config.json", {"schema_version": "public-prices-v1", "start": "2005-08-01",
                       "requested_end": "2026-11-02", "symbols": ["600000.SH"]})
            (daily / "index-membership").mkdir()
            (daily / "index-membership/original.xls").write_text("original", encoding="utf-8")
            write_json(daily / "manifest.json", {"files": {
                "config.json": digest(daily / "config.json"),
                "index-membership/original.xls": digest(daily / "index-membership/original.xls"),
                "prices/missing.jsonl": "missing-hash"}})
            FakeApi().upload_folder("me/labels", daily, "daily-prices")
            before = FakeApi.uploads["me/labels"].copy()
            with self.assertRaisesRegex(Invalid, "远端日线归档"):
                prepare_daily_prices("me/bench", "me/labels", "v7", "daily-prices", base / "normal")
            restored, previous, parent, provenance = prepare_daily_prices(
                "me/bench", "me/labels", "v7", "daily-prices", base / "rebuild", rebuild=True)
            self.assertIsNone(previous)
            self.assertEqual(parent, "sha-me/labels")
            self.assertEqual(provenance["rebuild"]["symbols"], ["600000.SH"])
            self.assertEqual(provenance["rebuild"]["missing_files"], 1)
            self.assertEqual((restored / "index-membership/original.xls").read_text(), "original")
            self.assertEqual(FakeApi.uploads["me/labels"], before)
            FakeApi.uploads["me/labels"]["daily-prices/config.json"] = "{}"
            with self.assertRaisesRegex(Invalid, "配置哈希"):
                prepare_daily_prices("me/bench", "me/labels", "v7", "daily-prices", base / "bad", rebuild=True)

    def test_manifest_mismatch_reports_missing_and_extra_files(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder) / "release"
            release(root)
            (root / "summary.json").unlink()
            (root / "unexpected.json").write_text("{}", encoding="utf-8")
            with self.assertRaisesRegex(Invalid, r"缺失 1 个 \['summary.json'\]；多余 1 个 \['unexpected.json'\]"):
                verify_release(root)

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
                with self.assertRaisesRegex(Invalid, "conflict"):
                    publish_daily_prices(base / "daily", "me/labels", "daily-prices", "old-sha")
            self.assertNotIn("me/labels", FakeApi.uploads)

    def test_daily_staging_corruption_never_reaches_main(self):
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder)
            release(base / "daily")
            config = base / "daily/config.json"
            write_json(config, {"last_session": "2026-11-02"})
            manifest = read_json(base / "daily/manifest.json")
            manifest["files"]["config.json"] = digest(config)
            write_json(base / "daily/manifest.json", manifest)
            original = FakeApi.upload_folder

            def corrupt(api, repo_id, revision, **kwargs):
                result = original(api, repo_id=repo_id, revision=revision, **kwargs)
                FakeApi.branches[repo_id, revision]["daily-prices/config.json"] = "tampered"
                return result

            with patch.object(FakeApi, "upload_folder", corrupt):
                with self.assertRaisesRegex(Invalid, "不符"):
                    publish_daily_prices(base / "daily", "me/labels", "daily-prices", "sha-me/labels")
            self.promoter.assert_not_called()
            self.assertNotIn("me/labels", FakeApi.uploads)
            self.assertTrue(FakeApi.branches)


class DailyPromotionGitTest(unittest.TestCase):
    def test_multicommit_fast_forward_and_concurrent_changes(self):
        # 真实Git提交图验证SDK分批提交的提升，避免浅克隆截断祖先造成 fetch-first。
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder)
            remote = base / "datasets/me/labels"
            remote.parent.mkdir(parents=True)
            subprocess.run(["git", "init", "--bare", "--quiet", str(remote)], check=True)
            work = base / "work"
            work.mkdir()

            def git(*args):
                return subprocess.check_output(["git", *args], cwd=work, text=True, stderr=subprocess.PIPE).strip()

            git("init", "--quiet")
            git("config", "user.name", "Benchmark test")
            git("config", "user.email", "benchmark@example.invalid")
            git("commit", "--quiet", "--allow-empty", "-m", "base")
            parent = git("rev-parse", "HEAD")
            git("remote", "add", "origin", str(remote))
            git("push", "origin", "HEAD:refs/heads/main")
            for name in ("attributes", "batch1", "batch2", "manifest"):
                git("commit", "--quiet", "--allow-empty", "-m", name)
            staged = git("rev-parse", "HEAD")
            git("push", "origin", "HEAD:refs/heads/staging")
            api = types.SimpleNamespace(endpoint=str(base), whoami=lambda: {"name": "me"})
            with patch("benchmark.hub.huggingface", return_value=types.SimpleNamespace(get_token=lambda: "test-token")):
                commit = promote_daily_branch(api, "me/labels", "staging", parent, staged)
                self.assertEqual(commit, staged)
                self.assertEqual(git("ls-remote", "origin", "refs/heads/main").split()[0], staged)
                with self.assertRaisesRegex(Invalid, "主分支已前进"):
                    promote_daily_branch(api, "me/labels", "staging", parent, staged)
                with self.assertRaisesRegex(Invalid, "暂存分支已变更"):
                    promote_daily_branch(api, "me/labels", "staging", staged, parent)
                self.assertEqual(git("ls-remote", "origin", "refs/heads/main").split()[0], staged)


if __name__ == "__main__":
    unittest.main()
