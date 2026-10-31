"""发现 `strategies/<名字>/` 下的策略，静态检查后在子进程中只喂入时点输入，生成仓位、打分并缓存结果。

每个策略目录含 `strategy.md`（首行 `# 标题`，其后是规则说明）与 `strategy.py`（顶层函数 `decide(sample)`，
返回 {证券代码: 仓位}，合计不超过 1，余下为现金）。策略代码只能导入纯计算标准库，不能读写文件、访问网络或反射私有属性；
子进程拿不到结果标签，只从标准输入读取 `inputs.jsonl` 的样本。
"""

import ast
import hashlib
import json
import re
import shutil
import subprocess
import sys
from datetime import datetime
from decimal import ROUND_DOWN, Decimal
from pathlib import Path

from .files import Invalid, digest, read_json, require, rows, write_json, write_rows
from .public_sources import SHANGHAI
from .release import verify_release
from .scoring import evaluate_weights

NAME = re.compile(r"[a-z][a-z0-9_]{0,39}")
ALLOWED_IMPORTS = ("bisect", "collections", "decimal", "functools", "heapq", "itertools", "math", "operator",
                   "statistics")
BLOCKED_NAMES = {"open", "exec", "eval", "compile", "__import__", "globals", "locals", "vars", "getattr", "setattr",
                 "delattr", "input", "breakpoint", "help", "memoryview", "exit", "quit"}
WEIGHT_STEP = Decimal("0.00000001")
TIMEOUT_SECONDS = 600
ENGINE_FILES = ("scoring.py", "strategy_runner.py")

SANDBOX = r"""
import builtins, json, sys
from decimal import Decimal

allowed, blocked = set(json.loads(sys.argv[2])), set(json.loads(sys.argv[3]))
real_import = builtins.__import__

def guarded_import(name, globals=None, locals=None, fromlist=(), level=0):
    if level or name.split(".")[0] not in allowed:
        raise ImportError(f"策略不允许导入 {name}")
    return real_import(name, globals, locals, fromlist, level)

safe = {k: v for k, v in vars(builtins).items() if k not in blocked}
safe["__import__"] = guarded_import
samples = [json.loads(line) for line in sys.stdin if line.strip()]
source = open(sys.argv[1], encoding="utf-8").read()
namespace = {"__builtins__": safe, "__name__": "strategy"}
exec(compile(source, "strategy.py", "exec"), namespace)
decide = namespace["decide"]

def amount(value):
    if isinstance(value, bool) or not isinstance(value, (int, float, str, Decimal)):
        raise TypeError(f"仓位必须是数值，收到 {type(value).__name__}")
    return str(Decimal(str(value)) if isinstance(value, float) else Decimal(value))

for sample in samples:
    chosen = decide(sample) or {}
    if not isinstance(chosen, dict):
        raise TypeError(f"{sample['id']}: decide 必须返回 dict，收到 {type(chosen).__name__}")
    print(json.dumps({"sample_id": sample["id"], "weights": {str(k): amount(v) for k, v in chosen.items()}}))
"""


def check_source(source):
    """返回违规列表；为空表示通过。"""
    try:
        tree = ast.parse(source)
    except SyntaxError as exc:
        return [f"语法错误（第 {exc.lineno} 行）：{exc.msg}"]
    problems = []
    for node in ast.walk(tree):
        where = f"第 {getattr(node, 'lineno', '?')} 行"
        if isinstance(node, ast.Import):
            problems += [f"{where} 不允许导入 {a.name}" for a in node.names if a.name.split(".")[0] not in ALLOWED_IMPORTS]
        elif isinstance(node, ast.ImportFrom):
            if node.level or (node.module or "").split(".")[0] not in ALLOWED_IMPORTS:
                problems.append(f"{where} 不允许导入 {node.module}")
        elif isinstance(node, ast.Name) and (node.id in BLOCKED_NAMES or node.id.startswith("__")):
            problems.append(f"{where} 不允许使用 {node.id}")
        elif isinstance(node, ast.Attribute) and node.attr.startswith("_"):
            problems.append(f"{where} 不允许访问私有属性 {node.attr}")
    if not any(isinstance(n, ast.FunctionDef) and n.name == "decide" for n in tree.body):
        problems.append("缺少顶层函数 decide(sample)")
    return problems


def describe(markdown):
    lines = markdown.strip().splitlines()
    require(lines and lines[0].startswith("# ") and lines[0][2:].strip(), "strategy.md 首行必须是 `# 标题`")
    body = "\n".join(lines[1:]).strip()
    require(body, "strategy.md 缺少策略说明")
    return lines[0][2:].strip(), body


def discover(directory):
    """返回 {名字: 策略目录}；目录名即策略名。"""
    found = {}
    for item in sorted(Path(directory).iterdir()):
        if item.is_dir() and not item.name.startswith((".", "_")):
            require(NAME.fullmatch(item.name), f"策略名须为小写字母开头的 snake_case：{item.name}")
            found[item.name] = item
    require(found, f"{directory} 下没有策略")
    return found


def generate_weights(code_path, samples, timeout=TIMEOUT_SECONDS):
    payload = "".join(json.dumps(s, ensure_ascii=False) + "\n" for s in samples)
    try:
        done = subprocess.run([sys.executable, "-I", "-c", SANDBOX, str(code_path), json.dumps(ALLOWED_IMPORTS),
                               json.dumps(sorted(BLOCKED_NAMES - {"__import__"}))],
                              input=payload, capture_output=True, text=True, timeout=timeout,
                              cwd=Path(code_path).parent, env={"PYTHONUTF8": "1"})
    except subprocess.TimeoutExpired as exc:
        raise Invalid(f"策略运行超过 {timeout} 秒") from exc
    require(done.returncode == 0, "策略运行失败：\n" + "\n".join(done.stderr.strip().splitlines()[-8:]))
    output = []
    for line in done.stdout.splitlines():
        row = json.loads(line)
        for code, weight in row["weights"].items():
            value = Decimal(weight).quantize(WEIGHT_STEP, rounding=ROUND_DOWN)
            if value:
                output.append({"sample_id": row["sample_id"], "symbol": code, "weight": str(value)})
    return output


def fingerprint(code_path, release):
    engine = Path(__file__).parent
    parts = [digest(code_path), digest(release / "manifest.json"), *(digest(engine / name) for name in ENGINE_FILES)]
    return hashlib.sha256("\n".join(parts).encode()).hexdigest()


def run_one(name, folder, release, samples, outcomes, target):
    """成功时 target 下有 weights.jsonl、scorecard.json、run.json；失败只留下记录错误的 run.json。"""
    code_path = folder / "strategy.py"
    record = {"strategy": name, "fingerprint": None, "ran_at": datetime.now(SHANGHAI).isoformat(timespec="seconds")}
    try:
        title, description = describe((folder / "strategy.md").read_text(encoding="utf-8"))
        source = code_path.read_text(encoding="utf-8")
        problems = check_source(source)
        require(not problems, "静态检查未通过：" + "；".join(problems))
        record.update(title=title, description=description, fingerprint=fingerprint(code_path, release))
        weights = generate_weights(code_path, samples)
        report = evaluate_weights(samples, outcomes, weights)
    except (Invalid, OSError, KeyError, ValueError, ArithmeticError) as exc:
        shutil.rmtree(target, ignore_errors=True)
        target.mkdir(parents=True)
        write_json(target / "run.json", {**record, "status": "failed", "error": str(exc)})
        return {"strategy": name, "status": "failed", "error": str(exc)}
    stage = target.with_name(f".{name}.part")
    shutil.rmtree(stage, ignore_errors=True)
    stage.mkdir(parents=True)
    write_rows(stage / "weights.jsonl", weights)
    write_json(stage / "scorecard.json", report)
    write_json(stage / "run.json", {**record, "status": "scored", "score": report["score"]["mean"]})
    shutil.rmtree(target, ignore_errors=True)
    stage.rename(target)
    return {"strategy": name, "status": "scored", "score": report["score"]["mean"]}


def bench_run(strategies_dir, release, out, force=False):
    """对目录中全部策略生成仓位并打分；代码、发布包与打分引擎都未变的策略沿用缓存。已删除策略的结果一并清理。"""
    verify_release(release)
    strategies = discover(strategies_dir)
    out.mkdir(parents=True, exist_ok=True)
    for stale in out.iterdir():
        if stale.is_dir() and stale.name not in strategies and (stale / "run.json").exists():
            shutil.rmtree(stale)
    samples, outcomes, results = None, None, []
    for name, folder in strategies.items():
        target, previous = out / name, out / name / "run.json"
        try:
            cached = read_json(previous) if not force and previous.exists() else None
            if cached and cached["status"] == "scored" and \
                    cached["fingerprint"] == fingerprint(folder / "strategy.py", release):
                title, description = describe((folder / "strategy.md").read_text(encoding="utf-8"))
                if (cached["title"], cached["description"]) != (title, description):
                    write_json(previous, {**cached, "title": title, "description": description})
                results.append({"strategy": name, "status": "cached", "score": cached["score"]})
                continue
        except (Invalid, OSError):
            pass
        if samples is None:
            samples, outcomes = rows(release / "inputs.jsonl"), rows(release / "outcomes.jsonl")
        results.append(run_one(name, folder, release, samples, outcomes, target))
    return results


def completed_runs(out):
    """{策略名: {"title", "description", "scorecard"}}，只含打分成功的策略。"""
    output = {}
    for item in sorted(Path(out).iterdir()):
        record = item / "run.json"
        if item.is_dir() and record.exists() and read_json(record)["status"] == "scored":
            meta = read_json(record)
            output[item.name] = {"title": meta["title"], "description": meta["description"],
                                 "scorecard": read_json(item / "scorecard.json")}
    return output


def failed_runs(out):
    return {item.name: read_json(item / "run.json")["error"] for item in sorted(Path(out).iterdir())
            if item.is_dir() and (item / "run.json").exists() and read_json(item / "run.json")["status"] == "failed"}
