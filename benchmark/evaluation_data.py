"""由评分计划和原始事实全量重建策略输入；不读取预计算 inputs 或未来标签。"""

import hashlib
import shutil
from decimal import Decimal

from .feature_pipeline import catalog, enrich, samples_digest
from .dataset import quarter_decisions
from .files import digest, instant, new_directory, read_json, require, write_json, write_rows
from .input_contract import dependency_samples, requirements, stock_indicators, validate_inputs
from .raw_data import read_price_archive
from .release import verify_release


def plan_symbols(plan):
    codes = set()
    for file in sorted((plan / "universe").glob("*.json")):
        snapshot = read_json(file)
        require(snapshot["selected"], f"股票池为空: {file.name}")
        codes.update(snapshot["selected"])
    require(codes, "评分计划缺少股票池")
    return codes


def decision_dates(calendar, year):
    dates = quarter_decisions(calendar, year)
    require(len(dates) == 4, f"评分年度日历不足四个季度: {year}")
    return dates


def raw_samples(plan, raw, unadjusted):
    config = read_json(plan / "config.json")
    samples = []
    for pool, years in sorted(config["pools"].items()):
        for year in years:
            snapshot = read_json(plan / "universe" / f"{pool}-{year}.json")
            require(snapshot["pool"] == pool and snapshot["year"] == year, "选样事实身份不符")
            by_code = {r["symbol"]: r for r in snapshot["candidates"]}
            require(len(by_code) == len(snapshot["candidates"]), "选样证券重复")
            for decision in decision_dates(raw.calendar, year):
                cutoff = decision + "T18:00:00+08:00"
                require(instant(snapshot["available_at"]) <= instant(cutoff), "选样事实晚于决策时点")
                stocks = []
                for code in snapshot["selected"]:
                    fact = by_code[code]
                    require(fact["shares_notice_date"] <= snapshot["selection_date"] < decision, "股本尚未公告或选样时点不符")
                    price, shares = Decimal(fact["close"]), Decimal(fact["total_shares"])
                    require(price.is_finite() and price > 0 and shares.is_finite() and shares > 0, "选样原始收盘/股数无效")
                    bar = unadjusted[code].get(decision)
                    if bar is not None and instant(bar.get("available_at", cutoff)) > instant(cutoff):
                        bar = None
                    stocks.append({"symbol": code, "close": bar["close"] if bar else None,
                                   "trade_status": ("trading" if bar["tradestatus"] == "1" else "suspended") if bar else "no_bar",
                                   "market_cap_at_selection": str(price * shares), "selection_date": snapshot["selection_date"],
                                   "sw_level1_name": fact.get("level1_name")})
                samples.append({"id": f"{pool}-{decision}", "pool": pool, "universe_year": year,
                                "decision_date": decision, "cutoff": cutoff, "stocks": stocks})
    return samples


def rebuild_evaluation(plan, raw_root, destination, strategy_sources):
    """所有衍生字段从空集合生成；只有声明依赖的指标进入样本。"""
    verify_release(plan)
    require(not (plan / "inputs.jsonl").exists(), "评分计划不得包含预计算 inputs.jsonl")
    raw, unadjusted, _ = read_price_archive(raw_root, plan_symbols(plan))
    samples = raw_samples(plan, raw, unadjusted)
    definitions = catalog()
    reports = []
    for pool in sorted({s["pool"] for s in samples}):
        part = [s for s in samples if s["pool"] == pool]
        fields = set()
        for source in strategy_sources:
            applicable = dependency_samples(part, source)
            if applicable:
                for path in stock_indicators(requirements(source)):
                    if path in definitions:
                        fields.add(path)
                    else:
                        validate_inputs(applicable, {"stocks.*." + path: requirements(source)["stocks.*." + path]})
        prepared, report = enrich(part, raw, fields)
        reports.append({"pool": pool, **report})
        by_id = {s["id"]: s for s in prepared}
        samples = [by_id.get(s["id"], s) for s in samples]
    for source in strategy_sources:
        validate_inputs(dependency_samples(samples, source), requirements(source))
    input_sha256 = samples_digest(samples)
    identity = hashlib.sha256((digest(plan / "manifest.json") + digest(raw_root / "manifest.json") + input_sha256).encode()).hexdigest()
    with new_directory(destination) as stage:
        write_rows(stage / "inputs.jsonl", samples)
        config = read_json(plan / "config.json")
        for key in ("input_features", "fugui_features", "base_comparison_tag"):
            config.pop(key, None)
        write_json(stage / "config.json", {**config, "version": "raw-" + identity[:16], "input_origin": "raw_facts"})
        write_json(stage / "features.json", {"builds": reports, "indicators": {k: v for r in reports for k, v in r["indicators"].items()},
                   "plan_manifest_sha256": digest(plan / "manifest.json"), "raw_manifest_sha256": digest(raw_root / "manifest.json"),
                   "input_sha256": input_sha256})
        write_json(stage / "summary.json", {"samples": len(samples), "samples_by_pool": {p: sum(s["pool"] == p for s in samples)
                   for p in config["pools"]}, "pools": config["pools"], "release_ready": False, "input_origin": "raw_facts"})
        shutil.copytree(plan / "universe", stage / "universe")
        # 计算和输入检查结束后才复制标签；计算器不解析标签内容。
        shutil.copyfile(plan / "outcomes.jsonl", stage / "outcomes.jsonl")
        write_json(stage / "manifest.json", {"files": {p.relative_to(stage).as_posix(): digest(p)
                   for p in sorted(stage.rglob("*")) if p.is_file()}})
    return {"samples": len(samples), "input_origin": "raw_facts", "identity": identity,
            "plan_manifest_sha256": digest(plan / "manifest.json"), "raw_manifest_sha256": digest(raw_root / "manifest.json")}
