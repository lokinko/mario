"""已发布股票池历史成员的日线增量归档；不改选样、不生成策略输入或评分标签。"""

import shutil
from datetime import timedelta

from .baostock_market import Baostock, provider_code
from .collect import retry
from .files import (day, digest, new_directory, number, read_json, require,
                    rows, symbol, write_json, write_rows)
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
    return sorted(selected), {pool: sorted(set(years)) for pool, years in sorted(pools.items())}


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
            for key in ("open", "high", "low", "close", "preclose", "volume", "amount"):
                number(row[key])
            if index or row["tradestatus"] == "1":
                for key in ("open", "high", "low", "close"):
                    number(row[key], positive=True)
                require(number(row["low"]) <= min(number(row["open"]), number(row["close"]))
                        <= max(number(row["open"]), number(row["close"])) <= number(row["high"]),
                        f"{code}: OHLC 范围不符 {date}")
        else:
            number(row["close"], positive=True)
        output[date] = row
    return output


def update_prices(source, previous, destination, start, end, provenance, overlap_days=14):
    require(day(start) <= day(end), "起始日期晚于截止日期")
    require(overlap_days >= 1, "回拉天数必须为正数")
    codes, pools = selection(source)
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

            def collect(code, flag, index=False, total_return=False):
                directory = stage / "prices" / code / f"adjust{flag}"
                saved = [row for path in sorted(directory.glob("*.jsonl")) for row in rows(path)]
                existing = checked_rows(saved, code, start, end, sessions, index, total_return)
                first, last = start, end
                if not index and not total_return:
                    basic = basics.get(provider_code(code))
                    require(basic and basic["ipoDate"], f"{code}: 缺少上市日期，不能判定行情完整性")
                    first = max(start, day(basic["ipoDate"]).isoformat())
                    if basic["outDate"]:
                        last = min(end, (day(basic["outDate"]) - timedelta(days=1)).isoformat())
                required = {date for date in sessions if first <= date <= last}
                since = max(first, (day(max(existing)) - timedelta(days=overlap_days)).isoformat()) if existing else first
                missing = sorted(required - set(existing))
                if missing:
                    since = min(since, missing[0])
                fresh = {}
                if required and since <= last:
                    values = retry(lambda: csi_index_closes(code, since, last) if total_return
                                   else market.daily(code, since, last, flag, index=index))
                    fresh = checked_rows(values, code, since, last, sessions, index, total_return)
                    # 回拉窗口必须完整返回，不能用旧数据掩盖供应商本次的缺行。
                    absent = sorted({date for date in required if date >= since} - set(fresh))
                    require(not absent, f"{code}/adjust{flag}: 日线未到齐 {absent[:5]}")
                merged = {**existing, **fresh}
                absent = sorted(required - set(merged))
                require(not absent, f"{code}/adjust{flag}: 历史缺行 {absent[:5]}")
                directory.mkdir(parents=True, exist_ok=True)
                years = sorted({date[:4] for date in merged})
                for year in years:
                    write_rows(directory / f"{year}.jsonl", [merged[date] for date in sorted(merged) if date.startswith(year)])
                coverage.append({"symbol": code, "adjustflag": flag, "rows": len(merged),
                                 "last_observed": max(merged) if merged else None,
                                 "refetched_from": since, "delisted_before_start": last < start})
                print(f"日线 {code}/adjust{flag}: {len(merged)} 行", flush=True)

            for code in codes:
                for flag in ("1", "3"):
                    collect(code, flag)
            collect(INDEX, "3", index=True)
            collect(TOTAL_RETURN_INDEX, "total_return", total_return=True)
        _, pools = selection(stage)
        config = {"schema_version": 1, "start": start, "requested_end": end, "symbols": codes,
                  "last_session": max(sessions), "overlap_days": overlap_days, "pools": pools,
                  "source": provenance, "price_source": "https://www.baostock.com/",
                  "total_return_source": CSI_INDEX_PERF,
                  "scope": "已发布年度股票池历史成员并集；不代表新增年份的年度选样"}
        write_json(stage / "config.json", config)
        write_json(stage / "coverage.json", coverage)
        (stage / "README.md").write_text(
            "# 每日行情归档\n\n包含决策日之后的价格，仅用于离线研究，不得直接作为历史策略输入。\n"
            "未复权 adjust3 与后复权 adjust1 分开存放；H00300 为全收益收盘点位。\n"
            "年度快照决定证券范围，延长行情不等于更新年度选样；不修改已发布评分切片或排名。\n"
            "每次回拉近期数据并检查全部交易日；缺行不会当作停牌或前向填充。\n"
            "供应商更早的修正需增大 overlap-days；HF 提交历史保留修正前版本。\n", encoding="utf-8")
        files = {p.relative_to(stage).as_posix(): digest(p) for p in sorted(stage.rglob("*"))
                 if p.is_file() and p.name != "manifest.json"}
        write_json(stage / "manifest.json", {"schema_version": 1, "files": files})
    return {"stocks": len(codes), "pools": pools, "last_session": max(sessions), "series": len(coverage)}
