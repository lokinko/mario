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

DEFAULT_CONFIG = Path(__file__).with_name("config.json")


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
        if args.command == "tech-universe":
            years = [int(value) for value in args.years.split(",") if value.strip()]
            result = collect_tech_universe(args.root, years, args.top, args.workers, args.sw_file)
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
