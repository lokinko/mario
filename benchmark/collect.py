"""免费来源批量补采；保留原始响应、独立价格口径与可恢复进度。"""

import hashlib
import json
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import date, datetime, timedelta, timezone
from urllib.parse import urlencode, urlparse

from .files import digest, local_file, read_json, require, rows, symbol, write_json, write_rows
from .public_sources import (CNINFO_STOCKS, SHANGHAI, cninfo_announcements,
                             download, report_catalog, yahoo_history)

CALENDAR_URL = "https://raw.githubusercontent.com/akfamily/akshare/main/akshare/data/calendar.json"


def save_json(path, value):
    part = path.with_suffix(path.suffix + ".part")
    write_json(part, value)
    part.replace(path)


def retry(operation, attempts=3):
    for attempt in range(attempts):
        try:
            return operation()
        except (OSError, ValueError, KeyError, TypeError, IndexError):
            if attempt == attempts - 1:
                raise
            time.sleep(1 + 2 * attempt)


def calendar_rows(values, start, end, source=CALENDAR_URL):
    dates = {date.fromisoformat(v).isoformat() for v in values}
    require(dates and min(dates) <= start and max(dates) >= end, "免费日历未覆盖请求区间")
    output = []
    current = date.fromisoformat(start)
    while current <= date.fromisoformat(end):
        day = current.isoformat()
        output.append({"date": day, "is_open": day in dates, "source_uri": source})
        current += timedelta(days=1)
    return output


def yahoo_rows(code, history, start, end, source):
    quotes = history["indicators"]["quote"][0]
    adjusted = history["indicators"].get("adjclose", [{}])[0].get("adjclose", [])
    output = []
    for index, timestamp in enumerate(history.get("timestamp", [])):
        day = datetime.fromtimestamp(timestamp, SHANGHAI).date().isoformat()
        if not start <= day <= end:
            continue
        values = {key: quotes[key][index] for key in ("open", "high", "low", "close", "volume")}
        if any(values[key] is None for key in ("open", "high", "low", "close")):
            continue
        output.append({"symbol": code, "date": day,
                       **{key: str(value) if value is not None else None for key, value in values.items()},
                       "adjusted_close": str(adjusted[index]) if index < len(adjusted) and adjusted[index] is not None else None,
                       "price_basis": "yahoo_provider_history_unreconciled", "source_uri": source})
    require(len({r["date"] for r in output}) == len(output), "Yahoo 日期重复")
    return output


def price_coverage(prices, calendar, start, end):
    valid = sorted({r["date"] for r in prices if start <= r["date"] <= end})
    opened = {r["date"] for r in calendar if r["is_open"] and start <= r["date"] <= end}
    observed = set(valid)
    internal = sorted(d for d in opened - observed if valid and valid[0] <= d <= valid[-1])
    return {"observed_sessions": len(valid), "first_observed": valid[0] if valid else None,
            "last_observed": valid[-1] if valid else None,
            "internal_missing_sessions": internal, "internal_missing_count": len(internal),
            "outside_observed_range_sessions": len(opened - observed - set(internal)),
            "nontrading_observations": sorted(observed - opened),
            "calendar_rule": "周末及日历闭市日不算缺口；首末观察区间外不推断上市/退市"}


def listing_observations(prices, fact):
    if not fact:
        return prices, []
    listed = date.fromisoformat(fact["listed_on"]).isoformat()
    require(fact["source_uri"].startswith("https://"), "上市日期需要明确 HTTPS 出处")
    return ([r for r in prices if r["date"] >= listed],
            [r for r in prices if r["date"] < listed])


def tencent_year(code, year):
    ticker = code[-2:].lower() + code[:6]
    url = "https://proxy.finance.qq.com/ifzqgtimg/appstock/app/newfqkline/get?" + urlencode(
        {"param": f"{ticker},day,{year}-01-01,{year}-12-31,640,"})
    payload = json.loads(download(url), parse_float=str)
    series = payload["data"][ticker].get("day", [])
    result = []
    for row in series:
        require(len(row) >= 6, "腾讯日线字段不完整")
        if not f"{year}-01-01" <= row[0] <= f"{year}-12-31":
            continue
        result.append({"symbol": code, "date": row[0], "open": str(row[1]), "close": str(row[2]),
                       "high": str(row[3]), "low": str(row[4]), "volume_source_units": str(row[5]),
                       "price_basis": "tencent_unadjusted", "source_uri": url})
    return payload, result


def collect_tencent(root, code, start, end):
    """Yahoo 不覆盖的证券使用腾讯独立口径，不伪造 Yahoo 调整因子。"""
    prices = {}
    for year in range(int(start[:4]), int(end[:4]) + 1):
        path = root / f"alternatives/{code}-{year}.json"
        if not path.exists():
            payload, output = retry(lambda: tencent_year(code, str(year)))
            save_json(path, {"payload": payload, "rows": output})
        for row in read_json(path)["rows"]:
            if start <= row["date"] <= end:
                prices[row["date"]] = row
    require(bool(prices), f"{code} 腾讯亦无历史日线")
    return [prices[key] for key in sorted(prices)]


def repair_catalogs(root, config_path):
    """针对已观察交易期内缺失财报，取消供应商分类限制重查披露窗口。"""
    config = read_json(config_path)
    stocks = read_json(root / "raw/cninfo-stocks.json")["stockList"]
    extras = extra_reports(root)
    results = []
    codes = [r["symbol"] for r in read_json(root / "request.json")["selection"]["instruments"]]
    for code in codes:
        price_rows = rows(root / f"prices/{code}.jsonl")
        if not price_rows:
            continue
        first = min(r["date"] for r in price_rows)
        base_path = root / f"announcements/{code}.json"
        if not base_path.exists():
            continue
        announcements = {str(r["announcementId"]): r for r in read_json(base_path)}
        for supplement in sorted((root / "announcements").glob(f"{code}-*-supplement.json")):
            announcements.update({str(r["announcementId"]): r for r in read_json(supplement)["announcements"]})
        found = {r["period_end"] for r in report_catalog(code, list(announcements.values()), config["start"], config["end"])}
        found.update(r["period_end"] for r in extras if r["symbol"] == code)
        for year in range(int(config["start"][:4]), int(config["end"][:4]) + 1):
            windows = {"03-31": (f"{year}-04-01", f"{year}-06-30"),
                       "06-30": (f"{year}-07-01", f"{year}-10-31"),
                       "09-30": (f"{year}-10-01", f"{year}-12-31"),
                       "12-31": (f"{year+1}-01-01", f"{year+1}-06-30")}
            for suffix, (start, end) in windows.items():
                period = f"{year}-{suffix}"
                if not max(first, config["start"]) <= period <= config["end"] or period in found:
                    continue
                target = root / f"announcements/{code}-{period}-supplement.json"
                try:
                    if not target.exists():
                        extra = retry(lambda: cninfo_announcements(code, start, end, stocks,
                                                                  report_categories=False, allow_empty=True))
                        save_json(target, {"query": {"symbol": code, "start": start, "end": end,
                                                     "report_categories": False}, "announcements": extra})
                    extra = read_json(target)["announcements"]
                    proposed = report_catalog(code, extra, config["start"], config["end"])
                    hit = [r for r in proposed if r["period_end"] == period]
                    results.append({"symbol": code, "period_end": period, "found": len(hit)})
                    print(f"补查 {code}/{period}: {len(hit)} 份完整财报候选", flush=True)
                except (OSError, ValueError, KeyError, TypeError) as exc:
                    results.append({"symbol": code, "period_end": period, "error": str(exc)})
    save_json(root / "catalog-repair.json", results)
    return {"checked": len(results), "recovered_periods": sum(r.get("found", 0) > 0 for r in results),
            "errors": [r for r in results if "error" in r]}


def pdf_file(root, report):
    url = report["source_uri"]
    host = urlparse(url).hostname or ""
    require(url.startswith("https://") and host == "static.cninfo.com.cn", "仅接收巨潮官方 PDF 来源")
    target = root / "documents" / (hashlib.sha256(url.encode()).hexdigest() + ".pdf")
    if target.exists():
        with target.open("rb") as handle:
            require(handle.read(5) == b"%PDF-", "已缓存文件不是 PDF")
    else:
        def fetch_pdf():
            content = download(url)
            require(content.startswith(b"%PDF-"), "下载结果不是 PDF")
            return content

        content = retry(fetch_pdf)
        part = target.with_suffix(".part")
        part.write_bytes(content)
        part.replace(target)
    return {**report, "sha256": digest(target), "document_path": target.relative_to(root).as_posix(),
            "bytes": target.stat().st_size, "download_status": "downloaded"}


def extra_reports(root):
    reports = rows(root / "extra-reports.jsonl")
    for report in reports:
        symbol(report["symbol"])
        period = date.fromisoformat(report["period_end"])
        require(report["report_type"] == {3: "q1", 6: "half_year", 9: "q3", 12: "annual"}.get(period.month)
                and period.day == {3: 31, 6: 30, 9: 30, 12: 31}.get(period.month), "补充财报报告期错误")
        require(report["source_uri"].startswith("https://static.cninfo.com.cn/"), "补充财报需有巨潮原件出处")
        review = report["period_review"]
        require(review["method"] == "pdf_cover_text" and review["page"] >= 1 and review["excerpt"],
                "补充财报必须有封面报告期核对证据")
        require(digest(local_file(root, report["document_path"])) == report["sha256"] == review["sha256"],
                "补充财报封面核对的文件哈希不符")
        require(report["version_verified"] is False, "封面报告期核对不能升级为历史版本已核验")
    return reports


def collect_public(config_path, symbols_path, root, workers=4):
    require(1 <= workers <= 6, "采集并发限制为 1–6")
    config, selection = read_json(config_path), read_json(symbols_path)
    instruments = selection["instruments"]
    codes = [symbol(row["symbol"]) for row in instruments]
    require(len(codes) == len(set(codes)) and codes, "采集证券重复或为空")
    root = root.resolve()
    root.mkdir(parents=True, exist_ok=True)
    request = {"schema_version": 1, "config": config, "selection": selection}
    if (root / "request.json").exists():
        require(read_json(root / "request.json") == request, "断点续传的配置/采集股票池与原请求不同")
    else:
        require(not list(root.iterdir()), "首次采集目录必须为空")
        save_json(root / "request.json", request)
    for folder in ("raw", "prices", "prelisting", "announcements", "documents", "document-index", "alternatives"):
        (root / folder).mkdir(exist_ok=True)
    errors = []
    calendar_path = root / "raw/calendar.json"
    if not calendar_path.exists():
        save_json(calendar_path, json.loads(retry(lambda: download(CALENDAR_URL))))
    calendar = calendar_rows(read_json(calendar_path), config["source_start"], config["end"])
    write_rows(root / "calendar.jsonl", calendar)
    stock_path = root / "raw/cninfo-stocks.json"
    if not stock_path.exists():
        save_json(stock_path, json.loads(retry(lambda: download(CNINFO_STOCKS))))
    stock_records = read_json(stock_path)["stockList"]
    all_codes = codes + [e["symbol"] for e in config["etfs"] if e["symbol"] not in codes]
    extras = extra_reports(root)
    facts = read_json(root / "instrument-facts.json") if (root / "instrument-facts.json").exists() else {}
    coverage = {}

    def collect_security(code):
        failures = []
        price_path, raw_path = root / f"prices/{code}.jsonl", root / f"raw/{code}.json"
        try:
            if not raw_path.exists():
                url, payload, history = retry(lambda: yahoo_history(code, config["source_start"], config["end"]))
                save_json(raw_path, {"source_uri": url, "payload": payload})
            saved = read_json(raw_path)
            history = saved["payload"]["chart"]["result"][0]
            prices = yahoo_rows(code, history, config["source_start"], config["end"], saved["source_uri"])
            prices, before_listing = listing_observations(prices, facts.get(code))
            if before_listing:
                write_rows(root / f"prelisting/{code}.jsonl", before_listing)
            write_rows(price_path, prices)
            cov = price_coverage(prices, calendar, config["start"], config["end"])
            if code in facts:
                cov.update(listing_fact=facts[code], excluded_prelisting_observations=len(before_listing))
            # 只为存在内部交易日缺口的年份补取独立口径，不逐日纠缠休市日。
            alternatives = []
            for year in sorted({d[:4] for d in cov["internal_missing_sessions"]}):
                alternative_path = root / f"alternatives/{code}-{year}.json"
                try:
                    if not alternative_path.exists():
                        payload, alternate_rows = retry(lambda: tencent_year(code, year))
                        save_json(alternative_path, {"payload": payload, "rows": alternate_rows})
                    alternatives.extend(read_json(alternative_path)["rows"])
                except (OSError, ValueError, KeyError, TypeError, IndexError) as exc:
                    failures.append(f"{code}/腾讯/{year}: {exc}")
            alt_dates = {r["date"] for r in alternatives}
            cov["alternative_source_recovered_dates"] = sorted(set(cov["internal_missing_sessions"]) & alt_dates)
            cov["remaining_internal_missing_dates"] = sorted(set(cov["internal_missing_sessions"]) - alt_dates)
            cov["price_basis"] = "Yahoo 原响应；腾讯未复权替代行单独存放，未混合复权计算"
        except (OSError, ValueError, KeyError, TypeError, IndexError) as exc:
            primary_error = str(exc)
            try:
                prices = collect_tencent(root, code, config["source_start"], config["end"])
                prices, before_listing = listing_observations(prices, facts.get(code))
                if before_listing:
                    write_rows(root / f"prelisting/{code}.jsonl", before_listing)
                write_rows(price_path, prices)
                cov = price_coverage(prices, calendar, config["start"], config["end"])
                cov.update(price_basis="tencent_unadjusted", primary_source_note=primary_error,
                           remaining_internal_missing_dates=cov["internal_missing_sessions"])
                if code in facts:
                    cov.update(listing_fact=facts[code], excluded_prelisting_observations=len(before_listing))
            except (OSError, ValueError, KeyError, TypeError, IndexError) as fallback_exc:
                cov = {"status": "failed"}
                failures.append(f"{code}/日线: {primary_error}；腾讯: {fallback_exc}")
        catalog = []
        if code in codes:
            try:
                path = root / f"announcements/{code}.json"
                if not path.exists():
                    announcements = retry(lambda: cninfo_announcements(
                        code, config["start"], f"{int(config['end'][:4]) + 1}-12-31", stock_records))
                    save_json(path, announcements)
                announcements = {str(r["announcementId"]): r for r in read_json(path)}
                for supplement in sorted((root / "announcements").glob(f"{code}-*-supplement.json")):
                    announcements.update({str(r["announcementId"]): r for r in read_json(supplement)["announcements"]})
                catalog = report_catalog(code, list(announcements.values()), config["start"], config["end"])
                catalog_by_id = {r["revision_id"]: r for r in catalog}
                for report in extras:
                    if report["symbol"] == code and config["start"] <= report["period_end"] <= config["end"]:
                        catalog_by_id[report["revision_id"]] = report
                catalog = list(catalog_by_id.values())
                cov["report_candidates"] = len(catalog)
                cov["report_periods"] = len({r["period_end"] for r in catalog})
            except (OSError, ValueError, KeyError, TypeError, IndexError) as exc:
                failures.append(f"{code}/财报目录: {exc}")
        return code, cov, catalog, failures

    catalogs = []
    with ThreadPoolExecutor(max_workers=workers) as pool:
        futures = [pool.submit(collect_security, code) for code in all_codes]
        for index, task in enumerate(as_completed(futures), 1):
            code, cov, catalog, failures = task.result()
            coverage[code] = cov
            catalogs.extend(catalog)
            errors.extend(failures)
            save_json(root / "progress.json", {"stage": "catalogs_and_prices", "completed_securities": index,
                                               "total_securities": len(all_codes), "errors": errors})
            print(f"采集 {index}/{len(all_codes)} {code}: 财报 {len(catalog)} 条；行情 {cov.get('observed_sessions', 0)} 行", flush=True)
    catalogs.sort(key=lambda r: (r["symbol"], r["period_end"], r["revision_id"]))
    write_rows(root / "report_sources.proposed.jsonl", catalogs)
    completed = []

    def fetch(report):
        index_path = root / "document-index" / (hashlib.sha256(report["source_uri"].encode()).hexdigest() + ".json")
        if index_path.exists():
            cached = read_json(index_path)
            path = root / cached["document_path"]
            if path.is_file():
                require(digest(path) == cached["sha256"], "缓存 PDF 哈希变化，请保留损坏文件后从源重新采集")
                return {**report, **{k: cached[k] for k in ("sha256", "document_path", "bytes", "download_status")}}
        result = pdf_file(root, report)
        save_json(index_path, result)
        return result

    by_url = {}
    for report in catalogs:
        by_url.setdefault(report["source_uri"], []).append(report)
    with ThreadPoolExecutor(max_workers=workers) as pool:
        futures = {pool.submit(fetch, reports[0]): reports for reports in by_url.values()}
        for index, task in enumerate(as_completed(futures), 1):
            reports = futures[task]
            try:
                result = task.result()
                for report in reports:
                    completed.append({**report, **{k: result[k] for k in ("sha256", "document_path", "bytes", "download_status")}})
            except (OSError, ValueError, KeyError, TypeError, IndexError) as exc:
                for report in reports:
                    errors.append(f"{report['symbol']}/{report['period_end']}/{report['revision_id']}: {exc}")
            if index % 25 == 0 or index == len(futures):
                save_json(root / "progress.json", {"stage": "documents", "processed": index,
                           "total": len(catalogs), "downloaded": len(completed), "errors": errors})
                print(f"财报原件 {index}/{len(catalogs)}；已下载 {len(completed)}；错误 {len(errors)}", flush=True)
    completed.sort(key=lambda r: (r["symbol"], r["period_end"], r["revision_id"]))
    write_rows(root / "reports.proposed.jsonl", completed)
    expected_periods = [f"{year}-{suffix}" for year in range(int(config["start"][:4]), int(config["end"][:4]) + 1)
                        for suffix in ("03-31", "06-30", "09-30", "12-31")
                        if config["start"] <= f"{year}-{suffix}" <= config["end"]]
    for code in codes:
        found = {r["period_end"] for r in completed if r["symbol"] == code}
        coverage[code]["downloaded_report_periods"] = len(found)
        coverage[code]["missing_report_periods"] = sorted(set(expected_periods) - found)
        first = coverage[code].get("first_observed")
        coverage[code]["missing_report_periods_within_price_history"] = sorted(
            p for p in set(expected_periods) - found if first and first <= p)
    result = {"status": "collected" if not errors else "collected_with_source_errors",
              "retrieved_at": datetime.now(timezone.utc).isoformat(),
              "scope": selection.get("scope", "explicit_candidate_collection"),
              "stock_count": len(codes), "etf_count": len(all_codes) - len(codes),
              "price_rows": sum(len(rows(root / f"prices/{code}.jsonl")) for code in all_codes),
              "report_candidates": len(catalogs), "downloaded_reports": len(completed),
              "report_bytes": sum(r["bytes"] for r in completed),
              "coverage": dict(sorted(coverage.items())), "errors": errors,
              "historical_top20": "pending_historical_constituents_and_year_end_market_caps",
              "release_ready": False}
    save_json(root / "coverage.json", result)
    save_json(root / "progress.json", {"stage": "finished", "downloaded": len(completed), "errors": errors})
    # 原件校验存于 document-index；最终清单让后续变化可复核。
    files = {p.relative_to(root).as_posix(): digest(p) for p in sorted(root.rglob("*"))
             if p.is_file() and p.name != "manifest.json" and not p.name.endswith(".part")}
    save_json(root / "manifest.json", {"schema_version": 1, "files": files})
    return {k: v for k, v in result.items() if k != "coverage"}
