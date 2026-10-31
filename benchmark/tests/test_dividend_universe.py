import unittest
from decimal import Decimal

from benchmark.dividend_universe import adjustment_check, fund_holdings, parse_adjustment, select_year, year_end_members
from benchmark.files import Invalid


def notice(content, title="关于调整沪深300和中证香港100等指数样本股的公告", ident=1):
    return {"data": {"id": ident, "title": title, "publishDate": "2012-12-17", "content": content}}


def table(pairs):
    return "".join(f"<tr><td>{a}</td><td>甲</td><td>{b}</td><td>乙</td></tr>" for a, b in pairs)


def holdings(year, rows):
    body = "".join(f"<tr><td>{i}</td><td>{code}</td><td>名</td><td>股吧</td><td>{w}%</td></tr>"
                   for i, (code, w) in enumerate(rows, 1))
    return f"<h4 class='t'>大成中证红利指数A {year}年4季度股票投资明细</h4><table>{body}</table>"


class AdjustmentTest(unittest.TestCase):
    def test_reads_dividend_section_between_other_index_tables(self):
        content = ("决定于 2013 年 1 月第一个交易日调整沪深300、中证红利等指数样本股，中证红利指数更换 2 只样本股。"
                   f"沪深300指数样本股调整名单 {table([('600000', '600001')])}"
                   f"中证红利指数样本股调整名单 {table([('000001', '600002'), ('000002', '600003')])}"
                   f"中证香港100指数样本股调整名单 {table([('000003', '600004')])}")
        result = parse_adjustment(notice(content))
        self.assertEqual(result["removed"], ["000001", "000002"])
        self.assertEqual(result["added"], ["600002", "600003"])
        self.assertEqual((result["effective_on"], result["effective_rule"]), ("2013-01-01", "当月第一个交易日"))

    def test_standalone_list_and_dated_effective(self):
        result = parse_adjustment(notice(f"（生效日期2013年12月16日）{table([('000001', '600002')])}",
                                         title="中证红利指数样本股调整名单"))
        self.assertEqual((result["effective_on"], result["effective_rule"]), ("2013-12-16", "指定日期"))

    def test_rejects_count_mismatch_and_unrelated_notice(self):
        with self.assertRaises(Invalid):
            parse_adjustment(notice("决定于2013年12月16日调整，中证红利指数更换 3 只股票。"
                                    f"中证红利指数样本股调整名单 {table([('000001', '600002')])}"))
        with self.assertRaises(Invalid):
            parse_adjustment(notice(f"决定于2013年12月16日调整。{table([('000001', '600002')])}"))


class MembershipTest(unittest.TestCase):
    def test_fund_holdings_need_full_annual_list(self):
        rows = [(f"600{i:03d}", "1.00") for i in range(30)] + [("688999", "0.00")]
        parsed = fund_holdings(holdings(2012, rows), 2012)
        self.assertEqual(parsed[-1], ("688999", Decimal("0.00")))
        with self.assertRaises(Invalid):
            fund_holdings(holdings(2012, rows[:10]), 2012)
        with self.assertRaises(Invalid):
            fund_holdings(holdings(2011, rows), 2012)

    def test_effective_timing_corrects_proxy_and_back_derives_before_fund(self):
        selections = {2010: "2009-12-31", 2011: "2010-12-31", 2012: "2011-12-30"}
        funds = {2010: [("000001", Decimal("1")), ("000004", Decimal("1")), ("688001", Decimal("0.01"))],
                 2011: [("000001", Decimal("1")), ("000004", Decimal("1"))]}
        adjustments = {
            2009: {"announcement_id": 9, "effective_on": "2010-01-01", "removed": ["000002"], "added": ["000003"]},
            2010: {"announcement_id": 10, "effective_on": "2011-01-01", "removed": ["000003"], "added": ["000004"]},
            2011: {"announcement_id": 11, "effective_on": "2011-12-12", "removed": ["000001"], "added": ["000005"]}}
        members = year_end_members(selections, funds, adjustments)
        self.assertEqual(set(members[2011]), {"000004", "000005"})
        self.assertEqual(members[2010], {"000001": "fund_090010_2010", "000003": "csi_10"})
        self.assertEqual(members[2009], {"000001": "fund_090010_2010", "000002": "csi_9"})
        check = adjustment_check(funds, adjustments, selections)
        self.assertEqual((check["2010"]["added_held"], check["2010"]["removed_held"]), (1, 0))
        self.assertTrue(check["2011"]["effective_by_selection"])

    def test_cannot_back_derive_without_official_list(self):
        with self.assertRaises(Invalid):
            year_end_members({2010: "2009-12-31", 2011: "2010-12-31"},
                             {2010: [("000001", Decimal("1"))]}, {})

    def test_selects_by_market_cap_and_keeps_membership_source(self):
        basics = {code: {"code_name": "现名", "ipoDate": listed, "outDate": ""}
                  for code, listed in (("000001.SZ", "2000-01-01"), ("600002.SH", "2000-01-01"),
                                       ("601003.SH", "2012-06-01"))}
        prices = {(code, "2012-12-31"): {"close": close, "tradestatus": "1"}
                  for code, close in (("000001.SZ", "10"), ("600002.SH", "5"), ("601003.SH", "99"))}
        shares = {code: [{"END_DATE": "2012-01-01 00:00:00", "NOTICE_DATE": "2012-01-02 00:00:00",
                          "TOTAL_SHARES": 100}] for code in basics}
        members = {"000001": "fund_090010_2012", "600002": "csi_3242", "830001": "fund_090010_2012",
                   "601003": "fund_090010_2012"}
        snapshot = select_year(2013, "2012-12-31", members, basics, prices, shares, 1)
        self.assertEqual(snapshot["selected"], ["000001.SZ"])
        self.assertEqual([(row["symbol"], row["reason"]) for row in snapshot["excluded"]],
                         [("601003.SH", "fund_new_share_listed_under_one_year"), ("830001", "not_a_share_sh_sz")])
        self.assertFalse(snapshot["membership_verified"])


if __name__ == "__main__":
    unittest.main()
