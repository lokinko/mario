"""股票池共用的按时点市值：选样日未复权收盘价 × 当时已公告的总股本。

科技池与红利池只在“候选从哪里来”上不同，市值口径、缓存布局与剔除原因在此统一。
"""

import json
from concurrent.futures import ThreadPoolExecutor
from decimal import Decimal
from urllib.parse import urlencode

from .baostock_market import mario_code
from .collect import retry, save_json
from .files import read_json, require, symbol
from .public_sources import download

EASTMONEY_SHARES = "https://datacenter.eastmoney.com/securities/api/data/v1/get"
MARKET_CAP_BASIS = "选样日未复权收盘价 × 变动日与公告日均不晚于选样日的最近一次总股本"


def stock_symbol(code):
    """来源只给 6 位代码；B 股与北交所不进入股票池。"""
    if code.startswith(("60", "68")):
        return f"{code}.SH"
    if code.startswith(("00", "30")):
        return f"{code}.SZ"
    return None


def listed_on(basic, selection_date):
    return bool(basic and basic["ipoDate"] and basic["ipoDate"] <= selection_date
                and not (basic["outDate"] and basic["outDate"] <= selection_date))


def shares_on(changes, selection_date):
    """只用当时已公告的股本：变动日和公告日都不得晚于选样日。"""
    known = [row for row in changes
             if row["END_DATE"][:10] <= selection_date and row["NOTICE_DATE"][:10] <= selection_date
             and row.get("TOTAL_SHARES")]
    return max(known, key=lambda row: (row["END_DATE"], row["NOTICE_DATE"])) if known else None


def candidate(stock, selection_date, basics, prices, shares):
    """返回 (候选行, None) 或 (None, 剔除原因)；未上市返回 not_listed_on_selection_date 由调用方决定是否记录。"""
    if not stock:
        return None, "not_a_share_sh_sz"
    basic = basics.get(stock)
    if not basic:
        return None, "not_covered_by_baostock"
    if not basic["ipoDate"] or basic["ipoDate"] > selection_date:
        return None, "not_listed_on_selection_date"
    if basic["outDate"] and basic["outDate"] <= selection_date:
        return None, "delisted_before_selection_date"
    price = prices.get((stock, selection_date))
    share = shares_on(shares.get(stock, []), selection_date)
    if not price or not price.get("close"):
        return None, "no_price_on_selection_date"
    if not share:
        return None, "no_announced_total_shares"
    total = Decimal(str(share["TOTAL_SHARES"]))
    return {
        "symbol": stock, "current_name": basic["code_name"],
        "market_cap": format(Decimal(price["close"]) * total, "f"),
        "close": price["close"], "trade_status": "trading" if price["tradestatus"] == "1" else "suspended",
        "total_shares": format(total, "f"), "shares_change_date": share["END_DATE"][:10],
        "shares_notice_date": share["NOTICE_DATE"][:10], "shares_change_reason": share.get("CHANGE_REASON"),
        "listed_on": basic["ipoDate"], "delisted_on": basic["outDate"] or None}, None


def ranked(candidates, top_n):
    order = sorted(candidates, key=lambda row: (-Decimal(row["market_cap"]), row["symbol"]))
    return [row["symbol"] for row in order[:top_n]]


def eastmoney_shares(code):
    rows, page = [], 1
    while True:
        query = urlencode({
            "reportName": "RPT_F10_EH_EQUITY", "source": "HSF10", "client": "PC",
            "columns": "SECUCODE,END_DATE,NOTICE_DATE,TOTAL_SHARES,LISTED_A_SHARES,CHANGE_REASON",
            "filter": f'(SECUCODE="{code}")', "pageNumber": page, "pageSize": 200,
            "sortTypes": 1, "sortColumns": "END_DATE"})
        payload = json.loads(download(f"{EASTMONEY_SHARES}?{query}"))
        result = payload.get("result") or {"pages": 0, "data": []}
        require(all(row["SECUCODE"] == code for row in result["data"]), "东方财富返回证券不符")
        rows += result["data"]
        if page >= result["pages"]:
            return rows
        page += 1


def cached(path, fetch):
    if path.exists():
        return read_json(path)
    value = fetch()
    path.parent.mkdir(parents=True, exist_ok=True)
    save_json(path, value)
    return value


def stock_basics(market, raw):
    return {mario_code(row["code"]): row for row in
            cached(raw / "baostock" / "stock_basic.json", market.stock_basics) if row["type"] == "1"}


def selection_dates(market, raw, years):
    """每个股票池年份的选样日为上一年最后一个交易日。"""
    output = {}
    for year in sorted(years):
        days = cached(raw / "baostock" / f"trade_dates-{year - 1}.json",
                      lambda: market.trade_dates(f"{year - 1}-12-01", f"{year - 1}-12-31"))
        opened = [row["calendar_date"] for row in days if row["is_trading_day"] == "1"]
        require(opened, f"{year - 1} 年 12 月没有交易日")
        output[year] = opened[-1]
    return output


def market_inputs(market, raw, wanted, workers):
    """wanted：{证券: {选样日}}；返回选样日未复权日线与东方财富总股本变动历史。"""
    prices = {}
    for stock, dates in sorted(wanted.items()):
        for selection_date in sorted(dates):
            rows = cached(raw / "baostock" / "daily" / stock / f"{selection_date}.json",
                          lambda: market.daily(stock, selection_date, selection_date))
            if rows:
                prices[stock, selection_date] = rows[0]

    def shares_for(stock):
        return stock, cached(raw / "eastmoney" / "shares" / f"{symbol(stock)}.json",
                             lambda: retry(lambda: eastmoney_shares(stock)))

    with ThreadPoolExecutor(max_workers=workers) as pool:
        shares = dict(pool.map(shares_for, sorted(wanted)))
    return prices, shares
