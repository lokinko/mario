import argparse
import json
import os
import sys
from pathlib import Path

from .dataset import Dataset, TABLES, validate_config
from .evaluate import baseline, evaluate
from .files import new_directory, read_json, require, rows, write_json, write_rows
from .release import build, verify_release
from .public_sources import probe_public
from .collect import collect_public, repair_catalogs
from .sources import Tushare, collect_prices, collect_universe, fetch_documents
from .tech_universe import collect_tech_universe
from .dividend_universe import collect_dividend_universe
from .hub import fetch, publish
from .dashboard import dashboard_server, write_dashboard
from .pool_release import build_pool_release
from .scoring import evaluate_weights
from .strategy_runner import bench_run
from .feature_pipeline import build_features, describe_indicators
from .raw_data import import_raw, read_raw

DEFAULT_CONFIG = Path(__file__).with_name("config.json")
DEFAULT_STRATEGIES = Path(__file__).with_name("strategies")


def main(argv=None):
    parser = argparse.ArgumentParser(description="中国红利 top-20 离线研究 benchmark")
    sub = parser.add_subparsers(dest="command", required=True)
    init = sub.add_parser("init", help="创建本地输入目录，不伪造任何市场数据")
    init.add_argument("--root", type=Path, required=True)
    init.add_argument("--config", type=Path, default=DEFAULT_CONFIG)
    probe = sub.add_parser("probe-public", help="无 token 免费来源覆盖实测，结果不冒充完整 benchmark")
    probe.add_argument("--config", type=Path, default=DEFAULT_CONFIG)
    probe.add_argument("--out", type=Path, required=True)
    probe.add_argument("--report-symbol", default="600036.SH")
    collect = sub.add_parser("collect-public", help="免费来源批量补采与断点恢复，不发布、不伪造历史股票池")
    collect.add_argument("--root", type=Path, required=True)
    collect.add_argument("--symbols-file", type=Path, required=True)
    collect.add_argument("--config", type=Path, default=DEFAULT_CONFIG)
    collect.add_argument("--workers", type=int, default=4)
    repair = sub.add_parser("repair-catalogs", help="对缺失财报期取消分类过滤、按披露窗口定向补查")
    repair.add_argument("--root", type=Path, required=True)
    repair.add_argument("--config", type=Path, default=DEFAULT_CONFIG)
    tech = sub.add_parser("tech-universe", help="免费来源按时点选出科技股池候选与前 N，保留待核对状态")
    tech.add_argument("--root", type=Path, required=True)
    tech.add_argument("--years", required=True, help="逗号分隔的股票池年份，如 2008,2015,2021")
    tech.add_argument("--top", type=int, default=20)
    tech.add_argument("--workers", type=int, default=4)
    tech.add_argument("--sw-file", type=Path, help="本地下载的申万个股行业变更 xls；站点证书链不完整时使用")
    dividend = sub.add_parser("dividend-universe", help="重建中证红利选样日成分并按市值选出前 N，保留待核对状态")
    dividend.add_argument("--root", type=Path, required=True)
    dividend.add_argument("--years", required=True, help="逗号分隔的股票池年份，从 2009 起")
    dividend.add_argument("--top", type=int, default=20)
    dividend.add_argument("--workers", type=int, default=4)
    pool_build = sub.add_parser("pool-build", help="由一个或多个年度股票池构建时点输入与结果分离的可评分切片")
    pool_build.add_argument("--universe-root", type=Path, action="append", required=True,
                            help="可重复；每个目录是一个股票池的采集结果")
    pool_build.add_argument("--out", type=Path, required=True)
    bench = sub.add_parser("bench-run", help="运行 strategies/ 下全部策略：静态检查、生成仓位、打分（未改动的沿用缓存）并刷新看板")
    bench.add_argument("--release", type=Path, required=True, help="pool-build 生成的评分切片")
    bench.add_argument("--strategies", type=Path, default=DEFAULT_STRATEGIES)
    bench.add_argument("--out", type=Path, help="默认 .runtime/benchmark/bench/<发布包目录名>")
    bench.add_argument("--force", action="store_true", help="忽略缓存全部重跑")
    bench.add_argument("--raw", type=Path, help="原始归档；按策略声明补算缺少的指标")
    sub.add_parser("indicators", help="列出可组合指标、依赖、类型、口径和计算指纹")
    raw = sub.add_parser("raw-import", help="将已有来源缓存整理为独立原始归档，不计算指标或联网")
    raw.add_argument("--universe-root", type=Path, action="append", default=[])
    raw.add_argument("--history", type=Path)
    raw.add_argument("--calendar", type=Path)
    raw.add_argument("--release", type=Path, help="仅用于确定需要导入的证券范围")
    raw.add_argument("--out", type=Path, required=True)
    features = sub.add_parser("features-build", help="由原始归档按字段组合衍生指标，不要求特定切片版本")
    features.add_argument("--release", type=Path, required=True)
    features.add_argument("--raw", type=Path, required=True)
    features.add_argument("--field", action="append", help="可重复；不指定则补全所有注册指标")
    features.add_argument("--built-on")
    features.add_argument("--out", type=Path, required=True)
    serve = sub.add_parser("bench-serve", help="在本机提供看板；bench-run 重新生成后已打开的页面自动载入")
    serve.add_argument("--release", type=Path, required=True, help="pool-build 生成的评分切片")
    serve.add_argument("--out", type=Path, help="默认 .runtime/benchmark/bench/<发布包目录名>")
    serve.add_argument("--port", type=int, default=8765)
    pool_score = sub.add_parser("pool-score", help="给外部生成的仓位文件（如 Agent 输出）按主指标打分")
    pool_score.add_argument("--weights", type=Path, required=True)
    pool_score.add_argument("--root", type=Path, required=True)
    pool_score.add_argument("--out", type=Path, required=True)
    hub = sub.add_parser("hub-publish", help="把评分切片上传到 Hugging Face 数据集仓库：输入与标签分仓")
    hub.add_argument("--release", type=Path, required=True)
    hub.add_argument("--runs", type=Path, required=True, help="bench-run 输出目录")
    hub.add_argument("--repo", required=True)
    hub.add_argument("--labels-repo", required=True)
    hub.add_argument("--path", default="pool-scoring")
    hub.add_argument("--tag")
    hub_fetch = sub.add_parser("hub-fetch", help="从输入与标签仓库下载某个标签的评分切片，合并并校验哈希；目录已存在且校验通过时直接沿用")
    hub_fetch.add_argument("--repo", required=True)
    hub_fetch.add_argument("--labels-repo", required=True)
    hub_fetch.add_argument("--path", default="pool-scoring")
    hub_fetch.add_argument("--tag", required=True)
    hub_fetch.add_argument("--out", type=Path, required=True)
    universe = sub.add_parser("collect-universe", help="采集历史成分和总市值提案，保留待核对状态")
    universe.add_argument("--root", type=Path, required=True)
    universe.add_argument("--provider-index-code", default="000922.SH")
    for name in ("plan", "validate", "collect-prices", "fetch-documents", "verify"):
        command = sub.add_parser(name)
        command.add_argument("--root", type=Path, required=True)
    builder = sub.add_parser("build", help="校验后构建 Hugging Face 可识别的本地发布包")
    builder.add_argument("--root", type=Path, required=True)
    builder.add_argument("--out", type=Path, required=True)
    base = sub.add_parser("baseline")
    score = sub.add_parser("evaluate")
    for command in (base, score):
        command.add_argument("--root", type=Path, required=True)
        command.add_argument("--split", choices=("train", "validation", "test"), required=True)
        command.add_argument("--horizon", type=int, default=63)
        command.add_argument("--out", type=Path, required=True)
    score.add_argument("--predictions", type=Path, required=True)
    score.add_argument("--top-k", type=int, default=5)
    args = parser.parse_args(argv)
    try:
        if args.command == "indicators":
            result = describe_indicators()
        elif args.command == "raw-import":
            symbols = {s["symbol"] for sample in rows(args.release / "inputs.jsonl") for s in sample["stocks"]} if args.release else None
            result = import_raw(args.out, roots=args.universe_root, history=args.history,
                                calendar_path=args.calendar, symbols=symbols)
        elif args.command == "features-build":
            result = build_features(args.release, args.raw, args.out, args.field, args.built_on)
        elif args.command == "tech-universe":
            years = [int(value) for value in args.years.split(",") if value.strip()]
            result = collect_tech_universe(args.root, years, args.top, args.workers, args.sw_file)
        elif args.command == "dividend-universe":
            years = [int(value) for value in args.years.split(",") if value.strip()]
            result = collect_dividend_universe(args.root, years, args.top, args.workers)
        elif args.command == "bench-run":
            out = args.out or Path(".runtime/benchmark/bench") / args.release.resolve().name
            runs = bench_run(args.strategies, args.release, out, args.force, raw=read_raw(args.raw) if args.raw else None)
            board = write_dashboard(out, out, args.release)
            print(json.dumps({"runs": runs, "dashboard": board["dashboard"]}, ensure_ascii=False, indent=2))
            return 0 if all(run["status"] != "failed" for run in runs) else 1
        elif args.command == "bench-serve":
            out = args.out or Path(".runtime/benchmark/bench") / args.release.resolve().name
            with dashboard_server(out, args.port) as server:
                print(f"看板地址：http://127.0.0.1:{server.server_port}/dashboard（Ctrl+C 停止）", flush=True)
                try:
                    server.serve_forever()
                except KeyboardInterrupt:
                    pass
            return 0
        elif args.command == "hub-publish":
            result = publish(args.release, args.repo, args.labels_repo, args.path, args.runs, args.tag)
        elif args.command == "hub-fetch":
            result = fetch(args.repo, args.labels_repo, args.path, args.tag, args.out)
        elif args.command == "pool-build":
            result = build_pool_release(args.universe_root, args.out)
        elif args.command == "pool-score":
            verify_release(args.root)
            require(not args.out.exists(), "输出文件已存在，请使用新路径")
            samples = rows(args.root / "inputs.jsonl")
            args.out.parent.mkdir(parents=True, exist_ok=True)
            result = evaluate_weights(samples, rows(args.root / "outcomes.jsonl"), rows(args.weights))
            write_json(args.out, result)
            result = {key: result[key] for key in ("metric", "score", "return_score", "win_rate",
                                                   "pool_equal_weight_score", "csi300_score", "beat_csi300_periods",
                                                   "beat_pool_equal_weight_periods", "by_pool")}
        elif args.command == "repair-catalogs":
            result = repair_catalogs(args.root, args.config)
        elif args.command == "collect-public":
            result = collect_public(args.config, args.symbols_file, args.root, args.workers)
        elif args.command == "probe-public":
            result = probe_public(args.config, args.out, args.report_symbol)
        elif args.command == "init":
            config = read_json(args.config)
            validate_config(config)
            with new_directory(args.root) as stage:
                write_json(stage / "config.json", config)
                (stage / "documents").mkdir()
            result = {"root": str(args.root.resolve()), "status": "empty", "next": "plan"}
        elif args.command == "plan":
            dataset = Dataset(args.root)
            config = dataset.config
            result = {"status": "pending_acquisition", "start": config["start"], "end": config["end"],
                      "tushare_token_configured": bool(os.environ.get("TUSHARE_TOKEN")),
                      "counts": {name: len(dataset.tables[name]) for name in TABLES},
                      "expected_annual_snapshots": int(config["end"][:4]) - int(config["start"][:4]) + 1,
                      "minimum_company_quarter_reports": 20 * 4 * (int(config["end"][:4]) - int(config["start"][:4]) + 1),
                      "etfs": config["etfs"],
                      "preferred_source": "免费公开来源，先运行 probe-public 核对覆盖；Tushare 为可选适配器",
                      "required": ["历史完整中证红利候选池、成分披露/生效证据和年末总市值",
                                   "股票及 ETF 上市/退市元数据，日历、日线和复权因子",
                                   "每季度财报原件目录及原始/修订版实际披露时间",
                                   "供应商遗漏与停牌逐项核对；不得自动填充",
                                   "发布前独立核对各来源再分发权限"]}
        elif args.command == "validate":
            result = Dataset(args.root).validate()
            print(json.dumps(result, ensure_ascii=False, indent=2))
            return 0 if result["valid"] else 1
        elif args.command == "collect-universe":
            collect_universe(args.root, Tushare(args.root / "cache"), args.provider_index_code)
            result = {"status": "requires_membership_review", "next": "核对历史成分公告、生效范围、年末市值与信息时点"}
        elif args.command == "collect-prices":
            collect_prices(args.root, Tushare(args.root / "cache"))
            result = {"status": "collected", "next": "validate；核对遗漏和停牌"}
        elif args.command == "fetch-documents":
            fetch_documents(args.root)
            result = {"status": "downloaded", "next": "validate"}
        elif args.command == "build":
            result = build(args.root, args.out)
        elif args.command == "verify":
            result = verify_release(args.root)
        else:
            verify_release(args.root)
            samples = rows(args.root / "inputs" / f"{args.split}.jsonl")
            labels = rows(args.root / "labels" / f"{args.split}.jsonl")
            require(args.horizon in read_json(args.root / "config.json")["horizons"], "未配置该 horizon")
            require(not args.out.exists(), "输出文件已存在，请使用新路径")
            args.out.parent.mkdir(parents=True, exist_ok=True)
            if args.command == "baseline":
                write_rows(args.out, baseline(samples, args.horizon))
                result = {"predictions": str(args.out), "strategy": "past_momentum"}
            else:
                result = evaluate(samples, labels, rows(args.predictions),
                                  read_json(args.root / "config.json")["reference_etf"],
                                  args.split, args.horizon, args.top_k)
                write_json(args.out, result)
        print(json.dumps(result, ensure_ascii=False, indent=2))
        return 0
    except (ValueError, KeyError, TypeError, ArithmeticError, OSError) as exc:
        print(f"benchmark 失败: {exc}", file=sys.stderr)
        return 1
