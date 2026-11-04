"""分红事实采集与五年窗口整理；不把供应商当前记录当作历史时点已核验事实。"""

import hashlib
import json
import time
import csv
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime
from decimal import Decimal
from pathlib import Path

from .files import day, digest, new_directory, read_json, require, symbol, write_json, write_rows
from .public_sources import SHANGHAI, dividend_page


def source_date(value):
    if value is None or value == "":
        return None
    result = value[:10]
    day(result)
    return result


def amount(value):
    if value is None:
        return None
    number = Decimal(str(value))
    require(number.is_finite() and number >= 0, "分红金额/送转比例无效")
    return str(number)


def normalize(code, records, start, end):
    """窗口按实际除权除息日；尚未实施的方案单列，不能加进已派现金。"""
    events, plans = [], []
    for record in records:
        require(record["SECUCODE"] == code and record["SECURITY_CODE"] == code[:6], "分红证券身份不符")
        ex_date = source_date(record.get("EX_DIVIDEND_DATE"))
        notice = source_date(record.get("NOTICE_DATE"))
        plan_notice = source_date(record.get("PLAN_NOTICE_DATE"))
        value = {"symbol": code, "security_name": record.get("SECURITY_NAME_ABBR"), "report_date": source_date(record["REPORT_DATE"]),
                 "plan_announced_on": plan_notice, "notice_on": notice,
                 "record_date": source_date(record.get("EQUITY_RECORD_DATE")), "ex_dividend_date": ex_date,
                 "cash_per_10_shares_cny_pretax": amount(record.get("PRETAX_BONUS_RMB")),
                 "bonus_shares_per_10": amount(record.get("BONUS_RATIO")),
                 "capitalization_shares_per_10": amount(record.get("IT_RATIO")),
                 "provider_status": record["ASSIGN_PROGRESS"], "plan_text": record.get("IMPL_PLAN_PROFILE"),
                 "source_record_sha256": hashlib.sha256(json.dumps(record, sort_keys=True, ensure_ascii=False).encode()).hexdigest(),
                 "pit_verified": False}
        if record["ASSIGN_PROGRESS"] == "实施分配" and ex_date and start <= ex_date <= end:
            value["status"] = "implemented"
            events.append(value)
        elif any(d and start <= d <= end for d in (notice, plan_notice)):
            value["status"] = "outside_window" if record["ASSIGN_PROGRESS"] == "实施分配" else "proposal_or_pending"
            plans.append(value)
    return sorted(events, key=lambda r: (r["ex_dividend_date"], r["report_date"])), plans


def collect_dividends(symbols, start, end, destination, cache_root, workers=2):
    require(day(start) <= day(end), "分红窗口错误")
    require(1 <= workers <= 2, "分红采集并发须为1至2")
    codes = sorted({symbol(code) for code in symbols})
    require(codes, "分红证券范围为空")
    cache_root = Path(cache_root)
    cache_root.mkdir(parents=True, exist_ok=True)
    collected_at = datetime.now(SHANGHAI).isoformat(timespec="seconds")

    def acquire(code):
        try:
            records, files, total, pages, source_empty = [], {}, None, 1, False
            page = 1
            while page <= pages:
                path = cache_root / f"{code}-{page}.json"
                if path.exists():
                    cached = read_json(path)
                    require(cached["symbol"] == code and cached["page"] == page, "分红缓存请求不符")
                else:
                    error = None
                    for attempt in range(3):
                        try:
                            url, payload = dividend_page(code, page)
                            cached = {"symbol": code, "page": page, "source_uri": url, "response": payload,
                                      "retrieved_at": datetime.now(SHANGHAI).isoformat(timespec="seconds")}
                            write_json(path, cached)
                            break
                        except (ValueError, OSError, TimeoutError) as exc:
                            error = exc
                            if attempt < 2:
                                time.sleep(1 + attempt)
                    else:
                        raise ValueError(str(error))
                result = cached["response"]["result"]
                if result is None:
                    require(cached["response"].get("code") == 9201 and page == 1, "分红来源返回未知空响应")
                    files[path.name] = digest(path)
                    total, source_empty = 0, True
                    break
                count = int(result["count"])
                source_empty = count == 0
                require(total is None or total == count, "分红翻页期间记录总数变化")
                total, pages = count, int(result["pages"])
                require(pages <= 100 and isinstance(result["data"], list), "分红分页元数据错误")
                records.extend(result["data"])
                files[path.name] = digest(path)
                page += 1
            require(len(records) == total, "分红分页不完整")
            keys = [json.dumps(r, sort_keys=True, ensure_ascii=False) for r in records]
            require(len(keys) == len(set(keys)), "分红来源出现重复记录")
            events, plans = normalize(code, records, start, end)
            years = {}
            for event in events:
                year = event["ex_dividend_date"][:4]
                years.setdefault(year, []).append(event)
            summary = {"symbol": code, "security_name": records[0].get("SECURITY_NAME_ABBR") if records else None,
                       "status": "source_no_records" if source_empty else "observed" if events else "no_implemented_record_in_window",
                       "source_records": len(records), "implemented_events": len(events),
                       "cash_events": sum(r["cash_per_10_shares_cny_pretax"] is not None and Decimal(r["cash_per_10_shares_cny_pretax"]) > 0 for r in events),
                       "cash_per_10_by_payment_year": {year: str(sum(Decimal(r["cash_per_10_shares_cny_pretax"]) for r in values))
                              if all(r["cash_per_10_shares_cny_pretax"] is not None for r in values) else None for year, values in years.items()},
                       "proposal_or_other_records": len(plans), "raw_files": files, "pit_verified": False}
            print(f"分红 {code}: {len(events)} 项实施记录，{len(plans)} 项其他方案", flush=True)
            return summary, events, plans
        except (ValueError, OSError, KeyError, TypeError, ArithmeticError) as exc:
            print(f"分红 {code}: 来源缺失 {exc}", flush=True)
            return {"symbol": code, "status": "source_unavailable", "error": str(exc)}, [], []

    with ThreadPoolExecutor(max_workers=workers) as pool:
        results = list(pool.map(acquire, codes))
    summaries = [r[0] for r in results]
    with new_directory(destination) as stage:
        (stage / "stocks").mkdir()
        (stage / "raw").mkdir()
        for summary, events, plans in results:
            write_json(stage / "stocks" / f"{summary['symbol']}.json", {"summary": summary, "implemented": events, "other_plans": plans})
            for name in summary.get("raw_files", {}):
                (stage / "raw" / name).write_bytes((cache_root / name).read_bytes())
        write_rows(stage / "summary.jsonl", summaries)
        write_rows(stage / "events.jsonl", [event for _, events, _ in results for event in events])
        years = [str(year) for year in range(day(start).year, day(end).year + 1)]
        with (stage / "summary.csv").open("w", encoding="utf-8-sig", newline="") as handle:
            writer = csv.writer(handle)
            writer.writerow(["证券代码", "证券简称", "记录状态", "实施记录数", "现金分红记录数",
                             *[year + "年每10股派现合计（元，税前，未复权）" for year in years]])
            for r in summaries:
                writer.writerow([r["symbol"], r.get("security_name"), r["status"], r.get("implemented_events"), r.get("cash_events"),
                                 *[r.get("cash_per_10_by_payment_year", {}).get(year) for year in years]])
        with (stage / "events.csv").open("w", encoding="utf-8-sig", newline="") as handle:
            writer = csv.writer(handle)
            writer.writerow(["证券代码", "证券简称", "报告期", "除权除息日", "股权登记日", "方案公告日", "最新公告日",
                             "每10股派现（元，税前）", "每10股送股", "每10股转增", "方案"])
            for _, events, _ in results:
                for r in events:
                    writer.writerow([r[k] for k in ("symbol", "security_name", "report_date", "ex_dividend_date", "record_date",
                                     "plan_announced_on", "notice_on", "cash_per_10_shares_cny_pretax", "bonus_shares_per_10",
                                     "capitalization_shares_per_10", "plan_text")])
        report = {"start": start, "end": end, "collected_at": collected_at, "securities": len(codes),
                  "source_unavailable": [r["symbol"] for r in summaries if r["status"] == "source_unavailable"],
                  "source_no_records": [r["symbol"] for r in summaries if r["status"] == "source_no_records"],
                  "implemented_events": sum(r.get("implemented_events", 0) for r in summaries),
                  "window_basis": "ex_dividend_date", "amount_basis": "CNY_pretax_per_10_original_shares", "pit_verified": False}
        write_json(stage / "coverage.json", report)
        write_json(stage / "manifest.json", {"files": {p.relative_to(stage).as_posix(): digest(p) for p in sorted(stage.rglob("*")) if p.is_file()}})
    return report
