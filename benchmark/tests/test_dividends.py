"""分红事实的窗口、单位、方案状态和空来源区分；不访问真实服务。"""

import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from benchmark.dividends import collect_dividends, normalize
from benchmark.files import Invalid, read_json, rows
from benchmark.release import verify_release


def event(ex_date="2022-01-05", status="实施分配"):
    return {"SECUCODE": "600000.SH", "SECURITY_CODE": "600000", "REPORT_DATE": "2020-12-31 00:00:00",
            "ASSIGN_PROGRESS": status, "EX_DIVIDEND_DATE": ex_date, "NOTICE_DATE": "2021-12-25 00:00:00",
            "PLAN_NOTICE_DATE": "2021-03-31 00:00:00", "EQUITY_RECORD_DATE": "2022-01-04 00:00:00",
            "PRETAX_BONUS_RMB": "3.25", "BONUS_RATIO": None, "IT_RATIO": "2", "IMPL_PLAN_PROFILE": "10转2派3.25"}


class DividendTest(unittest.TestCase):
    def test_window_uses_payment_event_and_pending_plans_are_separate(self):
        implemented, plans = normalize("600000.SH", [event(), event("2027-01-01", "董事会预案")], "2021-11-04", "2026-11-04")
        self.assertEqual(len(implemented), 1)
        self.assertEqual(implemented[0]["report_date"], "2020-12-31")
        self.assertEqual(implemented[0]["cash_per_10_shares_cny_pretax"], "3.25")
        self.assertIsNone(implemented[0]["bonus_shares_per_10"])
        self.assertEqual(len(plans), 1)
        self.assertFalse(implemented[0]["pit_verified"])
        wrong = {**event(), "SECUCODE": "000001.SZ"}
        with self.assertRaises(Invalid):
            normalize("600000.SH", [wrong], "2021-11-04", "2026-11-04")

    def test_empty_source_preserves_response_and_does_not_claim_zero_dividends(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            payload = {"success": False, "code": 9201, "result": None, "message": "返回数据为空"}
            with patch("benchmark.dividends.dividend_page", return_value=("https://example.org/source", payload)):
                report = collect_dividends(["600000.SH"], "2021-11-04", "2026-11-04", root / "out", root / "cache")
            self.assertEqual(report["source_no_records"], ["600000.SH"])
            self.assertEqual(rows(root / "out/summary.jsonl")[0]["status"], "source_no_records")
            self.assertIsNone(read_json(root / "out/raw/600000.SH-1.json")["response"]["result"])
            self.assertTrue(verify_release(root / "out")["valid"])

    def test_pagination_cache_and_report_totals_are_checked(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            responses = [("https://example.org/1", {"result": {"count": 2, "pages": 2, "data": [event()]}}),
                         ("https://example.org/2", {"result": {"count": 2, "pages": 2, "data": [event("2023-01-05")]}})]
            with patch("benchmark.dividends.dividend_page", side_effect=responses) as request:
                report = collect_dividends(["600000.SH"], "2021-11-04", "2026-11-04", root / "out", root / "cache")
                self.assertEqual(request.call_count, 2)
            self.assertEqual(report["implemented_events"], 2)
            with patch("benchmark.dividends.dividend_page") as request:
                collect_dividends(["600000.SH"], "2021-11-04", "2026-11-04", root / "replay", root / "cache")
                request.assert_not_called()
            self.assertEqual(rows(root / "out/events.jsonl"), rows(root / "replay/events.jsonl"))
