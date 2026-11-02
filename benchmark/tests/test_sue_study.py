"""合成财报覆盖舍入、披露版本、季节窗口和股份变化；不请求财务供应商。"""

import copy
import tempfile
import unittest
from decimal import Decimal
from pathlib import Path

from benchmark.files import digest
from scripts.benchmark_sue_study import normalize, period, quarter, sue, verify_sources


def fact(end, profit, available=None, basis="ordinary"):
    return {"period_start": end[:4] + "-01-01", "period_end": end,
            "available_at": available or str(int(end[:4]) + 1) + "-04-01T23:59:59+08:00",
            "ordinary_profit_ytd": profit, "unit": "CNY", "basis": basis, "reviewed": True, "sources": [end]}


def proof(year, end=None, available=None):
    return {"start": f"{year}-01-01", "end": end or f"{year}-12-31", "shares": "100",
            "available_at": available or f"{year + 1}-04-01T23:59:59+08:00",
            "basis": "ordinary", "constant": True, "reviewed": True, "sources": [f"capital-{year}"]}


def quarters(count=12):
    first = quarter("2012-03-31")
    return {period(first + i): {"eps": str(i * i - 50), "basis": "ordinary", "shares": "100"}
            for i in range(count)}


class SueTest(unittest.TestCase):
    def test_rounding_trap_uses_profit_difference_and_evidence_availability(self):
        capital = proof(2014, "2014-06-30", "2014-08-26T23:59:59+08:00")
        capital["shares"] = "1796000000"
        ledger = {"cumulative": [fact("2014-03-31", "239853012.23", "2014-04-24T23:59:59+08:00"),
                                  fact("2014-06-30", "640502694.09", "2014-08-26T23:59:59+08:00")],
                  "capital_proofs": [capital]}
        result, issues, latest = normalize(ledger, "2014-09-30T18:00:00+08:00")
        second = result["2014-06-30"]
        self.assertEqual(second["profit"], "400649681.86")
        self.assertEqual(Decimal(second["eps"]).quantize(Decimal(".01")), Decimal(".22"))
        self.assertNotEqual(Decimal(second["eps"]), Decimal(".36") - Decimal(".13"))
        self.assertEqual(second["available_at"], capital["available_at"])
        self.assertEqual(latest, "2014-06-30")
        self.assertEqual(issues, {})
        earlier, issues, _ = normalize(ledger, "2014-06-30T18:00:00+08:00")
        self.assertEqual(earlier, {})
        self.assertEqual(issues["2014-03-31"], "unverified_constant_ordinary_shares")

    def test_future_revision_does_not_change_earlier_snapshot_and_order_is_irrelevant(self):
        original = fact("2012-03-31", "-100")
        correction = fact("2012-03-31", "-200", "2013-06-01T23:59:59+08:00")
        ledger = {"cumulative": [original], "capital_proofs": [proof(2012)]}
        expected = normalize(ledger, "2013-05-01T18:00:00+08:00")
        ledger["cumulative"].insert(0, correction)
        self.assertEqual(normalize(ledger, "2013-05-01T18:00:00+08:00"), expected)
        later = normalize(ledger, "2013-07-01T18:00:00+08:00")[0]
        self.assertEqual(later["2012-03-31"]["eps"], "-2")
        ledger["cumulative"].reverse()
        self.assertEqual(normalize(ledger, "2013-07-01T18:00:00+08:00")[0], later)

    def test_missing_quarter_basis_change_and_changed_capital_do_not_silently_subtract(self):
        ledger = {"cumulative": [fact("2012-06-30", "200")], "capital_proofs": [proof(2012)]}
        self.assertEqual(normalize(ledger, "2013-05-01T18:00:00+08:00")[1]["2012-06-30"], "missing_previous_ytd")
        ledger["cumulative"].append(fact("2012-03-31", "100", basis="restated"))
        self.assertEqual(normalize(ledger, "2013-05-01T18:00:00+08:00")[1]["2012-06-30"], "incomparable_profit_basis")
        ledger["cumulative"][1]["basis"] = "ordinary"
        ledger["capital_proofs"][0]["constant"] = False
        self.assertEqual(normalize(ledger, "2013-05-01T18:00:00+08:00")[0], {})
        ledger["capital_proofs"].append(proof(2012))
        ledger["capital_proofs"][0]["start"] = "2012-04-01"
        ledger["capital_proofs"][0]["end"] = "2012-06-30"
        self.assertEqual(normalize(ledger, "2013-05-01T18:00:00+08:00")[1]["2012-06-30"], "known_share_change")

    def test_innovation_uses_seasonal_difference_and_sample_sd_including_latest(self):
        result = sue(quarters(), "2014-12-31")
        # EPS=i²-50 gives eight annual innovations 16,24,...72. Sample variance=384.
        expected = Decimal(72) / Decimal(384).sqrt()
        self.assertLess(abs(Decimal(result["value"]) - expected), Decimal("1e-25"))
        self.assertEqual(result["observations"], 8)
        self.assertEqual([r["value"] for r in result["innovations"]], [str(n) for n in range(16, 73, 8)])

    def test_minimum_six_does_not_extend_calendar_window_or_fall_back_from_latest(self):
        data = quarters(16)
        result = sue(data, "2015-12-31")
        self.assertEqual(result["innovations"][0]["period_end"], "2014-03-31")
        for end in ("2012-03-31", "2012-06-30"):
            del data[end]
        self.assertEqual(sue(data, "2015-12-31")["observations"], 8)
        for end in ("2013-03-31", "2013-06-30"):
            del data[end]
        self.assertEqual(sue(data, "2015-12-31")["observations"], 6)
        del data["2013-09-30"]
        self.assertEqual(sue(data, "2015-12-31")["issue"], "fewer_than_six_innovations")
        del data["2015-12-31"]
        self.assertEqual(sue(data, "2015-12-31")["issue"], "latest_innovation_unavailable")

    def test_zero_variance_and_unadjusted_shares_are_explicit(self):
        data = quarters()
        for row in data.values():
            row["eps"] = "-1"
        self.assertEqual(sue(data, "2014-12-31")["issue"], "zero_innovation_variance")
        data = quarters()
        data["2014-12-31"]["shares"] = "200"
        self.assertEqual(sue(data, "2014-12-31")["innovations"][-1]["issue"], "unadjusted_share_change")

    def test_invalid_units_numbers_and_ambiguous_versions_fail(self):
        valid = {"cumulative": [fact("2012-03-31", "100")], "capital_proofs": [proof(2012)]}
        for key, value in (("unit", "CNY_10000"), ("ordinary_profit_ytd", "NaN"), ("reviewed", False)):
            ledger = copy.deepcopy(valid)
            ledger["cumulative"][0][key] = value
            with self.assertRaises(ValueError):
                normalize(ledger, "2013-05-01T18:00:00+08:00")
        valid["cumulative"] *= 2
        with self.assertRaises(ValueError):
            normalize(valid, "2013-05-01T18:00:00+08:00")

    def test_window_cannot_mix_accounting_bases_or_share_scales(self):
        data = quarters()
        # 每组同比对本身都一致，但跨组口径不同，仍不能合成一个标准差。
        for end in ("2012-03-31", "2013-03-31", "2014-03-31"):
            data[end]["basis"] = "new_accounting_basis"
        self.assertEqual(sue(data, "2014-12-31")["issue"], "incomparable_window_basis")
        data = quarters()
        for end in ("2012-03-31", "2013-03-31", "2014-03-31"):
            data[end]["shares"] = "200"
        self.assertEqual(sue(data, "2014-12-31")["issue"], "unadjusted_window_share_change")

    def test_archive_hash_and_latest_source_time_are_required(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "source.pdf"
            path.write_bytes(b"synthetic archive identity, not a parsed PDF")
            record = fact("2012-03-31", "100")
            source = {"document_path": "source.pdf", "sha256": digest(path), "pages": [1],
                      "source_uri": "https://example.test/report.pdf", "available_at": record["available_at"]}
            ledger = {"sources": {"2012-03-31": source}, "cumulative": [record], "capital_proofs": []}
            verify_sources(ledger, root)
            record["available_at"] = "2012-04-01T23:59:59+08:00"
            with self.assertRaises(ValueError):
                verify_sources(ledger, root)
            record["available_at"] = source["available_at"]
            path.write_bytes(b"changed")
            with self.assertRaises(ValueError):
                verify_sources(ledger, root)
