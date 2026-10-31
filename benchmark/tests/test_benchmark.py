import ast
import copy
import io
import json
import os
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from datetime import date, timedelta
from decimal import Decimal
from pathlib import Path
from unittest.mock import patch

from benchmark.cli import main
from benchmark.dataset import Dataset, select_universe, validate_config
from benchmark.evaluate import baseline, correlation, evaluate
from benchmark.files import Invalid, digest, local_file, read_json, rows, write_json, write_rows
from benchmark.release import build, verify_release
from benchmark.public_sources import history_coverage, probe_public, report_catalog
from benchmark.sources import Tushare, collect_prices, collect_universe, fetch_documents


def fixture(root):
    """全部为合成数据；星期一至五仅用于测试，不能作为真实中国交易日历。"""
    root.mkdir()
    config = read_json(Path(__file__).parents[1] / "config.json")
    config.update(version="synthetic-test-only", source_start="2018-01-01",
                  start="2019-01-01", end="2021-12-31", horizons=[21, 63, 252],
                  splits={"train": ["2019-01-01", "2019-12-31"],
                          "validation": ["2020-01-01", "2020-12-31"],
                          "test": ["2021-01-01", "2021-12-31"]})
    write_json(root / "config.json", config)
    source = "https://example.org/synthetic-test-only"
    calendar = []
    day = date(2018, 1, 1)
    while day <= date(2021, 12, 31):
        calendar.append({"date": day.isoformat(), "is_open": day.weekday() < 5, "source_uri": source})
        day += timedelta(days=1)
    dates = [r["date"] for r in calendar if r["is_open"]]
    stocks = [f"{600000 + n}.SH" for n in range(21)]
    etfs = [r["symbol"] for r in config["etfs"]]
    instruments = [{"symbol": code, "kind": "stock" if code in stocks else "etf",
                    "name": "合成测试证券", "listed_on": "2010-01-01", "delisted_on": None,
                    "source_uri": source} for code in stocks + etfs]
    universes = []
    for year in range(2019, 2022):
        selection = max(d for d in dates if d < f"{year}-01-01")
        universes.append({"year": year, "selection_date": selection,
                          "available_at": selection + "T18:00:00+08:00",
                          "membership_published_at": selection + "T12:00:00+08:00",
                          "source_uri": source, "membership_verified": True, "candidate_count": 21,
                          "candidates": [{"symbol": code, "market_cap": str(100000 + (n + year) % 21)}
                                         for n, code in enumerate(stocks)]})
    prices = []
    for n, code in enumerate(stocks + etfs):
        for index, value in enumerate(dates):
            price = Decimal(10) + Decimal(index * (n + 1)) / 10000
            prices.append({"symbol": code, "date": value, "status": "trading",
                           "open": str(price), "close": str(price + Decimal("0.01")),
                           "high": str(price + 1), "low": str(price - 1), "volume": "10000",
                           "amount": "100000", "adj_factor": "1", "source_uri": source,
                           "adjustment_basis": "provider_total_return_proxy",
                           "available_at": value + "T18:00:00+08:00"})
    document = root / "synthetic.pdf"
    document.write_bytes(b"%PDF-1.4\nsynthetic test fixture only\n%%EOF\n")
    reports = []
    for code in stocks:
        for year in range(2019, 2022):
            for month, suffix, kind in ((3, "03-31", "q1"), (6, "06-30", "half_year"),
                                        (9, "09-30", "q3"), (12, "12-31", "annual")):
                published = f"{year + (month == 12)}-{(month % 12) + 1:02d}-20T18:00:00+08:00"
                reports.append({"symbol": code, "period_end": f"{year}-{suffix}", "report_type": kind,
                                "published_at": published, "available_at": published,
                                "revision_id": "original", "version_verified": True,
                                "source_uri": source, "document_path": "synthetic.pdf",
                                "sha256": digest(document), "metrics": {"revenue_ytd_cny": "1000000"}})
    for name, table in (("calendar", calendar), ("instruments", instruments), ("universe", universes),
                        ("prices", prices), ("reports", reports)):
        write_rows(root / f"{name}.jsonl", table)


class BenchmarkTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory()
        cls.root = Path(cls.temp.name) / "input"
        fixture(cls.root)

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    def setUp(self):
        self.dataset = Dataset(self.root)

    def test_complete_fixture_and_historical_changes(self):
        self.assertEqual(self.dataset.validate()["errors"], [])
        self.assertEqual(len(self.dataset.selected_symbols()), 21)
        self.assertEqual(len(list(self.dataset.samples())), 12)

    def test_market_cap_uses_exact_decimal_and_stable_ties(self):
        candidates = [{"symbol": f"{600000 + i}.SH", "market_cap": "9007199254740992"} for i in range(21)]
        candidates[-1]["market_cap"] = "9007199254740993"
        result = select_universe({"candidates": candidates})
        self.assertEqual(result[0]["symbol"], "600020.SH")
        self.assertEqual(result[1]["symbol"], "600000.SH")
        self.assertEqual(len(result), 20)

    def test_unknown_membership_and_late_selection_rejected(self):
        row = self.dataset.universes[2019]
        row["membership_verified"] = False
        self.assertTrue(any("核对历史成分" in e for e in self.dataset.validate()["errors"]))
        row["membership_verified"] = True
        row["available_at"] = "2019-01-01T10:00:00+08:00"
        self.assertTrue(any("时点错误" in e for e in self.dataset.validate()["errors"]))

    def test_duplicate_and_partial_universe_rejected(self):
        self.dataset.universes[2019]["candidate_count"] = 100
        self.assertTrue(any("未完整导入" in e for e in self.dataset.validate()["errors"]))

    def test_future_reports_and_revisions_not_backfilled(self):
        report = self.dataset.tables["reports"][0]
        code = report["symbol"]
        self.assertEqual(self.dataset.feature(code, "2019-03-29")["reports"], [])
        before = self.dataset.feature(code, "2019-06-28")["reports"]
        self.assertEqual(before[0]["metrics"]["revenue_ytd_cny"], "1000000")
        revision = {**report, "revision_id": "revision", "published_at": "2019-07-01T18:00:00+08:00",
                    "available_at": "2019-07-01T18:00:00+08:00", "metrics": {"revenue_ytd_cny": "900000"}}
        self.dataset.tables["reports"].append(revision)
        self.assertEqual(self.dataset.feature(code, "2019-06-28")["reports"][0]["revision_id"], "original")
        self.assertEqual(self.dataset.feature(code, "2019-09-30")["reports"][0]["revision_id"], "revision")

    def test_late_price_not_in_features(self):
        row = self.dataset.prices[("600000.SH", "2019-03-29")]
        row["available_at"] = "2019-03-30T01:00:00+08:00"
        self.assertIsNone(self.dataset.feature("600000.SH", "2019-03-29")["close"])

    def test_no_price_or_label_forward_fill_for_suspension(self):
        sample = next(self.dataset.samples())
        code = sample["stocks"][0]["symbol"]
        entry = self.dataset.calendar[self.dataset.calendar.index(sample["decision_date"]) + 1]
        self.dataset.prices[(code, entry)]["status"] = "suspended"
        labels = list(self.dataset.labels(sample))
        self.assertTrue(all(r["adjusted_return_proxy"] is None for r in labels if r["symbol"] == code))

    def test_labels_use_next_open_and_purge_split_boundary(self):
        samples = list(self.dataset.samples())
        labels = [r for s in samples for r in self.dataset.labels(s)]
        self.assertTrue(labels)
        for row in labels:
            self.assertLessEqual(row["exit_date"], self.dataset.config["splits"][row["split"]][1])
            sample = next(s for s in samples if s["id"] == row["sample_id"])
            self.assertGreater(row["entry_date"], sample["decision_date"])
        self.assertFalse(list(self.dataset.labels(samples[3])))
        row = labels[0]
        entry = self.dataset.price(row["symbol"], row["entry_date"])
        exit_row = self.dataset.price(row["symbol"], row["exit_date"])
        self.assertEqual(Decimal(row["adjusted_return_proxy"]), Decimal(exit_row["close"]) / Decimal(entry["open"]) - 1)

    def test_missing_price_and_report_block_release(self):
        self.dataset.tables["reports"].pop(0)
        errors = self.dataset.validate()["errors"]
        self.assertTrue(any("财报 600000.SH/2019-03-31" in e for e in errors), errors)

    def test_invalid_ohlc_float_and_hash(self):
        row = self.dataset.tables["prices"][0]
        row["low"] = "99999"
        self.assertTrue(any("OHLC" in e for e in self.dataset.validate()["errors"]))
        row["low"] = 1.0
        self.assertTrue(any("十进制字符串" in e for e in self.dataset.validate()["errors"]))
        self.dataset.tables["reports"][0]["sha256"] = "0" * 64
        self.assertTrue(any("SHA256" in e for e in self.dataset.validate()["errors"]))

    def test_path_traversal_and_unknown_timezone_rejected(self):
        with self.assertRaises(Invalid):
            local_file(self.root, "../outside.pdf")
        self.dataset.tables["reports"][0]["available_at"] = "2019-04-20T18:00:00"
        self.assertTrue(any("时区" in e for e in self.dataset.validate()["errors"]))

    def test_config_split_overlap_rejected(self):
        config = copy.deepcopy(self.dataset.config)
        config["splits"]["validation"][0] = "2019-12-31"
        with self.assertRaises(Invalid):
            validate_config(config)

    def test_release_build_baseline_evaluation_and_tampering(self):
        with tempfile.TemporaryDirectory() as directory:
            release = Path(directory) / "release"
            summary = build(self.root, release)
            self.assertEqual(summary["unique_stocks"], 21)
            self.assertFalse(summary["published"])
            self.assertTrue(verify_release(release)["valid"])
            samples = rows(release / "inputs/test.jsonl")
            labels = rows(release / "labels/test.jsonl")
            self.assertNotIn("adjusted_return_proxy", json.dumps(samples))
            report = evaluate(samples, labels, list(baseline(samples, 63)), "510300.SH", "test", 63)
            self.assertEqual(report["evaluated_periods"], 3)
            self.assertGreater(report["mean_rank_ic"], 0.9)
            with self.assertRaises(Invalid):
                build(self.root, release)
            (release / "inputs/test.jsonl").write_text("tampered", encoding="utf-8")
            with self.assertRaises(Invalid):
                verify_release(release)

    def test_evaluation_exposes_incomplete_predictions_and_rejects_duplicates(self):
        samples = list(self.dataset.samples())
        labels = [r for s in samples for r in self.dataset.labels(s)]
        predictions = list(baseline(samples, 63))
        predictions = [p for p in predictions if p["symbol"] != "600001.SH"]
        report = evaluate(samples, labels, predictions, "510300.SH", "test", 63)
        self.assertEqual(report["evaluated_periods"], 0)
        self.assertTrue(report["excluded"])
        with self.assertRaises(Invalid):
            evaluate(samples, labels, predictions + predictions[:1], "510300.SH", "test", 63)

    def test_rank_ties_and_constant_predictions(self):
        self.assertIsNone(correlation([1, 1, 1], [3, 2, 1]))
        self.assertAlmostEqual(correlation([1, 2, 2, 3], [1, 2, 2, 3]), 1)

    def test_empty_cli_and_no_token_fail_honestly(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "empty"
            with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
                self.assertEqual(main(["init", "--root", str(root)]), 0)
                self.assertEqual(main(["plan", "--root", str(root)]), 0)
                self.assertEqual(main(["validate", "--root", str(root)]), 1)
                self.assertEqual(main(["build", "--root", str(root), "--out", str(root / "release")]), 1)
            with patch.dict(os.environ, {}, clear=True), self.assertRaises(Invalid):
                Tushare(root / "cache")

    def test_source_cache_excludes_credentials_and_rejects_permission_error(self):
        class Response(io.BytesIO):
            pass

        payload = {"code": 0, "data": {"fields": ["close"], "items": [["10.123456789"]]}}
        with tempfile.TemporaryDirectory() as directory, patch.dict(os.environ, {"TUSHARE_TOKEN": "test-secret"}):
            client = Tushare(directory, interval=0)
            with patch("benchmark.sources.urlopen", return_value=Response(json.dumps(payload).encode())) as request:
                self.assertEqual(client.query("daily", ts_code="600000.SH"), [{"close": "10.123456789"}])
                self.assertEqual(client.query("daily", ts_code="600000.SH"), [{"close": "10.123456789"}])
                self.assertEqual(request.call_count, 1)
            self.assertNotIn("test-secret", next(Path(directory).glob("*.json")).read_text())
            with patch("benchmark.sources.urlopen", return_value=Response(b'{"code":-1,"msg":"test-secret"}')):
                with self.assertRaises(Invalid) as error:
                    client.query("fund_daily")
                self.assertNotIn("test-secret", str(error.exception))

    def test_document_sources_cannot_download_arbitrary_sites(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write_rows(root / "report_sources.jsonl", [{"source_uri": "https://example.org/a.pdf"}])
            with self.assertRaises(Invalid):
                fetch_documents(root)

    def test_research_modules_only_import_stdlib_and_own_package(self):
        optional = {"baostock_market.py": {"baostock"}}
        for path in Path(__file__).parents[1].glob("*.py"):
            tree = ast.parse(path.read_text(encoding="utf-8"))
            for node in ast.walk(tree):
                names = []
                if isinstance(node, ast.Import):
                    names = [name.name for name in node.names]
                elif isinstance(node, ast.ImportFrom) and not node.level:
                    names = [node.module]
                for name in names:
                    allowed = sys.stdlib_module_names | {"benchmark"} | optional.get(path.name, set())
                    self.assertIn(name.split(".")[0], allowed, path.name)

    def test_market_collector_preserves_precision_units_and_gaps(self):
        class FakeProvider:
            def query(self, api, **params):
                if api == "trade_cal":
                    return [{"cal_date": "20190102", "is_open": 1}]
                if api in ("adj_factor", "fund_adj"):
                    return [{"trade_date": "20190102", "adj_factor": "1.00000001"}]
                return [{"trade_date": "20190102", "open": "10.123456789", "high": "11",
                         "low": "9", "close": "10.99", "vol": "12.34", "amount": "56.78"}]

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            config = copy.deepcopy(self.dataset.config)
            config.update(source_start="2018-12-31", start="2019-01-01", end="2019-12-31")
            write_json(root / "config.json", config)
            write_rows(root / "universe.jsonl", self.dataset.tables["universe"][:1])
            write_rows(root / "instruments.jsonl", self.dataset.tables["instruments"])
            collect_prices(root, FakeProvider())
            prices = rows(root / "prices.jsonl")
            self.assertEqual(prices[0]["open"], "10.123456789")
            self.assertEqual(Decimal(prices[0]["volume"]), Decimal("1234"))
            self.assertEqual(Decimal(prices[0]["amount"]), Decimal("56780"))
            self.assertFalse(any(row["status"] == "suspended" for row in prices))
            with self.assertRaises(Invalid):
                collect_prices(root, FakeProvider())

    def test_historical_universe_collection_requires_evidence_review(self):
        class FakeProvider:
            def query(self, api, **params):
                if api == "trade_cal":
                    return [{"cal_date": params["end_date"], "is_open": 1}]
                if api == "index_weight":
                    return [{"trade_date": params["end_date"], "con_code": f"{600000 + i}.SH"} for i in range(21)]
                if api == "daily_basic":
                    return [{"total_mv": "123456.789"}]
                return [{"ts_code": params["ts_code"], "name": "合成证券", "list_date": "20100101", "delist_date": None}]

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write_json(root / "config.json", self.dataset.config)
            collect_universe(root, FakeProvider(), "000922.SH")
            snapshot = rows(root / "universe.jsonl")[0]
            self.assertFalse(snapshot["membership_verified"])
            self.assertIsNone(snapshot["membership_published_at"])
            self.assertEqual(Decimal(snapshot["candidates"][0]["market_cap"]), Decimal("1234567890"))

    def test_public_history_does_not_infer_completeness_or_fill_nulls(self):
        result = {"timestamp": [1451865600, 1451952000],
                  "indicators": {"quote": [{"open": [1, None], "high": [2, None],
                                            "low": [1, None], "close": [2, None]}]},
                  "events": {"dividends": {"one": {}}}}
        coverage = history_coverage(result, "2016-01-01", "2016-12-31")
        self.assertEqual(coverage["observed_sessions"], 1)
        self.assertEqual(coverage["null_ohlc_dates"], ["2016-01-05"])
        self.assertEqual(coverage["dividend_events"], 1)
        self.assertIn("pending", coverage["completeness"])

    def test_public_report_catalog_filters_summaries_and_future_periods(self):
        common = {"announcementTime": 1588118400000, "adjunctUrl": "finalpage/test.pdf"}
        titles = ["公司2020年第一季度报告", "公司2020年第一季度报告摘要", "公司2020年第一季度报告（修订版）",
                  "公司2020年第一季度报告更正公告", "公司2026年第一季度报告"]
        announcements = [{**common, "announcementId": str(i), "announcementTitle": title} for i, title in enumerate(titles)]
        catalog = report_catalog("600000.SH", announcements, "2016-01-01", "2025-12-31")
        self.assertEqual(len(catalog), 2)
        self.assertTrue(all(not row["version_verified"] for row in catalog))
        self.assertTrue(all(row["available_at"].endswith("23:59:59+08:00") for row in catalog))

    def test_public_probe_records_source_failures_without_claiming_release_ready(self):
        with tempfile.TemporaryDirectory() as directory, patch("benchmark.public_sources.yahoo_history", side_effect=ValueError("limited")), \
                patch("benchmark.public_sources.download", side_effect=ValueError("blocked")), \
                patch("benchmark.public_sources.cninfo_announcements", side_effect=ValueError("unavailable")):
            output = Path(directory) / "probe"
            result = probe_public(self.root / "config.json", output)
            self.assertFalse(result["release_ready"])
            self.assertEqual(len(result["errors"]), 7)
            self.assertTrue((output / "coverage.json").exists())


if __name__ == "__main__":
    unittest.main()
