"""Baostock 免费行情适配；本包中唯一依赖第三方库的模块，只在调用时导入。"""

import io
import time
from contextlib import redirect_stdout

from .files import Invalid, require, symbol

SOURCE = "https://www.baostock.com/"


def provider_code(code):
    symbol(code)
    number, market = code.split(".")
    require(market in ("SH", "SZ"), f"Baostock 不覆盖该市场: {code}")
    return f"{market.lower()}.{number}"


def mario_code(code):
    market, number = code.split(".")
    return f"{number}.{market.upper()}"


class Baostock:
    """登录会话不可并发使用；查询结果以字符串原样返回，不做口径转换。"""

    def __enter__(self):
        try:
            import baostock
        except ImportError as exc:
            raise Invalid("缺少 baostock：执行 uv sync --group market 后重试") from exc
        self.api = baostock
        self._login()
        return self

    def _login(self):
        with redirect_stdout(io.StringIO()):
            result = self.api.login()
        require(result.error_code == "0", f"Baostock 登录失败: {result.error_msg}")

    def __exit__(self, *_):
        with redirect_stdout(io.StringIO()):
            self.api.logout()

    def _rows(self, result):
        require(result.error_code == "0", f"Baostock 查询失败: {result.error_msg}")
        output = []
        while result.next():
            output.append(dict(zip(result.fields, result.get_row_data())))
        require(result.error_code == "0", f"Baostock 分页失败: {result.error_msg}")
        return output

    def _query(self, operation):
        for attempt in range(3):
            try:
                if attempt:
                    self._login()
                return self._rows(operation())
            except (OSError, Invalid):
                if attempt == 2:
                    raise
                # SDK 会话失效或分页连接中断时，丢弃部分结果并从头重查。
                with redirect_stdout(io.StringIO()):
                    try:
                        self.api.logout()
                    except OSError:
                        pass
                time.sleep(1 + 2 * attempt)

    def trade_dates(self, start, end):
        return self._query(lambda: self.api.query_trade_dates(start_date=start, end_date=end))

    def stock_basics(self):
        return self._query(lambda: self.api.query_stock_basic())

    def daily(self, code, start, end, adjustflag="3", index=False):
        """adjustflag：3 未复权，1 后复权（过去价格只依赖当时已发生的权益事件）。"""
        require(adjustflag in ("1", "3"), "只支持未复权或后复权")
        fields = "date,code,open,high,low,close,preclose,volume,amount"
        if not index:
            fields += ",tradestatus,isST"
        return self._query(lambda: self.api.query_history_k_data_plus(
            provider_code(code), fields, start_date=start, end_date=end,
            frequency="d", adjustflag=adjustflag))
