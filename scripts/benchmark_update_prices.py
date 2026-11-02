"""每日更新 benchmark 已发布股票池成员的日线，并提交到私有 HF 标签仓库。"""

import argparse
import json
import os
import socket
import sys
import tempfile
from datetime import datetime, timedelta
from pathlib import Path
from zoneinfo import ZoneInfo

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from benchmark.daily_prices import update_prices
from benchmark.hub import prepare_daily_prices, publish_daily_prices


def default_end():
    now = datetime.now(ZoneInfo("Asia/Shanghai"))
    return (now.date() if now.hour >= 18 else now.date() - timedelta(days=1)).isoformat()


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default="lokinko/mario_bench")
    parser.add_argument("--labels-repo", default="lokinko/mario_bench_labels")
    parser.add_argument("--tag", default="pool-scoring-v7", help="作为股票清单来源的不可变评分切片标签")
    parser.add_argument("--path", default="daily-prices", help="标签仓库中的独立归档路径")
    parser.add_argument("--start", default="2005-08-01", help="首次回补起点；同一路径后续不能改变")
    parser.add_argument("--end", default=default_end(), help="北京时间截止日期，18 点前默认前一日")
    parser.add_argument("--overlap-days", type=int, default=14)
    parser.add_argument("--dry-run", action="store_true", help="真实下载、采集和校验，但不上传")
    args = parser.parse_args(argv)
    try:
        socket.setdefaulttimeout(90)
        with tempfile.TemporaryDirectory(prefix="mario-daily-prices-") as folder:
            workspace = Path(folder)
            source, previous, parent, provenance = prepare_daily_prices(
                args.repo, args.labels_repo, args.tag, args.path, workspace)
            out = workspace / "updated"
            result = update_prices(source, previous, out, args.start, args.end, provenance, args.overlap_days)
            if previous is not None and (previous / "manifest.json").read_bytes() == (out / "manifest.json").read_bytes():
                result["status"] = "unchanged"
            elif args.dry_run:
                result["status"] = "dry_run"
            else:
                result["upload"] = publish_daily_prices(out, args.labels_repo, args.path, parent)
                result["status"] = "published"
            print(json.dumps(result, ensure_ascii=False, indent=2))
            summary = os.environ.get("GITHUB_STEP_SUMMARY")
            if summary:
                with Path(summary).open("a", encoding="utf-8") as handle:
                    handle.write("## Benchmark 每日行情\n\n```json\n" + json.dumps(result, ensure_ascii=False, indent=2) + "\n```\n")
        return 0
    except Exception as exc:
        # SDK/来源错误也必须非零退出；不得把抓取失败报告成更新成功。
        print(f"每日行情更新失败: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
