import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("watch_data", Path(__file__).with_name("prepare-watch-dividends.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class PreparationTest(unittest.TestCase):
    def fixture(self, root, records, status="observed"):
        (root / "raw").mkdir()
        (root / "stocks").mkdir()
        raw = {"symbol": "600036.SH", "page": 1, "source_uri": "https://example.test/dividends",
               "retrieved_at": "2026-11-04T14:00:00+08:00",
               "response": {"success": True, "result": {"pages": 1, "count": len(records), "data": records}}}
        if status == "source_no_records":
            raw["response"] = {"code": 9201, "result": None}
        files = {}
        def write(name, value):
            content = json.dumps(value, ensure_ascii=False).encode()
            (root / name).write_bytes(content)
            files[name] = hashlib.sha256(content).hexdigest()
        write("raw/600036.SH-1.json", raw)
        write("stocks/600036.SH.json", {"summary": {"symbol": "600036.SH", "security_name": "招商银行", "status": status,
                                                     "raw_files": {"600036.SH-1.json": files["raw/600036.SH-1.json"]}}})
        write("coverage.json", {"end": "2026-11-04", "securities": 1, "window_basis": "ex_dividend_date"})
        (root / "manifest.json").write_text(json.dumps({"files": files}), encoding="utf-8")

    def record(self, **changes):
        return {"SECUCODE": "600036.SH", "SECURITY_CODE": "600036", "ASSIGN_PROGRESS": "实施分配",
                "EX_DIVIDEND_DATE": "2026-07-01", "REPORT_DATE": "2025-12-31",
                "PRETAX_BONUS_RMB": "1.234567890123456789", "BONUS_RATIO": None, "IT_RATIO": None, **changes}

    def test_future_and_proposed_records_are_excluded_and_decimals_are_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root, [self.record(), self.record(EX_DIVIDEND_DATE="2026-11-11"), self.record(ASSIGN_PROGRESS="预披露")])
            stock = module.prepare(root)["stocks"][0]
            self.assertEqual(stock["status"], "ready")
            events = stock["history"]["events"]
            self.assertEqual(len(events), 1)
            self.assertEqual(events[0]["cashPerTen"], "1.234567890123456789")
            self.assertEqual(events[0]["bonusPerTen"], "0")

    def test_missing_implementation_date_is_unknown_and_old_report_labels_are_not_payment_dates(self):
        for ex_date, report_date, expected in [(None, "2020-12-31", "source_incomplete"), ("1996-12-09", "1996-12-31", "ready")]:
            with self.subTest(expected=expected), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                self.fixture(root, [self.record(EX_DIVIDEND_DATE=ex_date, REPORT_DATE=report_date)])
                stock = module.prepare(root)["stocks"][0]
                self.assertEqual(stock["status"], expected)
                if expected == "source_incomplete":
                    self.assertIsNone(stock["history"])

    def test_no_source_records_never_becomes_a_known_zero(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root, [], "source_no_records")
            self.assertIsNone(module.prepare(root)["stocks"][0]["history"])

    def test_modified_original_aborts_the_package_instead_of_silently_skipping(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root, [self.record()])
            (root / "raw/600036.SH-1.json").write_text("{}", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "哈希不符"):
                module.prepare(root)


if __name__ == "__main__":
    unittest.main()
