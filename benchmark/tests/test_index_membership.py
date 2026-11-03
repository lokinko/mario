import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from benchmark.files import Invalid, read_json
from benchmark.index_membership import (INDICES, collect_memberships, membership_selection,
                                         parse_membership)


def workbook_rows(index, offset=0):
    header = ["日期Date", "指数代码 Index Code", "指数名称 Index Name", "英文名称",
              "成份券代码Constituent Code", "成份券名称Constituent Name", "英文名称",
              "交易所Exchange", "交易所英文名称"]
    return [header] + [["20261102", index, INDICES[index], "Index", f"{600000 + offset + i:06d}",
                        f"证券{i}", "Stock", "上海证券交易所", "Shanghai Stock Exchange"] for i in range(50)]


def workbook_content(index):
    return json.dumps(workbook_rows(index, 25 if index == "000016" else 0)).encode()


class IndexMembershipTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        patcher = patch("benchmark.index_membership.sheet_rows", side_effect=json.loads)
        patcher.start()
        self.addCleanup(patcher.stop)
        patcher = patch("benchmark.index_membership.download",
                        side_effect=lambda url, **_: workbook_content(url.rsplit("/", 1)[-1][:6]))
        patcher.start()
        self.addCleanup(patcher.stop)

    def test_full_memberships_overlap_dedup_and_repeat_is_stable(self):
        result = collect_memberships(self.root, "2026-11-03")
        self.assertEqual({k: v["members"] for k, v in result.items()}, {"930050": 50, "000016": 50})
        codes, summary = membership_selection(self.root)
        self.assertEqual(len(codes), 75)
        self.assertEqual(summary["930050"]["current_members_union"], 50)
        before = sorted(p.relative_to(self.root) for p in self.root.rglob("*") if p.is_file())
        collect_memberships(self.root, "2026-11-03")
        self.assertEqual(before, sorted(p.relative_to(self.root) for p in self.root.rglob("*") if p.is_file()))
        snapshot = read_json(next((self.root / "index-membership/930050").glob("*.json")))
        self.assertFalse(snapshot["historical_membership_verified"])
        self.assertEqual(snapshot["observed_on"], "2026-11-03")
        self.assertEqual(snapshot["source_date"], "2026-11-02")

    def test_rejects_wrong_index_truncated_duplicate_and_mixed_date_tables(self):
        for kind in ("wrong_index", "truncated", "duplicate", "mixed_date", "exchange", "header"):
            with self.subTest(kind=kind):
                table = workbook_rows("930050")
                if kind == "wrong_index":
                    table[1][1] = "000016"
                elif kind == "truncated":
                    table.pop()
                elif kind == "duplicate":
                    table[-1] = table[1]
                elif kind == "mixed_date":
                    table[1][0] = "20261101"
                elif kind == "exchange":
                    table[1][7] = "未知市场"
                else:
                    table[0][4] = "changed"
                with self.assertRaises(Invalid):
                    parse_membership(json.dumps(table).encode(), "930050")

    def test_new_observation_retains_previous_members_and_detects_tampering(self):
        collect_memberships(self.root, "2026-11-03")
        with patch("benchmark.index_membership.download",
                   side_effect=lambda url, **_: json.dumps(workbook_rows(url.rsplit("/", 1)[-1][:6], 100)).encode()):
            collect_memberships(self.root, "2026-11-04")
        codes, summary = membership_selection(self.root)
        self.assertIn("600000.SH", codes)
        self.assertIn("600100.SH", codes)
        self.assertEqual(summary["930050"]["snapshots"], 2)
        self.assertEqual(summary["930050"]["observed_on"], "2026-11-04")
        next((self.root / "index-membership/930050").glob("*.xls")).write_bytes(b"changed")
        with self.assertRaisesRegex(Invalid, "哈希"):
            membership_selection(self.root)


if __name__ == "__main__":
    unittest.main()
