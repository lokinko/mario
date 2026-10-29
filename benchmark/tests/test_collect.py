import copy
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from benchmark.collect import calendar_rows, collect_public, extra_reports, listing_observations, pdf_file, price_coverage, repair_catalogs, tencent_year, yahoo_rows
from benchmark.files import Invalid, digest, read_json, rows, symbol, write_json, write_rows
from benchmark.public_sources import report_catalog


class PublicCollectionTests(unittest.TestCase):
    def test_verified_listing_date_separates_prelisting_nav_observations(self):
        prices = [{"date": "2016-09-29"}, {"date": "2016-11-03"}, {"date": "2016-11-04"}, {"date": "2016-11-07"}]
        trading, prelisting = listing_observations(prices, {"listed_on": "2016-11-04", "source_uri": "https://www.sse.com.cn/announcement"})
        self.assertEqual([r["date"] for r in trading], ["2016-11-04", "2016-11-07"])
        self.assertEqual(len(prelisting), 2)
        self.assertEqual(listing_observations(prices, None), (prices, []))

    def test_repair_does_not_requery_report_periods_already_in_supplements(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "raw").mkdir()
            (root / "prices").mkdir()
            (root / "announcements").mkdir()
            write_json(root / "config.json", {"start": "2020-01-01", "end": "2020-12-31"})
            write_json(root / "request.json", {"selection": {"instruments": [{"symbol": "600000.SH"}]}})
            write_json(root / "raw/cninfo-stocks.json", {"stockList": []})
            write_rows(root / "prices/600000.SH.jsonl", [{"date": "2020-10-01"}])
            write_json(root / "announcements/600000.SH.json", [])
            row = {"announcementId": "annual", "announcementTitle": "2020年年度报告",
                   "announcementTime": 1619740800000, "adjunctUrl": "finalpage/annual.pdf"}
            write_json(root / "announcements/600000.SH-2020-12-31-supplement.json", {"announcements": [row]})
            with patch("benchmark.collect.cninfo_announcements") as request:
                result = repair_catalogs(root, root / "config.json")
                self.assertEqual(request.call_count, 0)
                self.assertEqual(result["checked"], 0)

    def test_beijing_symbol_and_tencent_response_are_date_scoped(self):
        self.assertEqual(symbol("920599.BJ"), "920599.BJ")
        payload = {"data": {"bj920599": {"day": [
            ["2023-01-03", "1", "2", "3", "1", "100"],
            ["2025-01-02", "1", "2", "3", "1", "100"],
            ["2026-01-05", "1", "2", "3", "1", "100"]]}}}
        with patch("benchmark.collect.download", return_value=json.dumps(payload).encode()):
            raw, output = tencent_year("920599.BJ", "2025")
        self.assertEqual(len(raw["data"]["bj920599"]["day"]), 3)
        self.assertEqual([r["date"] for r in output], ["2025-01-02"])
        self.assertEqual(output[0]["price_basis"], "tencent_unadjusted")

    def test_report_title_abbreviations_and_spacing_are_not_lost(self):
        titles = ["恒源煤电2025年一季度报告", "2025 年三季度报告", "2025年中期报告", "2025年年度报告",
                  "2025年一季度报告摘要", "2025年三季度报告更正公告"]
        announcements = [{"announcementTitle": title, "announcementId": str(i),
                          "announcementTime": 1777392000000, "adjunctUrl": f"finalpage/{i}.pdf"}
                         for i, title in enumerate(titles)]
        catalog = report_catalog("600971.SH", announcements, "2016-01-01", "2025-12-31")
        self.assertEqual({r["period_end"] for r in catalog}, {"2025-03-31", "2025-06-30", "2025-09-30", "2025-12-31"})
        self.assertEqual(len(catalog), 4)

    def test_report_real_world_filenames_and_year_variants(self):
        titles = ["600295_鄂尔多斯_2022年_年度报告", "中国民生银行2021年半年度A股报告（全文）",
                  "中国石化2021年半年报", "中粮糖业2023年上半年度报告", "中南传媒2020年年报",
                  "农业银行2022年度第一季度报告", "中国石油2025年年报"]
        announcements = [{"announcementTitle": title, "announcementId": str(i),
                          "announcementTime": 1777392000000, "adjunctUrl": f"finalpage/{i}.pdf"}
                         for i, title in enumerate(titles)]
        catalog = report_catalog("600000.SH", announcements, "2016-01-01", "2025-12-31")
        self.assertEqual(len(catalog), len(titles))
        self.assertEqual([r["period_end"] for r in catalog], ["2022-12-31", "2021-06-30", "2021-06-30",
                                                              "2023-06-30", "2020-12-31", "2022-03-31", "2025-12-31"])

    def test_chinese_year_is_normalized_without_changing_quarter_name(self):
        announcements = [{"announcementTitle": title, "announcementId": str(i),
                          "announcementTime": 1777392000000, "adjunctUrl": f"finalpage/{i}.pdf"}
                         for i, title in enumerate(["二〇二〇年半年度报告全文", "二零二一年第一季度报告"])]
        result = report_catalog("601998.SH", announcements, "2016-01-01", "2025-12-31")
        self.assertEqual([r["period_end"] for r in result], ["2020-06-30", "2021-03-31"])

    def test_extra_reports_require_cover_evidence_hash_and_keep_version_unverified(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "report.pdf").write_bytes(b"%PDF-1.4\nreport")
            sha = digest(root / "report.pdf")
            row = {"symbol": "600000.SH", "period_end": "2020-06-30", "report_type": "half_year",
                   "source_uri": "https://static.cninfo.com.cn/test.pdf", "document_path": "report.pdf",
                   "sha256": sha, "version_verified": False,
                   "period_review": {"method": "pdf_cover_text", "page": 1, "excerpt": "2020年半年度报告", "sha256": sha}}
            write_rows(root / "extra-reports.jsonl", [row])
            self.assertEqual(extra_reports(root), [row])
            (root / "report.pdf").write_bytes(b"%PDF-1.4\nchanged")
            with self.assertRaises(Invalid):
                extra_reports(root)

    def test_holidays_are_not_missing_price_sessions(self):
        calendar = calendar_rows(["20250930", "20251009", "20251010"], "2025-09-30", "2025-10-10")
        prices = [{"date": "2025-09-30"}, {"date": "2025-10-10"}]
        result = price_coverage(prices, calendar, "2025-09-30", "2025-10-10")
        self.assertEqual(result["internal_missing_sessions"], ["2025-10-09"])
        self.assertEqual(result["nontrading_observations"], [])
        self.assertEqual(sum(not r["is_open"] for r in calendar), 8)

    def test_prelisting_range_is_not_inferred_to_be_a_trading_gap(self):
        calendar = calendar_rows(["20250102", "20250103", "20250106"], "2025-01-02", "2025-01-06")
        result = price_coverage([{"date": "2025-01-06"}], calendar, "2025-01-02", "2025-01-06")
        self.assertEqual(result["internal_missing_sessions"], [])
        self.assertEqual(result["outside_observed_range_sessions"], 2)

    def test_calendar_must_cover_both_boundaries(self):
        with self.assertRaises(Invalid):
            calendar_rows(["20250102"], "2025-01-01", "2025-12-31")

    def test_yahoo_price_basis_and_decimal_text_preserved_null_not_filled(self):
        result = {"timestamp": [1451865600, 1451952000], "indicators": {
            "quote": [{"open": ["1.123456789", None], "high": ["2", None], "low": ["1", None],
                       "close": ["1.5", None], "volume": [100, None]}],
            "adjclose": [{"adjclose": ["1.4", None]}]}}
        output = yahoo_rows("600000.SH", result, "2016-01-01", "2016-01-31", "https://example.org")
        self.assertEqual(len(output), 1)
        self.assertEqual(output[0]["open"], "1.123456789")
        self.assertEqual(output[0]["adjusted_close"], "1.4")
        self.assertEqual(output[0]["price_basis"], "yahoo_provider_history_unreconciled")
        self.assertNotIn("adj_factor", output[0])

    def test_pdf_cache_reused_with_hash_and_unsafe_source_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "documents").mkdir()
            report = {"source_uri": "https://static.cninfo.com.cn/test.pdf"}
            with patch("benchmark.collect.download", return_value=b"%PDF-1.4\ntest") as request:
                first = pdf_file(root, report)
                second = pdf_file(root, report)
                self.assertEqual(request.call_count, 1)
                self.assertEqual(first["sha256"], second["sha256"])
            with self.assertRaises(Invalid):
                pdf_file(root, {"source_uri": "https://example.org/test.pdf"})

    def test_batch_resume_skips_completed_requests_and_preserves_unverified_status(self):
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            root = parent / "collection"
            config = {"start": "2020-01-01", "end": "2020-12-31", "source_start": "2019-01-01", "etfs": []}
            selection = {"scope": "test_candidates_only", "instruments": [{"symbol": "600000.SH"}]}
            write_json(parent / "config.json", config)
            write_json(parent / "symbols.json", selection)
            history = {"timestamp": [1577923200, 1609372800], "indicators": {"quote": [{
                "open": ["1", "2"], "high": ["2", "3"], "low": ["1", "2"],
                "close": ["1.5", "2.5"], "volume": [100, 100]}]}}
            announcement = {"announcementId": "one", "announcementTitle": "公司2020年第一季度报告",
                            "announcementTime": 1588118400000, "adjunctUrl": "finalpage/one.pdf"}

            def download(url):
                if url.endswith("calendar.json"):
                    return json.dumps(["20190101", "20200102", "20201231"]).encode()
                if url.endswith("szse_stock.json"):
                    return b'{"stockList":[]}'
                return b"%PDF-1.4\ntest"

            with patch("benchmark.collect.download", side_effect=download) as request, \
                    patch("benchmark.collect.yahoo_history", return_value=("https://example.org", {"chart": {"result": [history]}}, history)) as prices, \
                    patch("benchmark.collect.cninfo_announcements", return_value=[announcement]) as announcements:
                first = collect_public(parent / "config.json", parent / "symbols.json", root, 1)
                second = collect_public(parent / "config.json", parent / "symbols.json", root, 1)
                self.assertEqual(first["downloaded_reports"], 1)
                self.assertEqual(second["downloaded_reports"], 1)
                self.assertEqual(prices.call_count, 1)
                self.assertEqual(announcements.call_count, 1)
                self.assertEqual(request.call_count, 3)
                self.assertFalse(second["release_ready"])
                self.assertFalse(rows(root / "reports.proposed.jsonl")[0]["version_verified"])
                cached_pdf = root / rows(root / "reports.proposed.jsonl")[0]["document_path"]
                cached_pdf.write_bytes(b"%PDF-1.4\ntampered")
                damaged = collect_public(parent / "config.json", parent / "symbols.json", root, 1)
                self.assertEqual(damaged["downloaded_reports"], 0)
                self.assertIn("哈希变化", damaged["errors"][0])
            changed = copy.deepcopy(selection)
            changed["instruments"].append({"symbol": "600001.SH"})
            write_json(parent / "symbols.json", changed)
            with self.assertRaises(Invalid):
                collect_public(parent / "config.json", parent / "symbols.json", root, 1)

    def test_download_failures_stay_visible_and_do_not_claim_complete(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "collection"
            config = Path(directory) / "config.json"
            selection = Path(directory) / "symbols.json"
            write_json(config, {"start": "2020-01-01", "end": "2020-12-31", "source_start": "2019-01-01", "etfs": []})
            write_json(selection, {"instruments": [{"symbol": "600000.SH"}]})
            history = {"timestamp": [1577923200, 1609372800], "indicators": {"quote": [{
                "open": ["1", "2"], "high": ["2", "3"], "low": ["1", "2"],
                "close": ["1.5", "2.5"], "volume": [100, 100]}]}}
            announcement = {"announcementId": "one", "announcementTitle": "公司2020年第一季度报告",
                            "announcementTime": 1588118400000, "adjunctUrl": "finalpage/one.pdf"}

            def download(url):
                if url.endswith("calendar.json"):
                    return json.dumps(["20190101", "20200102", "20201231"]).encode()
                return b'{"stockList":[]}'

            with patch("benchmark.collect.download", side_effect=download), \
                    patch("benchmark.collect.yahoo_history", return_value=("https://example.org", {"chart": {"result": [history]}}, history)), \
                    patch("benchmark.collect.cninfo_announcements", return_value=[announcement]), \
                    patch("benchmark.collect.pdf_file", side_effect=ValueError("blocked PDF")):
                result = collect_public(config, selection, root, 1)
            self.assertEqual(result["status"], "collected_with_source_errors")
            self.assertEqual(result["downloaded_reports"], 0)
            self.assertEqual(len(result["errors"]), 1)
            self.assertFalse(result["release_ready"])
            self.assertEqual(rows(root / "reports.proposed.jsonl"), [])


if __name__ == "__main__":
    unittest.main()
