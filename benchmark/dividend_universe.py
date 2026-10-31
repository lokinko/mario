"""红利股池按时点选样：选样日已生效的中证红利成分 × 已公告总股本 × 选样日未复权收盘价。

免费来源没有中证红利历史成分表，年末成分按以下顺序重建，结果保持 `membership_verified=false`：
1. 2010 年起取被动指数基金（大成中证红利 A）年报的全部持仓作为成分代理，剔除低权重与上市不满一年的打新残留；
2. 有官方调样名单的年份，按生效日相对选样日修正：已生效则调出移除、调入补齐，1 月生效则反向；
3. 基金成立前的年末，从下一年末的成分按官方名单倒推一次调样。
指数 2008-05-26 才发布，2007 年末不存在成分，因此红利池从 2009 年开始。
"""

import html
import json
import re
from datetime import date, datetime, timedelta
from decimal import Decimal

from .baostock_market import SOURCE as BAOSTOCK, Baostock
from .collect import retry, save_json
from .files import read_json, require, write_json
from .market_cap import (EASTMONEY_SHARES, MARKET_CAP_BASIS, cached, candidate, listed_on, market_inputs,
                         ranked, selection_dates, stock_basics, stock_symbol)
from .public_sources import SHANGHAI, download

CSI_ANNOUNCEMENT = "https://www.csindex.com.cn/csindex-home/announcement/queryAnnouncementById?id="
# 键为调样公告所在年份（当年 12 月公布）；2015–2017、2021 年名单只在链接表格中，2022 年起官网不再公告红利指数名单。
ADJUSTMENTS = {2008: 892, 2009: 4744, 2010: 6150, 2011: 6687, 2012: 3242, 2013: 6750, 2014: 3596,
               2018: 11859, 2019: 11529, 2020: 11514}
PROXY_FUND = "090010"
FUND_HOLDINGS = "https://fundf10.eastmoney.com/FundArchivesDatas.aspx"
FIRST_FUND_YEAR = 2010
FIRST_POOL_YEAR = 2009
MIN_FUND_WEIGHT = Decimal("0.05")
MEMBERSHIP_BASIS = ("中证红利（000922）选样日已生效成分的重建：大成中证红利 A（090010）年报全部持仓"
                    "（占净值不低于 0.05%，剔除上市不满一年的打新股）为代理，按中证官方调样公告修正生效时点；未经逐项核对")


def plain_text(markup):
    return re.sub(r"\s+", " ", html.unescape(re.sub(r"<[^>]+>", " ", markup)))


def parse_adjustment(payload):
    data = payload["data"]
    text = plain_text(data.get("content") or "")
    when = re.search(r"(?:决定于|生效日期)\s*(\d{4})\s*年\s*(\d{1,2})\s*月\s*(?:(\d{1,2})\s*日|第一个交易日)", text)
    require(when, f"公告 {data['id']} 缺少生效日期")
    year, month, day = int(when.group(1)), int(when.group(2)), int(when.group(3) or 1)
    marker = "中证红利指数样本股调整名单"
    if marker in text:
        segment = text[text.index(marker) + len(marker):]
        end = re.search(r"\S*指数样本(?:股)?调整名单|备选名单", segment)
        segment = segment[:end.start()] if end else segment
    else:
        require("中证红利" in data["title"] and "调整名单" in data["title"], f"公告 {data['id']} 不含中证红利名单")
        segment = text
    codes = re.findall(r"(?<!\d)\d{6}(?!\d)", segment)
    require(codes and len(codes) % 2 == 0, f"公告 {data['id']} 调出与调入名单不成对")
    removed, added = codes[0::2], codes[1::2]
    stated = re.search(r"中证红利指数更换\s*(\d+)\s*只", text)
    require(not stated or int(stated.group(1)) == len(removed), f"公告 {data['id']} 名单数与正文不符")
    require(not set(removed) & set(added), f"公告 {data['id']} 调出与调入重叠")
    return {"announcement_id": data["id"], "published_on": str(data["publishDate"])[:10],
            "effective_on": f"{year}-{month:02d}-{day:02d}",
            "effective_rule": "当月第一个交易日" if when.group(3) is None else "指定日期",
            "removed": removed, "added": added}


def fund_holdings(markup, year):
    """返回年报 {year}-12-31 全部持仓 [(代码, 占净值百分比)]；季报只有前十，不可替代。"""
    sections = [part for part in re.split(r"<h4", markup)[1:] if f"{year}年4季度股票投资明细" in part]
    require(sections, f"{PROXY_FUND} 缺少 {year} 年末持仓")
    output = []
    for row in re.findall(r"<tr>(.*?)</tr>", sections[0], re.S):
        cells = [plain_text(cell).strip() for cell in re.findall(r"<td[^>]*>(.*?)</td>", row, re.S)]
        if len(cells) >= 5 and re.fullmatch(r"\d{6}", cells[1]):
            output.append((cells[1], Decimal(cells[4].rstrip("%").replace(",", "") or "0")))
    require(len(output) > 20, f"{PROXY_FUND} {year} 年末持仓不完整，可能只是季报前十")
    return output


def year_end_members(selections, funds, adjustments):
    """selections：{股票池年份: 选样日}；返回 {年末年份: {6 位代码: 来源}}，年末年份 = 股票池年份 − 1。"""
    output = {}
    for end in sorted((year - 1 for year in selections), reverse=True):
        cutoff, adjustment = selections[end + 1], adjustments.get(end)
        if end in funds:
            members = {code: f"fund_{PROXY_FUND}_{end}" for code, weight in funds[end] if weight >= MIN_FUND_WEIGHT}
            if adjustment:
                live = adjustment["effective_on"] <= cutoff
                leaving, joining = ((adjustment["removed"], adjustment["added"]) if live
                                    else (adjustment["added"], adjustment["removed"]))
                for code in leaving:
                    members.pop(code, None)
                for code in joining:
                    members.setdefault(code, f"csi_{adjustment['announcement_id']}")
        else:
            later, later_adjustment = output.get(end + 1), adjustments.get(end + 1)
            require(later is not None and adjustment and adjustment["effective_on"] > cutoff,
                    f"{end} 年末无基金持仓，需要下一年末成分和次年初生效的官方调样名单倒推")
            require(not later_adjustment or later_adjustment["effective_on"] > selections[end + 2],
                    f"{end + 1} 年末成分已含下一次调样，无法倒推 {end} 年末")
            members = {code: source for code, source in later.items() if code not in adjustment["added"]}
            for code in adjustment["removed"]:
                members[code] = f"csi_{adjustment['announcement_id']}"
        output[end] = members
    return output


def adjustment_check(funds, adjustments, selections):
    """官方名单与基金代理的一致性：已生效调样的调入应在持仓中、调出不应在。"""
    output = {}
    for end, adjustment in sorted(adjustments.items()):
        if end not in funds or end + 1 not in selections:
            continue
        held = {code for code, weight in funds[end] if weight >= MIN_FUND_WEIGHT}
        output[str(end)] = {"effective_on": adjustment["effective_on"],
                            "effective_by_selection": adjustment["effective_on"] <= selections[end + 1],
                            "added_held": sum(code in held for code in adjustment["added"]),
                            "removed_held": sum(code in held for code in adjustment["removed"]),
                            "size": len(adjustment["added"])}
    return output


def one_year_before(day):
    return (date.fromisoformat(day) - timedelta(days=365)).isoformat()


def select_year(year, selection_date, members, basics, prices, shares, top_n):
    """指数要求连续多年现金分红，基金持有的上市不满一年新股只可能来自打新配售。"""
    candidates, excluded = [], []
    for code, source in sorted(members.items()):
        stock = stock_symbol(code)
        row, reason = candidate(stock, selection_date, basics, prices, shares)
        if row and source.startswith("fund_") and row["listed_on"] > one_year_before(selection_date):
            row, reason = None, "fund_new_share_listed_under_one_year"
        if reason:
            excluded.append({"symbol": stock or code, "reason": reason, "membership_source": source})
            continue
        candidates.append({**row, "membership_source": source})
    return {
        "pool": "dividend", "year": year, "selection_date": selection_date,
        "available_at": f"{selection_date}T18:00:00+08:00", "membership_published_at": None,
        "membership_verified": False, "source_uri": f"{FUND_HOLDINGS}?type=jjcc&code={PROXY_FUND}",
        "membership_basis": MEMBERSHIP_BASIS, "market_cap_basis": MARKET_CAP_BASIS,
        "member_count": len(members), "candidate_count": len(candidates), "candidates": candidates,
        "selected": ranked(candidates, top_n), "excluded": excluded}


def fund_page(raw, year):
    path = raw / "eastmoney" / f"fund-{PROXY_FUND}" / f"{year}.html"
    if not path.exists():
        url = f"{FUND_HOLDINGS}?type=jjcc&code={PROXY_FUND}&topline=300&year={year}&month=12"
        content = retry(lambda: download(url, referer="https://fundf10.eastmoney.com/"))
        fund_holdings(content.decode("utf-8"), year)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content)
    return path.read_text(encoding="utf-8")


def collect_dividend_universe(root, years, top_n=20, workers=4):
    require(years and all(FIRST_POOL_YEAR <= y <= 2100 for y in years), "红利池年份从 2009 起（指数 2008-05 发布）")
    require(1 <= workers <= 6, "并发数须为 1–6")
    root.mkdir(parents=True, exist_ok=True)
    request = {"pool": "dividend", "years": sorted(years), "top_n": top_n, "index": "000922.CSI",
               "proxy_fund": PROXY_FUND, "adjustments": ADJUSTMENTS}
    if (root / "request.json").exists():
        require(read_json(root / "request.json") == json.loads(json.dumps(request)),
                "采集目录的范围已固定，改变范围请使用新目录")
    else:
        write_json(root / "request.json", request)
    raw = root / "raw"
    adjustments = {}
    for end, notice in ADJUSTMENTS.items():
        payload = cached(raw / "csi" / f"announcement-{notice}.json", lambda: retry(
            lambda: json.loads(download(f"{CSI_ANNOUNCEMENT}{notice}", referer="https://www.csindex.com.cn/"))))
        adjustments[end] = parse_adjustment(payload)
    pool_years = range(min(years), max(max(years), FIRST_FUND_YEAR + 1) + 1)
    funds = {end: fund_holdings(fund_page(raw, end), end)
             for end in range(max(pool_years.start - 1, FIRST_FUND_YEAR), pool_years.stop - 1)}
    with Baostock() as market:
        basics = stock_basics(market, raw)
        selections = selection_dates(market, raw, pool_years)
        members = year_end_members(selections, funds, adjustments)
        wanted = {}
        for year in years:
            for code in members[year - 1]:
                stock = stock_symbol(code)
                if listed_on(basics.get(stock), selections[year]):
                    wanted.setdefault(stock, set()).add(selections[year])
        prices, shares = market_inputs(market, raw, wanted, workers)
    universe = root / "universe"
    universe.mkdir(exist_ok=True)
    summary = {"pool": "dividend", "top_n": top_n, "years": {},
               "adjustment_check": adjustment_check(funds, adjustments, selections)}
    for year in sorted(years):
        snapshot = select_year(year, selections[year], members[year - 1], basics, prices, shares, top_n)
        save_json(universe / f"dividend-{year}.json", snapshot)
        reasons, sources = {}, {}
        for row in snapshot["excluded"]:
            reasons[row["reason"]] = reasons.get(row["reason"], 0) + 1
        for source in members[year - 1].values():
            kind = source.rsplit("_", 1)[0] if source.startswith("fund_") else "csi_adjustment"
            sources[kind] = sources.get(kind, 0) + 1
        summary["years"][str(year)] = {"selection_date": selections[year], "member_count": snapshot["member_count"],
                                       "member_sources": sources, "candidate_count": snapshot["candidate_count"],
                                       "excluded": reasons, "selected": snapshot["selected"]}
    summary["sources"] = {"membership": [FUND_HOLDINGS, CSI_ANNOUNCEMENT], "shares": EASTMONEY_SHARES,
                          "prices": BAOSTOCK}
    summary["recorded_at"] = datetime.now(SHANGHAI).isoformat()
    summary["release_ready"] = False
    save_json(root / "coverage.json", summary)
    return summary
