"""科技股池按时点选样：申万行业 × 已公告总股本 × 选样日未复权收盘价。

结果只是带出处的候选提案，`membership_verified` 保持 false；申万文件是事后维护的历史，
需要人工核对后才能进入正式发布。
"""

from datetime import date, datetime, timedelta

from .baostock_market import SOURCE as BAOSTOCK, Baostock
from .collect import retry, save_json
from .files import digest, read_json, require, write_json
from .market_cap import (EASTMONEY_SHARES, MARKET_CAP_BASIS, candidate, listed_on, market_inputs, ranked,
                         selection_dates, stock_basics, stock_symbol)
from .public_sources import SHANGHAI, download
from .xls import sheet_rows

SW_HISTORY = "https://www.swsresearch.com/swindex/pdf/SwClass2021/StockClassifyUse_stock.xls"
SW_2014_EFFECTIVE = "2014-02-21"
TECH_LEVEL1 = {
    "before_sw2014": {"27": "电子元器件", "32": "信息设备", "47": "信息服务"},
    "sw2014_or_later": {"27": "电子", "71": "计算机", "72": "传媒", "73": "通信"},
}
INDUSTRY_BASIS = "申万当前维护的个股行业变更历史，可能经事后修订，不等于当年首次公布的口径"


def excel_day(value):
    return (date(1899, 12, 30) + timedelta(days=int(value))).isoformat()


def sw_history(data):
    table = sheet_rows(data)
    require(table and table[0][:4] == ["股票代码", "计入日期", "行业代码", "更新日期"], "申万文件表头变化")
    history = {}
    for code, effective, industry, updated in (row[:4] for row in table[1:]):
        code = f"{int(code):06d}" if isinstance(code, float) else code
        history.setdefault(code, []).append(
            {"effective": excel_day(effective), "code": str(industry), "updated": excel_day(updated)})
    for records in history.values():
        records.sort(key=lambda row: row["effective"])
    return history


def industry_on(records, selection_date):
    current = [row for row in records if row["effective"] <= selection_date]
    return current[-1] if current else None


def tech_level1(record, selection_date):
    """行业版本按选样日判断：新版沿用未变的旧代码，旧版独有代码在新版下无效。"""
    version = "before_sw2014" if selection_date < SW_2014_EFFECTIVE else "sw2014_or_later"
    prefix = record["code"][:2]
    name = TECH_LEVEL1[version].get(prefix)
    if name:
        return {"version": version, "level1_code": prefix + "0000", "level1_name": name}
    if version == "sw2014_or_later" and prefix in TECH_LEVEL1["before_sw2014"].keys() - {"27"}:
        return {"version": version, "level1_code": prefix + "0000", "level1_name": None}
    return None


def select_year(year, selection_date, history, basics, prices, shares, top_n):
    candidates, excluded = [], []
    for code, records in sorted(history.items()):
        record = industry_on(records, selection_date)
        industry = record and tech_level1(record, selection_date)
        if not industry:
            continue
        stock = stock_symbol(code)
        if not industry["level1_name"]:
            if listed_on(basics.get(stock), selection_date):
                excluded.append({"symbol": stock, "reason": "sw_code_not_migrated_after_sw2014"})
            continue
        row, reason = candidate(stock, selection_date, basics, prices, shares)
        if reason:
            if reason != "not_listed_on_selection_date":
                excluded.append({"symbol": stock or code, "reason": reason})
            continue
        candidates.append({**row, "sw_industry_code": record["code"], "sw_effective": record["effective"],
                           "sw_updated": record["updated"], **industry})
    return {
        "pool": "tech", "year": year, "selection_date": selection_date,
        "available_at": f"{selection_date}T18:00:00+08:00", "membership_published_at": None,
        "membership_verified": False, "source_uri": SW_HISTORY,
        "industry_basis": INDUSTRY_BASIS, "market_cap_basis": MARKET_CAP_BASIS,
        "candidate_count": len(candidates), "candidates": candidates,
        "selected": ranked(candidates, top_n), "excluded": excluded}


def collect_tech_universe(root, years, top_n=20, workers=4, sw_file=None):
    require(years and all(2006 <= y <= 2100 for y in years), "科技池年份从 2006 起")
    require(1 <= workers <= 6, "并发数须为 1–6")
    root.mkdir(parents=True, exist_ok=True)
    request = {"pool": "tech", "years": sorted(years), "top_n": top_n, "sw_source": SW_HISTORY}
    if (root / "request.json").exists():
        require(read_json(root / "request.json") == request, "采集目录的范围已固定，改变范围请使用新目录")
    else:
        write_json(root / "request.json", request)
    raw = root / "raw"
    sw_path = raw / "sw" / "StockClassifyUse_stock.xls"
    if not sw_path.exists():
        sw_path.parent.mkdir(parents=True, exist_ok=True)
        # 申万站点只下发叶子证书，标准库无法补全中间证书；不关闭校验，改为导入本地下载的原件。
        content = sw_file.read_bytes() if sw_file else retry(lambda: download(SW_HISTORY))
        sw_history(content)
        sw_path.write_bytes(content)
        write_json(raw / "sw" / "meta.json", {
            "source_uri": SW_HISTORY, "bytes": len(content), "sha256": digest(sw_path),
            "acquisition": "imported_local_download" if sw_file else "direct_download",
            "recorded_at": datetime.now(SHANGHAI).isoformat()})
    history = sw_history(sw_path.read_bytes())
    with Baostock() as market:
        basics = stock_basics(market, raw)
        selections = selection_dates(market, raw, years)
        wanted = {}
        for selection_date in selections.values():
            for code, records in history.items():
                record = industry_on(records, selection_date)
                stock = stock_symbol(code)
                industry = record and tech_level1(record, selection_date)
                if industry and industry["level1_name"] and listed_on(basics.get(stock), selection_date):
                    wanted.setdefault(stock, set()).add(selection_date)
        prices, shares = market_inputs(market, raw, wanted, workers)
    universe = root / "universe"
    universe.mkdir(exist_ok=True)
    summary = {"pool": "tech", "top_n": top_n, "sw_sha256": digest(sw_path), "years": {}}
    for year, selection_date in sorted(selections.items()):
        snapshot = select_year(year, selection_date, history, basics, prices, shares, top_n)
        save_json(universe / f"tech-{year}.json", snapshot)
        reasons = {}
        for row in snapshot["excluded"]:
            reasons[row["reason"]] = reasons.get(row["reason"], 0) + 1
        summary["years"][str(year)] = {"selection_date": selection_date,
                                       "candidate_count": snapshot["candidate_count"],
                                       "excluded": reasons, "selected": snapshot["selected"]}
    summary["sources"] = {"industry": SW_HISTORY, "shares": EASTMONEY_SHARES, "prices": BAOSTOCK}
    summary["release_ready"] = False
    save_json(root / "coverage.json", summary)
    return summary
