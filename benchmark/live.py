"""Git 主分支评测编排与只读同步；HF 访问只经 hub，在线 API 不执行策略。"""
import argparse
import re
import signal
import tempfile
import threading
from datetime import datetime, timezone
from pathlib import Path

from .dashboard import render, write_dashboard
from .files import Invalid, read_json, require
from .hub import fetch, live_read, live_write
from .strategy_runner import bench_run

LIVE_PATH = "dashboard/current.json"


def now():
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def evaluate(repo, labels_repo, tag, commit, run_url, workspace, strategies):
    require(re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]*", tag), "评分切片标签格式错误")
    previous = live_read(labels_repo, LIVE_PATH, missing=True)
    state = {"commit": commit, "tag": tag, "run_url": run_url, "started_at": now(),
             "status": "running", "completed": 0, "total": 0, "strategies": []}
    # Last successful scores remain visible while the next complete snapshot is evaluated.
    data = previous or {"strategies": [], "periods": [], "generated_at": None}
    data["evaluation"] = state
    live_write(labels_repo, LIVE_PATH, data)
    try:
        release = workspace / tag
        fetch(repo, labels_repo, "pool-scoring", tag, release)
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
        candidate["result_tag"] = tag
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
    parser.add_argument("command", choices=("evaluate", "sync"))
    parser.add_argument("--repo", default="lokinko/mario_bench")
    parser.add_argument("--labels-repo", default="lokinko/mario_bench_labels")
    parser.add_argument("--tag", default="pool-scoring-v8")
    parser.add_argument("--commit")
    parser.add_argument("--run-url", default="")
    parser.add_argument("--strategies", type=Path, default=Path(__file__).with_name("strategies"))
    parser.add_argument("--out", type=Path, default=Path(".runtime/benchmark/current"))
    parser.add_argument("--watch", action="store_true")
    args = parser.parse_args(argv)
    if args.command == "evaluate":
        if not args.commit:
            parser.error("evaluate 必须提供 --commit")
        with tempfile.TemporaryDirectory(prefix="mario-benchmark-") as folder:
            evaluate(args.repo, args.labels_repo, args.tag, args.commit, args.run_url, Path(folder), args.strategies)
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
