"""策略声明所需能力；允许额外字段，只校验实际消费的路径和类型。"""

import ast
from datetime import date
from decimal import Decimal

from .files import Invalid, require

KINDS = {"decimal", "boolean", "text", "returns", "object"}


def dependency_samples(samples, source):
    """显式池范围外无需计算专属指标；池标识本身仍需存在。"""
    nodes = [n for n in ast.parse(source).body if isinstance(n, ast.Assign)
             and any(isinstance(t, ast.Name) and t.id == "REQUIRES_POOLS" for t in n.targets)]
    if not nodes:
        return samples
    require(len(nodes) == 1, "REQUIRES_POOLS 只能声明一次")
    try:
        pools = ast.literal_eval(nodes[0].value)
    except (ValueError, TypeError) as exc:
        raise Invalid("REQUIRES_POOLS 必须为静态股票池列表") from exc
    require(isinstance(pools, (tuple, list)) and pools and all(isinstance(p, str) for p in pools), "股票池范围错误")
    validate_inputs(samples, {"pool": "text"})
    return [sample for sample in samples if sample["pool"] in pools]


def requirements(source):
    declarations = [node for node in ast.parse(source).body if isinstance(node, ast.Assign)
                    and any(isinstance(t, ast.Name) and t.id == "REQUIRES" for t in node.targets)]
    require(len(declarations) <= 1, "REQUIRES 只能声明一次")
    if not declarations:
        return {}
    try:
        value = ast.literal_eval(declarations[0].value)
    except (ValueError, TypeError) as exc:
        raise Invalid("REQUIRES 必须为静态路径/类型字典") from exc
    require(isinstance(value, dict), "REQUIRES 必须为路径/类型字典")
    for path, kind in value.items():
        require(isinstance(path, str) and path and all(part and not part.startswith("_") for part in path.split(".")),
                f"无效依赖路径: {path}")
        require(isinstance(kind, str) and kind.removesuffix("?") in KINDS, f"无效依赖类型: {kind}")
    return value


def valid_value(value, kind):
    if value is None:
        return kind.endswith("?")
    kind = kind.rstrip("?")
    if kind == "decimal":
        return isinstance(value, str) and Decimal(value).is_finite()
    if kind == "boolean":
        return type(value) is bool
    if kind == "text":
        return isinstance(value, str)
    if kind == "returns":
        return isinstance(value, dict) and all(date.fromisoformat(k).isoformat() == k
                    and (v is None or valid_value(v, "decimal")) for k, v in value.items())
    return isinstance(value, dict)


def validate_inputs(samples, required):
    problems = []

    def inspect(value, parts, kind, location):
        if not parts:
            try:
                valid = valid_value(value, kind)
            except (ValueError, TypeError, ArithmeticError):
                valid = False
            if not valid:
                problems.append(f"invalid_indicator_type: {location}: 需要 {kind}")
            return
        head, *tail = parts
        if head == "*":
            if not isinstance(value, list):
                problems.append(f"invalid_input_collection: {location}")
            else:
                for index, child in enumerate(value):
                    inspect(child, tail, kind, location + "/" + str(child.get("symbol", index) if isinstance(child, dict) else index))
        elif not isinstance(value, dict) or head not in value:
            problems.append(f"missing_indicator: {location}/{'.'.join(parts)}")
        else:
            inspect(value[head], tail, kind, location + "/" + head)

    for sample in samples:
        for path, kind in required.items():
            inspect(sample, path.split("."), kind, str(sample.get("id", "unknown_sample")))
    require(not problems, f"输入依赖不满足（{len(problems)} 项）：" + "；".join(problems[:12]))


def stock_indicators(required):
    return {path[len("stocks.*."):] for path in required if path.startswith("stocks.*.")}
