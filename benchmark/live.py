"""Git 主分支评测编排与只读同步；HF 访问只经 hub，在线 API 不执行策略。"""
import argparse
import signal
import threading
from datetime import datetime, timezone
from pathlib import Path

from .dashboard import render, write_dashboard
from .files import Invalid, digest, read_json, require, write_json
from .hub import fetch_evaluation_plan, fetch_evaluation_raw, live_read, live_write
from .strategy_runner import bench_run, check_source, discover
from .evaluation_data import plan_symbols, rebuild_evaluation

LIVE_PATH = "dashboard/current.json"


def now():
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def prepare(repo, labels_repo, plan_revision, raw_revision, workspace):
    workspace.mkdir(parents=True, exist_ok=True)
    plan = fetch_evaluation_plan(repo, labels_repo, plan_revision, workspace / "plan")
    raw = fetch_evaluation_raw(labels_repo, raw_revision, plan_symbols(workspace / "plan"), workspace / "raw")
    result = {"plan": plan, "raw": raw, "input_origin": "raw_facts"}
    write_json(workspace / "prepared.json", result)
    return result


def evaluate(labels_repo, commit, run_url, workspace, strategies):
    prepared = read_json(workspace / "prepared.json")
    previous = live_read(labels_repo, LIVE_PATH, missing=True)
    state = {"commit": commit, "tag": "raw", "run_url": run_url, "started_at": now(),
             "input_origin": "raw_facts", "sources": prepared,
             "status": "running", "completed": 0, "total": 0, "strategies": []}
    # Last successful scores remain visible while the next complete snapshot is evaluated.
    data = previous or {"strategies": [], "periods": [], "generated_at": None}
    data["evaluation"] = state
    live_write(labels_repo, LIVE_PATH, data)
    try:
        sources = []
        for folder in discover(strategies).values():
            source = (folder / "strategy.py").read_text(encoding="utf-8")
            require(not check_source(source), f"策略静态检查失败: {folder.name}")
            sources.append(source)
        release = workspace / "derived"
        built = rebuild_evaluation(workspace / "plan", workspace / "raw", release, sources)
        state["tag"] = read_json(release / "config.json")["version"]
        out = workspace / "runs"

        def progress(results, total):
            state.update(completed=len(results), total=total, strategies=results.copy(), updated_at=now())
            live_write(labels_repo, LIVE_PATH, data)

        runs = bench_run(strategies, release, out, progress=progress)
        failures = [run for run in runs if run["status"] == "failed"]
        if failures:
            raise Invalid("；".join(f"{item['strategy']}: {item['error']}" for item in failures))
        write_dashboard(out, out, release)
        candidate = read_json(out / "dashboard.json")
        candidate["result_commit"] = commit
        candidate["result_tag"] = state["tag"]
        candidate["result_sources"] = {"input_manifest_sha256": digest(release / "manifest.json"),
                                       "raw_manifest_sha256": built["raw_manifest_sha256"],
                                       "plan_manifest_sha256": built["plan_manifest_sha256"],
                                       "input_origin": "raw_facts", "plan": prepared["plan"], "raw": prepared["raw"]}
        state.update(status="completed", finished_at=now())
        candidate["evaluation"] = state
        live_write(labels_repo, LIVE_PATH, candidate)
        return candidate
    except Exception as exc:
        state.update(status="failed", finished_at=now(), error=str(exc))
        data["evaluation"] = state
        live_write(labels_repo, LIVE_PATH, data)
        raise


def sync(labels_repo, out, cache=None):
    data = live_read(labels_repo, LIVE_PATH, cache=cache)
    if not isinstance(data, dict) or not all(key in data for key in ("strategies", "periods", "evaluation")):
        raise Invalid("在线评测快照格式错误")
    out.mkdir(parents=True, exist_ok=True)
    # Keep page/script local: remote research data can never replace executable website assets.
    for name, text in render(data).items():
        target = out / name
        if target.exists() and target.read_text(encoding="utf-8") == text:
            continue
        temporary = out / (name + ".part")
        temporary.write_text(text, encoding="utf-8")
        temporary.replace(target)


def main(argv=None):
    parser = argparse.ArgumentParser(description="持续评测与研究看板同步")
    parser.add_argument("command", choices=("prepare", "evaluate", "sync"))
    parser.add_argument("--repo", default="lokinko/mario_bench")
    parser.add_argument("--labels-repo", default="lokinko/mario_bench_labels")
    parser.add_argument("--plan-revision", default="pool-scoring-v8", help="只取范围、选样事实和独立标签")
    parser.add_argument("--raw-revision", default="baostock-daily-20261102", help="同源原始行情归档的固定出处")
    parser.add_argument("--workspace", type=Path, default=Path(".runtime/benchmark/workflow"))
    parser.add_argument("--commit")
    parser.add_argument("--run-url", default="")
    parser.add_argument("--strategies", type=Path, default=Path(__file__).with_name("strategies"))
    parser.add_argument("--out", type=Path, default=Path(".runtime/benchmark/current"))
    parser.add_argument("--watch", action="store_true")
    args = parser.parse_args(argv)
    if args.command == "prepare":
        print(prepare(args.repo, args.labels_repo, args.plan_revision, args.raw_revision, args.workspace), flush=True)
        return
    if args.command == "evaluate":
        if not args.commit:
            parser.error("evaluate 必须提供 --commit")
        evaluate(args.labels_repo, args.commit, args.run_url, args.workspace, args.strategies)
        return
    stopping = threading.Event()
    if args.watch:
        for sig in (signal.SIGTERM, signal.SIGINT):
            signal.signal(sig, lambda *_: stopping.set())
    cache = {}
    while not stopping.is_set():
        try:
            sync(args.labels_repo, args.out, cache)
        except Exception as exc:
            if not args.watch:
                raise
            print(f"研究看板同步失败，保留上次结果：{exc}", flush=True)
        if not args.watch or stopping.wait(30):
            break


if __name__ == "__main__":
    main()
