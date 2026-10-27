"""免费来源覆盖探测：归档真实响应，绝不把当前成分当历史成分。"""

import json
import re
from datetime import datetime, timedelta, timezone
from decimal import Decimal
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode
from urllib.request import Request, urlopen

from .files import digest, new_directory, read_json, require, symbol, write_json, write_rows

SHANGHAI = timezone(timedelta(hours=8))
CSI_CONSTITUENTS = "https://oss-ch.csindex.com.cn/static/html/csindex/public/uploads/file/autofile/cons/000922cons.xls"
CNINFO_STOCKS = "https://www.cninfo.com.cn/new/data/szse_stock.json"
CNINFO_QUERY = "https://www.cninfo.com.cn/new/hisAnnouncement/query"


def download(url, body=None):
    request = Request(url, data=urlencode(body).encode() if body else None,
                      headers={"User-Agent": "Mozilla/5.0", "Referer": "https://www.cninfo.com.cn/"})
    with urlopen(request, timeout=40) as response:
        result = response.read(100 * 1024 * 1024 + 1)
    require(len(result) <= 100 * 1024 * 1024, "公开文件超过大小上限")
    return result


def yahoo_history(code, start, end):
    symbol(code)
    ticker = code.replace(".SH", ".SS")
    first = datetime.fromisoformat(start).replace(tzinfo=SHANGHAI)
    last = datetime.fromisoformat(end).replace(tzinfo=SHANGHAI) + timedelta(days=1)
    query = urlencode({"period1": int(first.timestamp()), "period2": int(last.timestamp()),
                       "interval": "1d", "events": "div,splits"})
    for host in ("query1.finance.yahoo.com", "query2.finance.yahoo.com"):
        try:
            url = f"https://{host}/v8/finance/chart/{ticker}?{query}"
            payload = json.loads(download(url), parse_float=str)
            result = payload["chart"]["result"][0]
            require(result["meta"]["symbol"] == ticker, "供应商返回证券不符")
            return url, payload, result
        except (HTTPError, URLError, TimeoutError, ValueError, KeyError, TypeError):
            continue
    raise ValueError(f"{code} 免费日线请求失败或受到限流")


def history_coverage(result, start, end):
    quotes = result["indicators"]["quote"][0]
    dates, missing = [], []
    for index, timestamp in enumerate(result.get("timestamp", [])):
        date = datetime.fromtimestamp(timestamp, SHANGHAI).date().isoformat()
        if start <= date <= end:
            values = [quotes.get(key, [])[index] for key in ("open", "high", "low", "close")]
            if any(value is None or not Decimal(str(value)).is_finite() for value in values):
                missing.append(date)
            else:
                dates.append(date)
    return {"observed_sessions": len(dates), "first_observed": min(dates) if dates else None,
            "last_observed": max(dates) if dates else None, "null_ohlc_dates": missing,
            "years": {str(year): sum(d.startswith(str(year)) for d in dates)
                      for year in range(int(start[:4]), int(end[:4]) + 1)},
            "dividend_events": len(result.get("events", {}).get("dividends", {})),
            "split_events": len(result.get("events", {}).get("splits", {})),
            "price_basis": "yahoo_provider_history_unreconciled",
            "completeness": "pending_exchange_calendar_and_adjustment_reconciliation"}


def cninfo_announcements(code, start, end):
    symbol(code)
    stocks = json.loads(download(CNINFO_STOCKS))["stockList"]
    matches = [r for r in stocks if r["code"] == code[:6]]
    require(len(matches) == 1, "巨潮证券组织标识缺失或不唯一")
    body = {"pageNum": "1", "pageSize": "30", "column": "szse", "tabName": "fulltext",
            "plate": "", "stock": f"{code[:6]},{matches[0]['orgId']}", "searchkey": "",
            "secid": "", "trade": "", "seDate": f"{start}~{end}", "sortName": "",
            "sortType": "", "isHLtitle": "false",
            "category": ";".join(["category_ndbg_szsh", "category_bndbg_szsh",
                                    "category_yjdbg_szsh", "category_sjdbg_szsh"])}
    first = json.loads(download(CNINFO_QUERY, body))
    total = int(first["totalAnnouncement"])
    require(total > 0, "巨潮未返回定期报告；可能是覆盖不足、接口变化或访问限制")
    output = list(first.get("announcements") or [])
    for page in range(2, (total + 29) // 30 + 1):
        import time
        time.sleep(0.6)
        response = json.loads(download(CNINFO_QUERY, {**body, "pageNum": str(page)}))
        require(int(response["totalAnnouncement"]) == total, "翻页期间公告总数变化，请重试")
        output.extend(response.get("announcements") or [])
    require(len(output) == total, "巨潮公告翻页不完整")
    require(len({r["announcementId"] for r in output}) == total, "公告翻页存在重复")
    return output


def report_catalog(code, announcements, start, end):
    output = []
    periods = {"第一季度": ("03-31", "q1"), "半年度": ("06-30", "half_year"),
               "第三季度": ("09-30", "q3"), "年度": ("12-31", "annual")}
    for row in announcements:
        title = row.get("announcementTitle", "")
        if any(term in title for term in ("摘要", "英文", "取消", "更正公告", "说明", "意见")):
            continue
        match = re.search(r"(20\d{2})年?(第一季度|半年度|第三季度|年度)报告", title)
        if not match or not row.get("adjunctUrl"):
            continue
        suffix, kind = periods[match[2]]
        period = f"{match[1]}-{suffix}"
        if not start <= period <= end:
            continue
        # 巨潮字段常只有公告日 00:00，不据此假定该日开盘前即可用。
        date = datetime.fromtimestamp(row["announcementTime"] / 1000, SHANGHAI).date().isoformat()
        available = date + "T23:59:59+08:00"
        output.append({"symbol": code, "period_end": period, "report_type": kind,
                       "published_at": available, "available_at": available,
                       "announcement_time_ms": row["announcementTime"],
                       "revision_id": str(row["announcementId"]), "version_verified": False,
                       "source_uri": "https://static.cninfo.com.cn/" + row["adjunctUrl"].lstrip("/"),
                       "title": title, "metrics": {}})
    return output


def probe_public(config_path, destination, report_symbol="600036.SH"):
    config = read_json(config_path)
    symbol(report_symbol)
    with new_directory(destination) as stage:
        raw = stage / "raw"
        raw.mkdir()
        result = {"status": "coverage_probe_only", "start": config["start"], "end": config["end"],
                  "retrieved_at": datetime.now(timezone.utc).isoformat(), "etfs": [],
                  "historical_top20": "pending_historical_constituents_and_year_end_market_caps",
                  "release_ready": False, "errors": []}
        for item in config["etfs"]:
            code = item["symbol"]
            try:
                url, payload, history = yahoo_history(code, config["source_start"], config["end"])
                write_json(raw / f"{code}.json", payload)
                result["etfs"].append({**item, "source_uri": url,
                                       **history_coverage(history, config["start"], config["end"])})
            except (ValueError, KeyError, TypeError, IndexError, OSError) as exc:
                result["errors"].append(f"{code}: {exc}")
        try:
            content = download(CSI_CONSTITUENTS)
            require(content[:8] == bytes.fromhex("d0cf11e0a1b11ae1") or content.startswith(b"PK"),
                    "指数成分文件不是 Excel，可能是访问限制页面")
            (raw / "000922-current-constituents.xls").write_bytes(content)
            result["current_constituents"] = {"source_uri": CSI_CONSTITUENTS, "bytes": len(content),
                                               "scope": "current_only_not_historical_top20"}
        except (ValueError, OSError) as exc:
            result["errors"].append(f"中证当前成分文件: {exc}")
        try:
            # 覆盖最终年度年报的下一年披露窗口。
            announcements = cninfo_announcements(report_symbol, config["start"],
                                                 str(int(config["end"][:4]) + 1) + "-12-31")
            write_json(raw / f"{report_symbol}-announcements.json", announcements)
            catalog = report_catalog(report_symbol, announcements, config["start"], config["end"])
            write_rows(stage / "report_sources.proposed.jsonl", catalog)
            expected = [f"{year}-{suffix}" for year in range(int(config["start"][:4]), int(config["end"][:4]) + 1)
                        for suffix in ("03-31", "06-30", "09-30", "12-31")
                        if config["start"] <= f"{year}-{suffix}" <= config["end"]]
            found = {r["period_end"] for r in catalog}
            result["report_probe"] = {"symbol": report_symbol, "announcement_count": len(announcements),
                                       "source_uri": CNINFO_QUERY, "scope": "one_company_probe_not_full_corpus",
                                       "full_report_candidates": len(catalog), "expected_periods": len(expected),
                                       "observed_periods": len(found), "missing_periods": sorted(set(expected) - found),
                                       "verification": "titles_only_pending_document_and_version_review"}
            candidates = sorted(catalog, key=lambda r: r["period_end"], reverse=True)
            require(bool(candidates), "没有可探测的财报 PDF 链接")
            link = candidates[0]["source_uri"]
            content = download(link)
            require(content.startswith(b"%PDF-"), "巨潮财报下载不是 PDF")
            (raw / f"{report_symbol}-sample-report.pdf").write_bytes(content)
            result["report_probe"]["sample_pdf"] = {"source_uri": link, "bytes": len(content)}
        except (ValueError, KeyError, TypeError, OSError) as exc:
            result["errors"].append(f"巨潮财报探测: {exc}")
        write_json(stage / "coverage.json", result)
        write_json(stage / "manifest.json", {"files": {p.relative_to(stage).as_posix(): digest(p)
                                                       for p in sorted(stage.rglob("*")) if p.is_file()}})
    return result
