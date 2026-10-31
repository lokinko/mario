import struct
import sys
import unittest
from decimal import Decimal
from unittest.mock import patch

from benchmark.baostock_market import Baostock, mario_code, provider_code
from benchmark.files import Invalid
from benchmark.market_cap import shares_on, stock_symbol
from benchmark.tech_universe import industry_on, select_year, tech_level1
from benchmark.xls import sheet_rows

FREE, END, FAT_SECTOR = 0xFFFFFFFF, 0xFFFFFFFE, 0xFFFFFFFD


def record(kind, body):
    return struct.pack("<HH", kind, len(body)) + body


def synthetic_workbook():
    """合成 BIFF8：第三个共享字符串跨 CONTINUE 记录，并从 UTF-16 切换为单字节。"""
    sst = struct.pack("<II", 3, 3)
    sst += struct.pack("<HB", 4, 1) + "股票代码".encode("utf-16-le")
    sst += struct.pack("<HB", 3, 0) + b"abc"
    sst += struct.pack("<HB", 4, 1) + "混合".encode("utf-16-le")
    stream = record(0x0809, struct.pack("<HH", 0x0600, 0x0005) + bytes(12))
    stream += record(0x00FC, sst) + record(0x003C, b"\x00ab") + record(0x000A, b"")
    stream += record(0x0809, struct.pack("<HH", 0x0600, 0x0010) + bytes(12))
    stream += record(0x00FD, struct.pack("<HHHI", 0, 0, 0, 0))
    stream += record(0x00FD, struct.pack("<HHHI", 0, 1, 0, 2))
    stream += record(0x0203, struct.pack("<HHHd", 1, 0, 0, 33331.0))
    stream += record(0x027E, struct.pack("<HHHI", 1, 1, 0, (42 << 2) | 2))
    stream += record(0x00BD, struct.pack("<HH", 2, 0) + struct.pack("<HI", 0, 0x3FF80000)
                     + struct.pack("<HI", 0, (1234 << 2) | 3) + struct.pack("<H", 1))
    stream += record(0x000A, b"")
    stream += record(0x0000, bytes(4096))
    sectors = -(-len(stream) // 512)
    fat = [FAT_SECTOR, END] + [3 + i for i in range(sectors - 1)] + [END]
    fat += [FREE] * (128 - len(fat))
    header = bytearray(512)
    header[:8] = bytes.fromhex("d0cf11e0a1b11ae1")
    struct.pack_into("<HHHHH", header, 24, 0x3E, 3, 0xFFFE, 9, 6)
    struct.pack_into("<IIIIIIII", header, 44, 1, 1, 0, 4096, END, 0, END, 0)
    struct.pack_into("<109I", header, 76, 0, *([FREE] * 108))

    def entry(name, kind, start, size):
        value = bytearray(128)
        encoded = (name + "\0").encode("utf-16-le")
        value[:len(encoded)] = encoded
        struct.pack_into("<HB", value, 64, len(encoded), kind)
        struct.pack_into("<II", value, 116, start, size)
        return bytes(value)

    directory = entry("Root Entry", 5, END, 0) + entry("Workbook", 2, 2, len(stream)) + bytes(256)
    body = stream + bytes(sectors * 512 - len(stream))
    return bytes(header) + struct.pack("<128I", *fat) + directory + body


def basic(code, ipo, out=""):
    return {"code_name": code, "ipoDate": ipo, "outDate": out, "type": "1"}


class XlsTest(unittest.TestCase):
    def test_reads_shared_strings_across_continue_and_numeric_cells(self):
        self.assertEqual(sheet_rows(synthetic_workbook()),
                         [["股票代码", "混合ab"], [33331.0, 42.0], [1.5, 12.34]])

    def test_rejects_non_ole_files(self):
        with self.assertRaises(Invalid):
            sheet_rows(b"PK\x03\x04 not an xls")


class TechUniverseTest(unittest.TestCase):
    def test_industry_uses_record_effective_on_selection_date_and_maps_sw_versions(self):
        records = [{"effective": "1997-06-12", "code": "320101", "updated": "2015-10-27"},
                   {"effective": "2014-02-21", "code": "730202", "updated": "2024-09-27"},
                   {"effective": "2021-07-30", "code": "480301", "updated": "2021-07-31"}]
        self.assertEqual(tech_level1(industry_on(records, "2007-12-28"), "2007-12-28")["level1_name"], "信息设备")
        self.assertEqual(tech_level1(industry_on(records, "2014-12-31"), "2014-12-31")["level1_name"], "通信")
        self.assertIsNone(tech_level1(industry_on(records, "2021-12-31"), "2021-12-31"))
        self.assertIsNone(industry_on(records, "1990-01-01"))
        unchanged = {"effective": "2010-01-01", "code": "270502", "updated": ""}
        self.assertEqual(tech_level1(unchanged, "2020-12-31")["level1_name"], "电子")
        self.assertEqual(tech_level1(unchanged, "2013-12-31")["level1_name"], "电子元器件")
        stale = tech_level1({"effective": "2010-01-01", "code": "470301", "updated": ""}, "2020-12-31")
        self.assertIsNone(stale["level1_name"])

    def test_shares_ignore_changes_announced_after_selection(self):
        changes = [{"END_DATE": "2007-06-01 00:00:00", "NOTICE_DATE": "2007-05-28 00:00:00", "TOTAL_SHARES": 100},
                   {"END_DATE": "2007-12-20 00:00:00", "NOTICE_DATE": "2008-01-10 00:00:00", "TOTAL_SHARES": 200},
                   {"END_DATE": "2008-01-05 00:00:00", "NOTICE_DATE": "2007-12-01 00:00:00", "TOTAL_SHARES": 300}]
        self.assertEqual(shares_on(changes, "2007-12-28")["TOTAL_SHARES"], 100)
        self.assertIsNone(shares_on(changes, "2007-05-01"))

    def test_selection_keeps_later_delisted_stocks_and_records_exclusions(self):
        tech = [{"effective": "2010-01-01", "code": "270101", "updated": "2015-10-27"}]
        history = {"300104": tech, "000021": tech, "600036": [{"effective": "2010-01-01", "code": "440101",
                                                              "updated": ""}],
                   "200002": tech, "000555": tech, "000666": tech, "000777": tech, "300999": tech,
                   "600804": [{"effective": "2007-01-01", "code": "470301", "updated": "2015-10-27"}]}
        basics = {"300104.SZ": basic("乐视网", "2010-08-12", "2020-07-21"),
                  "000021.SZ": basic("深科技", "1994-02-02"), "600036.SH": basic("招商银行", "2002-04-09"),
                  "000555.SZ": basic("已退", "2000-01-01", "2014-06-01"),
                  "000666.SZ": basic("无股本", "2000-01-01"), "000777.SZ": basic("无行情", "2000-01-01"),
                  "300999.SZ": basic("未上市", "2015-06-01"), "600804.SH": basic("旧代码", "2000-01-01")}
        day = "2014-12-31"
        price = {"close": "10.00", "tradestatus": "1"}
        prices = {("300104.SZ", day): {"close": "58.80", "tradestatus": "0"}, ("000021.SZ", day): price,
                  ("600036.SH", day): price, ("000666.SZ", day): price}
        share = [{"END_DATE": "2014-01-01 00:00:00", "NOTICE_DATE": "2013-12-30 00:00:00", "TOTAL_SHARES": 1000}]
        shares = {"300104.SZ": share, "000021.SZ": [{**share[0], "TOTAL_SHARES": 2000}],
                  "000777.SZ": share, "600036.SH": share}
        snapshot = select_year(2015, day, history, basics, prices, shares, top_n=1)
        self.assertEqual([c["symbol"] for c in snapshot["candidates"]], ["000021.SZ", "300104.SZ"])
        leshi = snapshot["candidates"][1]
        self.assertEqual((leshi["trade_status"], leshi["delisted_on"]), ("suspended", "2020-07-21"))
        self.assertEqual(Decimal(leshi["market_cap"]), Decimal("58800"))
        self.assertEqual(snapshot["selected"], ["300104.SZ"])
        self.assertEqual({row["symbol"]: row["reason"] for row in snapshot["excluded"]},
                         {"000555.SZ": "delisted_before_selection_date", "000666.SZ": "no_announced_total_shares",
                          "000777.SZ": "no_price_on_selection_date", "200002": "not_a_share_sh_sz",
                          "600804.SH": "sw_code_not_migrated_after_sw2014"})
        self.assertFalse(snapshot["membership_verified"])
        self.assertEqual(snapshot["candidate_count"], 2)

    def test_symbol_mapping_excludes_b_shares_and_other_markets(self):
        self.assertEqual([stock_symbol(c) for c in ("600036", "688981", "000063", "300104", "200002", "830799")],
                         ["600036.SH", "688981.SH", "000063.SZ", "300104.SZ", None, None])
        self.assertEqual((provider_code("600036.SH"), mario_code("sz.300104")), ("sh.600036", "300104.SZ"))
        with self.assertRaises(Invalid):
            provider_code("830799.BJ")

    def test_missing_optional_dependency_is_reported_without_traceback(self):
        with patch.dict(sys.modules, {"baostock": None}):
            with self.assertRaises(Invalid):
                with Baostock():
                    pass


if __name__ == "__main__":
    unittest.main()
