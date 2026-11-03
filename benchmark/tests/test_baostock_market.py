import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch

from benchmark.baostock_market import Baostock
from benchmark.files import Invalid


def result(error="0", fail_page=False):
    value = SimpleNamespace(error_code=error, error_msg="provider error", fields=["close"])
    calls = iter([True, False])

    def next_row():
        available = next(calls)
        if not available and fail_page:
            value.error_code = "connection_lost"
        return available

    value.next = next_row
    value.get_row_data = lambda: ["10"]
    return value


class BaostockSessionTest(unittest.TestCase):
    def test_relogin_and_restart_discards_failed_partial_page(self):
        for first in (result("not_logged_in"), result(fail_page=True)):
            with self.subTest(first=first), patch("benchmark.baostock_market.time.sleep"):
                market = Baostock()
                market.api = SimpleNamespace(login=Mock(return_value=result()), logout=Mock(),
                                             query_stock_basic=Mock(side_effect=[first, result()]))
                self.assertEqual(market.stock_basics(), [{"close": "10"}])
                self.assertEqual(market.api.query_stock_basic.call_count, 2)
                market.api.login.assert_called_once()
                market.api.logout.assert_called_once()

    def test_persistent_failure_is_bounded_and_does_not_return_partial_rows(self):
        with patch("benchmark.baostock_market.time.sleep"):
            market = Baostock()
            market.api = SimpleNamespace(login=Mock(return_value=result()), logout=Mock(),
                                         query_stock_basic=Mock(side_effect=lambda: result(fail_page=True)))
            with self.assertRaisesRegex(Invalid, "分页失败"):
                market.stock_basics()
            self.assertEqual(market.api.query_stock_basic.call_count, 3)

    def test_failed_relogin_does_not_query_unauthenticated(self):
        with patch("benchmark.baostock_market.time.sleep"):
            market = Baostock()
            market.api = SimpleNamespace(login=Mock(return_value=result("login_failed")), logout=Mock(),
                                         query_stock_basic=Mock(return_value=result("not_logged_in")))
            with self.assertRaisesRegex(Invalid, "登录失败"):
                market.stock_basics()
            self.assertEqual(market.api.query_stock_basic.call_count, 1)
