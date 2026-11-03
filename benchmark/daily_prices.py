"""年度股票池历史成员与完整指数成分的日线归档；不生成策略输入或评分标签。"""

import shutil
import socket
from concurrent.futures import ProcessPoolExecutor, as_completed
from multiprocessing import get_context
from datetime import timedelta
from pathlib import Path

from .baostock_market import Baostock, provider_code
from .collect import retry
from .files import (Invalid, day, digest, new_directory, number, read_json, require,
                    rows, symbol, write_json, write_rows)
from .index_membership import membership_selection
from .pool_release import INDEX, POOLS, TOTAL_RETURN_INDEX
from .public_sources import CSI_INDEX_PERF, csi_index_closes
from .release import verify_release


def selection(root):
    selected, pools = set(), {}
    for path in sorted((root / "universe").glob("*.json")):
        snapshot = read_json(path)
        pool = snapshot["pool"]
        require(pool in POOLS, f"未知股票池: {pool}")
        require(snapshot["selected"], f"股票池为空: {path.name}")
        for code in snapshot["selected"]:
            provider_code(symbol(code))
            selected.add(code)
        pools.setdefault(pool, []).append(snapshot["year"])
    require(set(pools) == set(POOLS), "必须同时包含红利和科技股票池快照")
    extra, _ = membership_selection(root)
    return sorted(selected | set(extra)), {pool: sorted(set(years)) for pool, years in sorted(pools.items())}


def checked_rows(values, code, start, end, sessions, index=False, total_return=False):
    output = {}
    for row in values:
        date = day(row["date"]).isoformat()
        require(start <= date <= end, f"{code}: 来源返回区间外日期 {date}")
        require(date in sessions, f"{code}: 来源返回非交易日 {date}")
        require(date not in output, f"{code}: 来源返回重复日期 {date}")
        if not total_return:
            require(row["code"] == provider_code(code), f"{code}: 来源证券代码不符")
            if not index:
                require(row["tradestatus"] in ("0", "1"), f"{code}: 未知交易状态")
            issues = {}
            for key in ("open", "high", "low", "close", "preclose", "volume", "amount"):
                value = row[key]
                if value == "" and (key == "preclose" or (not index and row["tradestatus"] == "0")):
                    issues[key] = "provider_empty_preclose" if key == "preclose" else "provider_empty_suspended"
                    continue
                try:
                    number(value)
                except (ValueError, ArithmeticError) as exc:
                    raise Invalid(f"{code}: {date} 字段 {key} 无效") from exc
            if index or row["tradestatus"] == "1":
                for key in ("open", "high", "low", "close"):
                    number(row[key], positive=True)
                require(number(row["low"]) <= min(number(row["open"]), number(row["close"]))
                        <= max(number(row["open"]), number(row["close"])) <= number(row["high"]),
                        f"{code}: OHLC 范围不符 {date}")
            if issues:
                row = {**row, "issues": issues}
        else:
            number(row["close"], positive=True)
        output[date] = row
    return output


def collect_series(market, stage, code, flag, start, end, sessions, basic, overlap_days,
                   index=False, total_return=False):
    directory = stage / "prices" / code / f"adjust{flag}"
    saved = [row for path in sorted(directory.glob("*.jsonl")) for row in rows(path)]
    existing = checked_rows(saved, code, start, end, sessions, index, total_return)
    first, last = start, end
    if not index and not total_return:
        require(basic and basic["ipoDate"], f"{code}: 缺少上市日期，不能判定行情完整性")
        first = max(start, day(basic["ipoDate"]).isoformat())
        if basic["outDate"]:
            last = min(end, (day(basic["outDate"]) - timedelta(days=1)).isoformat())
    required = {date for date in sessions if first <= date <= last}
    since = max(first, (day(max(existing)) - timedelta(days=overlap_days)).isoformat()) if existing else first
    missing = sorted(required - set(existing))
    if missing:
        since = min(since, missing[0])
    def fetch_window(lower, upper):
        values = csi_index_closes(code, lower, upper) if total_return else market.daily(
            code, lower, upper, flag, index=index)
        if total_return:
            # 官方指数接口偶有闭市日点位；保留来源观察，但不当成交易日价格。
            closed = [row for row in values if day(row["date"]).isoformat() not in sessions]
            if closed:
                require(len({row["date"] for row in closed}) == len(closed), f"{code}: 闭市观察日期重复")
                for row in closed:
                    require(lower <= row["date"] <= upper, f"{code}: 闭市观察越出请求区间")
                    number(row["close"], positive=True)
                archive = stage / "source-observations" / f"{code}-calendar-closed.jsonl"
                archive.parent.mkdir(parents=True, exist_ok=True)
                observations = {row["date"]: row for row in rows(archive)}
                observations.update({row["date"]: {**row, "issue_code": "calendar_closed_index_observation"}
                                     for row in closed})
                write_rows(archive, [observations[date] for date in sorted(observations)])
            values = [row for row in values if row["date"] in sessions]
        return checked_rows(values, code, lower, upper, sessions, index, total_return)

    fresh = {}
    if required and since <= last:
        fresh = retry(lambda: fetch_window(since, last))
        # SDK 分页可能提前结束；按缺失年份缩小请求重取，不使用旧缓存掩盖缺行。
        absent = sorted({date for date in required if date >= since} - set(fresh))
        for year in sorted({date[:4] for date in absent}):
            dates = {date for date in absent if date.startswith(year)}
            lower, upper = min(dates), max(dates)

            def repair():
                result = fetch_window(lower, upper)
                missing = sorted(dates - set(result))
                require(not missing, f"{code}/adjust{flag}: 日线未到齐 {missing[:5]}")
                return result

            print(f"补取 {code}/adjust{flag}: {lower}—{upper}", flush=True)
            fresh.update(retry(repair))
    merged = {**existing, **fresh}
    absent = sorted(required - set(merged))
    require(not absent, f"{code}/adjust{flag}: 历史缺行 {absent[:5]}")
    directory.mkdir(parents=True, exist_ok=True)
    years = sorted({date[:4] for date in merged})
    for year in years:
        write_rows(directory / f"{year}.jsonl", [merged[date] for date in sorted(merged) if date.startswith(year)])
    print(f"日线 {code}/adjust{flag}: {len(merged)} 行", flush=True)
    return {"symbol": code, "adjustflag": flag, "rows": len(merged),
            "last_observed": max(merged) if merged else None,
            "refetched_from": since, "delisted_before_start": last < start}


def collect_stock(task, market=None):
    stage, code, start, end, sessions, basic, overlap_days = task
    if market is None:
        # SDK 在进程内共享连接；每个子进程独占自己的会话，禁止线程共享。
        with Baostock() as own_market:
            return collect_stock(task, own_market)
    return [collect_series(market, Path(stage), code, flag, start, end, sessions, basic, overlap_days)
            for flag in ("1", "3")]


def update_prices(source, previous, destination, start, end, provenance, overlap_days=14, workers=1):
    require(day(start) <= day(end), "起始日期晚于截止日期")
    require(overlap_days >= 1, "回拉天数必须为正数")
    require(1 <= workers <= 4, "日线采集进程数须为1至4")
    codes, pools = selection(source)
    if recovery := provenance.get("rebuild"):
        require(recovery["start"] == start, "重建不能改变原归档起点")
        require(recovery["requested_end"] <= end, "重建不能回退原归档截止日期")
        codes = sorted(set(codes) | {symbol(code) for code in recovery["symbols"]})
    if previous is not None:
        verify_release(previous)
        old = read_json(previous / "config.json")
        require(old["schema_version"] == 1 and old["start"] == start, "历史日线起点/格式不同，请使用新的 HF 路径")
        require(old["requested_end"] <= end, "拒绝将历史日线截止日期回退")
        old_codes = old.get("symbols") or selection(previous)[0]
        codes = sorted(set(codes) | set(old_codes))
    with new_directory(destination) as stage:
        if previous is not None:
            shutil.copytree(previous, stage, dirs_exist_ok=True)
        shutil.copytree(source / "universe", stage / "universe", dirs_exist_ok=True)
        if (source / "index-membership").exists():
            shutil.copytree(source / "index-membership", stage / "index-membership", dirs_exist_ok=True)
        coverage = []
        with Baostock() as market:
            print(f"读取交易日历 {start}—{end}，准备更新 {len(codes)} 只历史成员", flush=True)
            calendar = retry(lambda: market.trade_dates(start, end))
            require(calendar and len({row["calendar_date"] for row in calendar}) == len(calendar), "交易日历为空或日期重复")
            calendar_days = {row["calendar_date"] for row in calendar}
            expected_days = {(day(start) + timedelta(days=i)).isoformat()
                             for i in range((day(end) - day(start)).days + 1)}
            require(calendar_days == expected_days and all(row["is_trading_day"] in ("0", "1") for row in calendar),
                    "交易日历未完整覆盖请求范围")
            sessions = {row["calendar_date"] for row in calendar if row["is_trading_day"] == "1"}
            require(sessions, "请求范围内没有交易日")
            write_rows(stage / "calendar.jsonl", sorted(calendar, key=lambda r: r["calendar_date"]))
            print("读取证券上市/退市元数据", flush=True)
            basics = {row["code"]: row for row in retry(market.stock_basics)}
            write_json(stage / "instruments.json", {code: basics.get(provider_code(code)) for code in codes})

            coverage.append(collect_series(market, stage, INDEX, "3", start, end, sessions, None,
                                           overlap_days, index=True))
            coverage.append(collect_series(market, stage, TOTAL_RETURN_INDEX, "total_return", start, end,
                                           sessions, None, overlap_days, total_return=True))

            tasks = [(stage, code, start, end, sessions, basics.get(provider_code(code)), overlap_days) for code in codes]
            if workers == 1:
                for task in tasks:
                    coverage.extend(collect_stock(task, market))
            else:
                # spawn 不继承父进程的行情 socket；每个证券目录始终只有一个写入者。
                with ProcessPoolExecutor(max_workers=workers, mp_context=get_context("spawn"),
                                         initializer=socket.setdefaulttimeout, initargs=(90,)) as pool:
                    futures = [pool.submit(collect_stock, task) for task in tasks]
                    try:
                        for future in as_completed(futures):
                            coverage.extend(future.result())
                    except BaseException:
                        for future in futures:
                            future.cancel()
                        raise
        _, pools = selection(stage)
        _, indices = membership_selection(stage)
        config = {"schema_version": 1, "start": start, "requested_end": end, "symbols": codes,
                  "last_session": max(sessions), "overlap_days": overlap_days, "pools": pools, "indices": indices,
                  "source": provenance, "price_source": "https://www.baostock.com/",
                  "total_return_source": CSI_INDEX_PERF,
                  "scope": "已发布年度股票池历史成员与官方当前指数完整成分的并集；不代表历史指数成分"}
        write_json(stage / "config.json", config)
        write_json(stage / "coverage.json", sorted(coverage, key=lambda row: (row["symbol"], row["adjustflag"])))
        (stage / "README.md").write_text(
            "# 每日行情归档\n\n包含决策日之后的价格，仅用于离线研究，不得直接作为历史策略输入。\n"
            "未复权 adjust3 与后复权 adjust1 分开存放；H00300 为全收益收盘点位。\n"
            "年度快照决定证券范围，延长行情不等于更新年度选样；不修改已发布评分切片或排名。\n"
            "index-membership 保存中证A50、上证50完整50只成分原件及获取日期；不将当前成分回填到历史。\n"
            "历史观察过的退出成员继续保留。明确停牌时供应商空字段保持为空并标记 issues，不伪造零值。\n"
            "每次回拉近期数据并检查全部交易日；缺行不会当作停牌或前向填充。\n"
            "供应商更早的修正需增大 overlap-days；HF 提交历史保留修正前版本。\n", encoding="utf-8")
        files = {p.relative_to(stage).as_posix(): digest(p) for p in sorted(stage.rglob("*"))
                 if p.is_file() and p.name != "manifest.json"}
        write_json(stage / "manifest.json", {"schema_version": 1, "files": files})
    return {"stocks": len(codes), "pools": pools, "indices": indices,
            "last_session": max(sessions), "series": len(coverage)}
