"""每日更新红利/科技历史成员与中证A50/上证50完整成分日线，同步至HF 标签仓库。"""

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
from benchmark.hub import prepare_daily_prices, publish_daily_prices, historical_prices_reference
from benchmark.index_membership import INDICES, collect_memberships


def default_end():
    now = datetime.now(ZoneInfo("Asia/Shanghai"))
    return (now.date() if now.hour >= 18 else now.date() - timedelta(days=1)).isoformat()


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default="lokinko/mario_bench")
    parser.add_argument("--labels-repo", default="lokinko/mario_bench_labels")
    parser.add_argument("--tag", default="pool-scoring-v7", help="作为股票清单来源的不可变评分切片标签")
    parser.add_argument("--path", default="daily-public-prices", help="标签仓库中的独立归档路径")
    parser.add_argument("--start", default="2005-08-01", help="首次回补起点；同一路径后续不能改变")
    parser.add_argument("--end", default=default_end(), help="北京时间截止日期，18 点前默认前一日")
    parser.add_argument("--workers", type=int, default=2, help="公开 HTTP 并发数，1至2")
    parser.add_argument("--out", type=Path, help="可选：保留校验通过的本地归档；上传失败时仍保留，已有目录不可覆盖")
    parser.add_argument("--cache-root", type=Path, default=Path(".runtime/benchmark/daily-public-cache"),
                        help="首次回补的公开响应缓存；常规更新以 HF 归档为准")
    parser.add_argument("--historical-tag", default="baostock-daily-20261102", help="只读引用的旧双口径归档标签")
    parser.add_argument("--refresh-history", action="store_true", help="显式全区间刷新当前供应商历史；平时仅更新独立最新报价")
    parser.add_argument("--rebuild", action="store_true", help="完整重建新来源归档，保留历史名单及日期范围")
    parser.add_argument("--dry-run", action="store_true", help="真实下载、采集和校验，但不上传")
    args = parser.parse_args(argv)
    try:
        if args.path == "daily-prices" or args.path.startswith("daily-prices/"):
            raise ValueError("原Baostock历史归档只读，新供应商必须使用独立路径")
        socket.setdefaulttimeout(90)
        with tempfile.TemporaryDirectory(prefix="mario-daily-prices-") as folder:
            workspace = Path(folder)
            source, previous, parent, provenance = prepare_daily_prices(
                args.repo, args.labels_repo, args.tag, args.path, workspace, rebuild=args.rebuild)
            historical = historical_prices_reference(args.labels_repo, args.historical_tag)
            indices = collect_memberships(source)
            print("完整指数成分: " + json.dumps(indices, ensure_ascii=False), flush=True)
            out = args.out or workspace / "updated"
            result = update_prices(source, previous, out, args.start, args.end, provenance,
                                   args.workers, args.cache_root, historical, args.refresh_history)
            complete = set(result["indices"]) == set(INDICES) and all(
                item["members"] == item["at_last_session"] == 50 for item in result["indices"].values())
            result["indices_current"] = complete
            if not complete or result["unavailable_active"]:
                raise ValueError("当前指数覆盖不足或活跃证券来源不可用，拒绝发布")
            if previous is not None and (previous / "manifest.json").read_bytes() == (out / "manifest.json").read_bytes():
                result["status"] = "unchanged"
            elif args.dry_run:
                result["status"] = "dry_run"
            else:
                result["upload"] = publish_daily_prices(out, args.labels_repo, args.path, parent)
                result["status"] = ("published_with_source_gaps" if result["source_unavailable"]
                                    or result["historical_gap_securities"] else "published")
            print(json.dumps(result, ensure_ascii=False, indent=2))
            summary = os.environ.get("GITHUB_STEP_SUMMARY")
            if summary:
                with Path(summary).open("a", encoding="utf-8") as handle:
                    handle.write("## Benchmark 每日行情\n\n```json\n" + json.dumps(result, ensure_ascii=False, indent=2) + "\n```\n")
        return 0 if complete else 1
    except Exception as exc:
        # SDK/来源错误也必须非零退出；不得把抓取失败报告成更新成功。
        print(f"每日行情更新失败: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
