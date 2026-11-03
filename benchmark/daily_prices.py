"""公开 HTTP 日线归档；当前成分只用于采集，供应商价格不冒充历史时点输入。"""

import hashlib
import json
import shutil
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from datetime import date, datetime, timedelta

from .collect import CALENDAR_URL, calendar_rows, price_coverage, retry, save_json, yahoo_rows
from .files import (day, digest, new_directory, number, read_json, require, rows,
                    symbol, write_json, write_rows)
from .index_membership import INDICES, membership_selection
from .pool_release import POOLS
from .public_sources import SHANGHAI, download, yahoo_history
from .release import verify_release

PRICE_BASIS = "yahoo_provider_history_unreconciled"


def selection(root):
    selected, pools = set(), {}
    for path in sorted((root / "universe").glob("*.json")):
        snapshot = read_json(path)
        pool = snapshot["pool"]
        require(pool in POOLS and snapshot["selected"], f"年度股票池无效: {path.name}")
        for code in snapshot["selected"]:
            symbol(code)
            require(code.endswith((".SH", ".SZ")), f"公开日线尚不支持该市场: {code}")
            selected.add(code)
        pools.setdefault(pool, []).append(snapshot["year"])
    require(set(pools) == set(POOLS), "必须同时包含红利和科技股票池快照")
    extra, _ = membership_selection(root)
    return sorted(selected | set(extra)), {pool: sorted(set(years)) for pool, years in sorted(pools.items())}


def payload_digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, allow_nan=False).encode()).hexdigest()


def normalize(code, history, start, end, uri, sessions):
    prices, observations = [], []
    for row in yahoo_rows(code, history, start, end, uri):
        row.pop("source_uri", None)
        row["source_record"] = f"sources/{code}.json"
        issues = []
        for key in ("open", "high", "low", "close"):
            try:
                number(row[key], positive=True)
            except (ValueError, ArithmeticError):
                issues.append(f"invalid_{key}")
        if row["date"] not in sessions:
            issues.append("calendar_closed_observation")
        if issues:
            observations.append({**row, "issue_codes": issues})
        else:
            prices.append(row)
    return sorted(prices, key=lambda r: r["date"]), sorted(observations, key=lambda r: r["date"])



def latest_quote(code, record, end, sessions):
    meta = record["response"]["chart"]["result"][0]["meta"]
    try:
        price = str(meta["regularMarketPrice"])
        number(price, positive=True)
        timestamp = datetime.fromtimestamp(int(meta["regularMarketTime"]), SHANGHAI)
        date = timestamp.date().isoformat()
        if date > end or date not in sessions:
            return None
        return {"symbol": code, "date": date, "price": price, "currency": meta.get("currency"),
                "provider_time": timestamp.isoformat(), "price_basis": "yahoo_regular_market_quote_unreconciled",
                "source_record": f"quote-sources/{date[:7]}.json",
                "source_key": f"{date}:{code}:{payload_digest(meta)[:16]}"}
    except (ValueError, TypeError, KeyError, ArithmeticError, OSError):
        return None

def update_prices(source, previous, destination, start, end, provenance, workers=2, cache_root=None, historical=None, refresh_history=False):
    require(day(start) <= day(end), "起始日期晚于截止日期")
    require(1 <= workers <= 2, "公开 HTTP 采集并发数须为1至2")
    require(cache_root is None or not Path(cache_root).resolve().is_relative_to(Path(destination).resolve()),
            "采集缓存不能放进待创建的归档目录")
    codes, _ = selection(source)
    if recovery := provenance.get("rebuild"):
        require(recovery["start"] == start and recovery["requested_end"] <= end, "重建不能缩小原归档日期范围")
        codes = sorted(set(codes) | {symbol(code) for code in recovery["symbols"]})
    if historical is not None:
        codes = sorted(set(codes) | set(historical["securities"]))
    if previous is not None:
        verify_release(previous)
        old = read_json(previous / "config.json")
        require(old["schema_version"] == 2 and old["price_basis"] == PRICE_BASIS
                and old.get("storage_layout") == "one_file_per_security",
                "既有归档价格口径不同，请用新的 --path 保留两种独立来源")
        require(old["start"] == start and old["requested_end"] <= end, "归档起点不同或截止日期回退")
        codes = sorted(set(codes) | set(old["symbols"]))
    with new_directory(destination) as stage:
        if previous is not None:
            shutil.copytree(previous, stage, dirs_exist_ok=True)
        shutil.copytree(source / "universe", stage / "universe", dirs_exist_ok=True)
        if (source / "index-membership").exists():
            shutil.copytree(source / "index-membership", stage / "index-membership", dirs_exist_ok=True)
        raw = stage / "sources"
        raw.mkdir(exist_ok=True)
        calendar_payload = json.loads(retry(lambda: download(CALENDAR_URL)))
        write_json(raw / "calendar.json", calendar_payload)
        known_dates = {date.fromisoformat(str(value)).isoformat() for value in calendar_payload}
        require(known_dates and min(known_dates) <= start and max(known_dates) >= end,
                "日历原件未覆盖请求范围，不能将未知日期推断为休市")
        calendar = calendar_rows(calendar_payload, start, end)
        sessions = {row["date"] for row in calendar if row["is_open"]}
        require(sessions, "区间内没有已知交易日")
        last_session = max(sessions)
        write_rows(stage / "calendar.jsonl", calendar)
        print(f"公开 HTTP 归档 {len(codes)} 只证券，最后交易日 {last_session}", flush=True)

        previous_reports = {r["symbol"]: r for r in read_json(stage / "coverage.json")["securities"]} if (stage / "coverage.json").exists() else {}
        previous_quotes = read_json(stage / "latest-quotes.json") if (stage / "latest-quotes.json").exists() else {}

        def collect(code):
            cache = Path(cache_root) / f"{code}.json" if cache_root is not None else None
            price_path = stage / "prices" / f"{code}.jsonl"
            old_rows = rows(price_path)
            old_report = previous_reports.get(code)
            if not refresh_history and old_report and old_report["status"] == "source_unavailable" and old_report.get("historical_retired"):
                return {**old_report, "source_check": "previously_unavailable_retired"}
            quote_only = bool(old_rows) and previous is not None and not refresh_history
            fetch_start = max(start, (day(last_session) - timedelta(days=14)).isoformat()) if quote_only else start
            record, origin = None, None
            try:
                record = read_json(cache) if previous is None and cache is not None and cache.exists() else None
                origin = "cache" if record is not None else None
                if record is not None:
                    require(record["sha256"] == payload_digest(record["response"]), f"来源缓存哈希错误: {code}")
                    require(record["symbol"] == code and record["price_basis"] == PRICE_BASIS, f"来源缓存身份错误: {code}")
                    if (record["start"], record["end"]) != (start, end):
                        record = None
                if record is None:
                    uri, payload, _ = retry(lambda: yahoo_history(code, fetch_start, end), attempts=2)
                    record = {"symbol": code, "start": fetch_start, "end": end, "price_basis": PRICE_BASIS,
                              "source_uri": uri, "response": payload, "sha256": payload_digest(payload)}
                    origin = "live"
                history = record["response"]["chart"]["result"][0]
                ticker = code.replace(".SH", ".SS")
                require(history["meta"]["symbol"] == ticker, f"{code}: 来源证券身份不符")
                if quote_only:
                    prices = old_rows
                    observations = rows(stage / "source-observations" / f"{code}.jsonl")
                else:
                    prices, observations = normalize(code, history, start, end, record["source_uri"], sessions)
                quote = latest_quote(code, record, end, sessions)
                require(prices or quote is not None, f"{code}: 区间内没有有效日线或报价")
            except (OSError, ValueError, TypeError, KeyError, ArithmeticError) as exc:
                if origin == "live" and record is not None:
                    write_json(raw / f"{code}-failed-response.json", record)
                coverage = price_coverage(old_rows, calendar, start, end)
                print(f"来源问题 {code}: {exc}", flush=True)
                return {"symbol": code, "status": "source_unavailable", **coverage,
                        "issue_code": "public_history_unavailable", "detail": str(exc)}

            prior_quote = previous_quotes.get(code)
            quote_source = None
            if quote is not None and (prior_quote is None or quote["provider_time"] >= prior_quote["provider_time"]):
                meta = record["response"]["chart"]["result"][0]["meta"]
                quote_source = {"symbol": code, "source_uri": record["source_uri"],
                                "response_part": "chart.result[0].meta", "response": meta,
                                "sha256": payload_digest(meta)}
            else:
                quote = prior_quote

            # 历史仅首次/显式全区间刷新；近期查询只更新独立报价，不拼接价格尺度。
            if not quote_only:
                price_path.parent.mkdir(exist_ok=True)
                write_rows(price_path, prices)
                write_json(raw / f"{code}.json", record)
                observation_path = stage / "source-observations" / f"{code}.jsonl"
                if observations:
                    observation_path.parent.mkdir(exist_ok=True)
                    write_rows(observation_path, observations)
                elif observation_path.exists():
                    observation_path.unlink()
                if cache is not None:
                    cache.parent.mkdir(parents=True, exist_ok=True)
                    save_json(cache, record)
            coverage = price_coverage(prices, calendar, start, end)
            last_price = max([d for d in (coverage["last_observed"], quote["date"] if quote else None) if d], default=None)
            status = ("current_daily_bar" if coverage["last_observed"] == last_session else
                      "current_quote_only" if last_price == last_session else "source_lags_or_no_trade")
            print(f"公开日线 {code}: {len(prices)} 行，末日 {coverage['last_observed']}", flush=True)
            return {"symbol": code, "status": status, **coverage,
                    "latest_quote": quote, "last_price_date": last_price, "_quote_source": quote_source,
                    "history_refreshed": not quote_only,
                    "source_uri": read_json(raw / f"{code}.json")["source_uri"], "raw_sha256": digest(raw / f"{code}.json"),
                    "excluded_source_observations": len(observations)}

        with ThreadPoolExecutor(max_workers=workers) as pool:
            coverage = list(pool.map(collect, codes))
        months, source_months = {}, {}
        for item in coverage:
            packet = item.pop("_quote_source", None)
            quote = item.get("latest_quote")
            if not quote:
                continue
            month = quote["date"][:7]
            if month not in months:
                path = stage / "daily-quotes" / f"{month}.json"
                months[month] = read_json(path) if path.exists() else {}
            months[month][f"{quote['date']}:{item['symbol']}"] = quote
            if packet:
                if month not in source_months:
                    path = stage / "quote-sources" / f"{month}.json"
                    source_months[month] = read_json(path) if path.exists() else {}
                source_months[month][quote["source_key"]] = packet
        for directory, values in (("daily-quotes", months), ("quote-sources", source_months)):
            for month, value in values.items():
                (stage / directory).mkdir(exist_ok=True)
                write_json(stage / directory / f"{month}.json", value)
        write_json(stage / "latest-quotes.json", {r["symbol"]: r["latest_quote"] for r in coverage if r.get("latest_quote")})
        _, pools = selection(stage)
        _, indices = membership_selection(stage)
        by_code = {r["symbol"]: r for r in coverage}
        current_members = {index: set() for index in INDICES}
        for path in (source / "index-membership").glob("*/*.json"):
            snapshot = read_json(path)
            if snapshot["observed_on"] == indices[snapshot["index"]]["observed_on"]:
                current_members[snapshot["index"]].update(r["symbol"] for r in snapshot["members"])
        index_coverage = {index: {"members": len(members),
                                  "with_history": sum(by_code[c]["observed_sessions"] > 0 for c in members),
                                  "at_last_session": sum(by_code[c].get("last_price_date", by_code[c]["last_observed"]) == last_session for c in members),
                                  "daily_bars_at_last_session": sum(by_code[c]["last_observed"] == last_session for c in members),
                                  "quotes_at_last_session": sum(by_code[c].get("latest_quote", {}).get("date") == last_session
                                                                for c in members if by_code[c].get("latest_quote"))}
                          for index, members in current_members.items()}
        for item in coverage:
            prior = historical["securities"].get(item["symbol"]) if historical else None
            item["historical_reference_available"] = prior is not None
            item["historical_retired"] = bool(prior and prior["outDate"] and prior["outDate"] <= end)
            if prior:
                item["historical_last_observed"] = prior["series"]["3"]["last_observed"]
        if historical:
            write_json(stage / "historical-reference.json", historical)
        config = {"schema_version": 2, "storage_layout": "one_file_per_security", "history_refresh_policy": "initial_or_explicit", "start": start, "requested_end": end, "last_session": last_session,
                  "symbols": codes, "pools": pools, "indices": indices, "source": provenance,
                  "price_basis": PRICE_BASIS, "calendar_source": CALENDAR_URL,
                  "scope": "年度股票池历史成员与当前指数完整成分并集；当前成分不代表历史成分",
                  "historical_reference": "historical-reference.json" if historical else None, "release_ready": False}
        write_json(stage / "config.json", config)
        write_json(stage / "coverage.json", {"securities": coverage, "indices": index_coverage})
        (stage / "README.md").write_text(
            "---\nlicense: other\nviewer: false\n---\n\n# 每日公开行情归档\n\n当前中证A50、上证50完整成分与既有红利/科技历史成员并集。\n"
            "Yahoo历史OHLC使用供应商历史口径，adjusted_close另列；未完成复权对账，不能冒充原始未复权或Baostock后复权价。\n"
            "历史仅首次或显式全区间刷新，不拼接调整尺度；日常更新独立报价，按月累计保存报价和源元数据。\n"
            "latest-quotes单独保存有供应商时间戳的最新报价；不将报价补写成OHLC或调整收盘。\n"
            "coverage按证券报告实际首末日期、交易日缺口和来源问题；首末观察不推定上市/退市，缺行不推定停牌。\n"
            "不生成成交状态、成交额、财务指标或季度评分。当前成分不能回填为历史股票池。\n"
            "包含决策后价格，不得直接作为历史策略输入或模型训练语料。\n"
            "historical-reference固定引用原Baostock历史归档；不将其价格拼进本供应商序列。\n", encoding="utf-8")
        write_json(stage / "manifest.json", {"schema_version": 1, "files": {
            p.relative_to(stage).as_posix(): digest(p) for p in sorted(stage.rglob("*"))
            if p.is_file() and p.name != "manifest.json"}})
    return {"stocks": len(codes),
            "current_price_count": sum(r.get("last_price_date") == last_session for r in coverage),
            "latest_quote_records": sum(r.get("latest_quote") is not None for r in coverage),
            "available_history": sum(r["observed_sessions"] > 0 or r["historical_reference_available"] for r in coverage),
            "unavailable_active": sum(r["status"] == "source_unavailable" and not r["historical_retired"] for r in coverage), "with_history": sum(r["observed_sessions"] > 0 for r in coverage),
            "source_unavailable": sum(r["status"] == "source_unavailable" for r in coverage),
            "historical_gap_securities": sum(r["internal_missing_count"] > 0 for r in coverage),
            "indices": index_coverage, "last_session": last_session}
