"""按指标能力组合原始数据；快照标识只用于追溯，不参与兼容判断。"""

import copy
import hashlib
import json
import shutil
from dataclasses import dataclass
from decimal import Decimal
from pathlib import Path

from .files import day, digest, new_directory, read_json, require, rows, write_json, write_rows
from .input_contract import valid_value
from .price_indicators import (LONG_FIELDS, REGRESSION_FIELDS, long_features, momentum,
                               monthly_features, regression_features, weekly_features, volatility)
from .raw_data import PRICE_BASIS, read_raw
from .release import verify_release
from .technical_indicators import FIELDS as TECHNICAL_FIELDS, fugui_features


def price_features(bars, calendar, decision):
    output = {"momentum_63": momentum(bars, calendar, decision),
            "return_20": momentum(bars, calendar, decision, 20),
            "volatility_63": volatility(bars, calendar, decision)}
    return {**output, "issues": {k: "insufficient_or_unavailable_price_history" for k, v in output.items() if v is None}}


@dataclass(frozen=True)
class Indicator:
    """一个有稳定语义的输出字段；新增定义不要求重命名整套输入。"""

    path: str
    kind: str
    unit: str
    calculate: object
    price_basis: str = PRICE_BASIS
    dependencies: tuple = ("adjusted_daily_bars", "market_calendar")


INDICATORS = (
    *(Indicator(k, "decimal?", "ratio", price_features) for k in ("momentum_63", "return_20", "volatility_63")),
    *(Indicator("long_history." + k, "decimal?", "ratio", long_features) for k in LONG_FIELDS),
    Indicator("long_history.volatility_3y_weekly", "decimal?", "annualized_ratio", weekly_features),
    Indicator("long_history.daily_returns_1m", "returns", "daily_ratio", monthly_features),
    *(Indicator("long_history." + k, "decimal?", "r_squared" if "r2" in k else "annualized_percent_times_r_squared",
                regression_features) for k in REGRESSION_FIELDS),
    *(Indicator("fugui." + k, "decimal?" if k == "monthly_upper_return" or k.startswith("monthly_macd_") and k != "monthly_macd_negative" else "boolean?",
                "ratio" if k == "monthly_upper_return" else "adjusted_price" if k.startswith("monthly_macd_") and k != "monthly_macd_negative" else "condition",
                fugui_features) for k in TECHNICAL_FIELDS),
)


def catalog(definitions=INDICATORS):
    result = {item.path: item for item in definitions}
    require(len(result) == len(definitions), "指标输出路径重复")
    return result


def describe_indicators(definitions=INDICATORS):
    # Hash all pure calculation modules, including transitive helpers; no release-version switch.
    code = hashlib.sha256("\n".join(digest(Path(__file__).with_name(name)) for name in
                                   ("feature_pipeline.py", "price_indicators.py", "technical_indicators.py")).encode()).hexdigest()
    return {item.path: {"kind": item.kind, "unit": item.unit, "dependencies": list(item.dependencies),
                        "price_basis": item.price_basis, "calculator": item.calculate.__name__, "code_sha256": code}
            for item in definitions}


def value_at(record, path):
    for key in path.split("."):
        if not isinstance(record, dict) or key not in record:
            return False, None
        record = record[key]
    return True, record


def put(record, path, value):
    *parents, key = path.split(".")
    for parent in parents:
        record = record.setdefault(parent, {})
        require(isinstance(record, dict), f"指标命名空间不是对象: {path}")
    record[key] = value


def enrich(samples, raw, fields, definitions=INDICATORS):
    """只补不存在的指标；明确的 null 是计算结果，不从其他来源隐式覆盖。"""
    available = catalog(definitions)
    requested = sorted(set(fields))
    output, calculated, cache = copy.deepcopy(samples), {}, {}
    for sample in output:
        for stock in sample["stocks"]:
            for path in requested:
                if value_at(stock, path)[0]:
                    continue
                context = f"{sample['id']}/{stock['symbol']}/{path}"
                require(path in available, f"unknown_indicator: {context}")
                item = available[path]
                require(raw.price_basis == item.price_basis, f"incompatible_price_basis: {context}: {raw.price_basis}")
                key = stock["symbol"], sample["decision_date"], sample.get("cutoff"), item.calculate
                if key not in cache:
                    bars, days = raw.at(stock["symbol"], sample["decision_date"], sample.get("cutoff"))
                    for common, sessions in (("momentum_63", 63), ("return_20", 20)):
                        if stock.get(common) is not None:
                            computed = momentum(bars, days, sample["decision_date"], sessions)
                            require(computed is not None and Decimal(computed) == Decimal(stock[common]),
                                    f"raw_input_mismatch: {context}/{common}")
                    cache[key] = item.calculate(bars, days, sample["decision_date"])
                result = cache[key]
                field = path.split(".")[-1]
                require(field in result, f"calculator_missing_output: {context}")
                require(valid_value(result[field], item.kind), f"invalid_calculator_output: {context}: {item.kind}")
                put(stock, path, result[field])
                namespace = path.rsplit(".", 1)[0] + "." if "." in path else ""
                if field in result.get("issues", {}):
                    put(stock, namespace + "issues." + field, result["issues"][field])
                calculated[path] = calculated.get(path, 0) + 1
    return output, {"calculated": calculated, "requested": requested,
                    "sources": raw.provenance, "indicators": {k: v for k, v in describe_indicators(definitions).items() if k in calculated}}


def build_features(release, raw_root, destination, fields=None, built_on=None):
    """指标先计算，标签最后原样复制；来源包和已有切片永不覆盖。"""
    verify_release(release)
    if built_on is not None:
        day(built_on)
    raw = read_raw(raw_root)
    samples, report = enrich(rows(release / "inputs.jsonl"), raw, fields if fields is not None else catalog())
    report["source_manifest_sha256"] = digest(release / "manifest.json")
    with new_directory(destination) as stage:
        for path in release.rglob("*"):
            relative = path.relative_to(release)
            if path.is_file() and relative.as_posix() not in ("manifest.json", "inputs.jsonl", "features.json"):
                (stage / relative).parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(path, stage / relative)
        write_rows(stage / "inputs.jsonl", samples)
        prior = read_json(release / "features.json") if (release / "features.json").exists() else {}
        write_json(stage / "features.json", {"indicators": {**prior.get("indicators", {}), **report["indicators"]},
                   "build": report, "parent_features_sha256": digest(release / "features.json") if prior else None})
        if (stage / "config.json").exists():
            config = read_json(stage / "config.json")
            # Old feature bundle labels are historical metadata, never a capability gate.
            for key in ("input_features", "fugui_features", "base_comparison_tag"):
                config.pop(key, None)
            if built_on:
                config["built_on"] = built_on
            write_json(stage / "config.json", config)
        write_json(stage / "manifest.json", {"files": {p.relative_to(stage).as_posix(): digest(p)
                   for p in sorted(stage.rglob("*")) if p.is_file()}})
    verify_release(destination)
    require(digest(destination / "outcomes.jsonl") == digest(release / "outcomes.jsonl"), "标签被改动")
    return report


def samples_digest(samples):
    return hashlib.sha256(json.dumps(samples, sort_keys=True, ensure_ascii=False, allow_nan=False).encode()).hexdigest()
