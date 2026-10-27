"""可恢复的行情采集与财报原件下载；凭据只从环境读取，不进入缓存。"""

import hashlib
import json
import os
import time
from datetime import date
from decimal import Decimal
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.parse import urlparse
from urllib.request import Request, urlopen

from .dataset import select_universe
from .files import (Invalid, digest, read_json, require, rows, symbol, write_json,
                    write_rows)


class Tushare:
    def __init__(self, cache, interval=1.3):
        self.token = os.environ.get("TUSHARE_TOKEN")
        require(bool(self.token), "未配置 TUSHARE_TOKEN；请在本机环境配置，不要写入仓库")
        self.cache = Path(cache)
        self.cache.mkdir(parents=True, exist_ok=True)
        self.interval = interval
        self.last_request = 0

    def query(self, api, **params):
        key = hashlib.sha256(json.dumps([api, params], sort_keys=True).encode()).hexdigest()
        path = self.cache / (key + ".json")
        if path.exists():
            return read_json(path)["rows"]
        time.sleep(max(0, self.interval - (time.monotonic() - self.last_request)))
        request = Request("https://api.tushare.pro", method="POST",
                          data=json.dumps({"api_name": api, "token": self.token,
                                           "params": params, "fields": ""}).encode(),
                          headers={"Content-Type": "application/json"})
        self.last_request = time.monotonic()
        try:
            with urlopen(request, timeout=60) as response:
                payload = json.load(response, parse_float=str)
        except (HTTPError, URLError, TimeoutError):
            raise Invalid(f"Tushare {api} 请求失败；已缓存的成功请求可复用") from None
        # 不输出供应商原始错误，以免供应商回显凭据。
        require(payload.get("code") == 0 and payload.get("data"),
                f"Tushare {api} 被拒绝，检查账户权限、积分或频率")
        data = payload["data"]
        result = [dict(zip(data["fields"], values, strict=True)) for values in data["items"]]
        require(len(result) < 1000, f"{api} 返回可能截断，请缩小请求范围")
        write_json(path, {"api": api, "params": params, "rows": result})
        return result


def iso(value):
    return date.fromisoformat(value).isoformat()


def collect_universe(root, client, index_code):
    """采集可核对的历史候选提案；供应商月度快照不冒充公告时点证据。"""
    config = read_json(root / "config.json")
    for name in ("universe", "instruments"):
        require(not (root / f"{name}.jsonl").exists(), f"{name}.jsonl 已存在，请使用新目录")
    snapshots = []
    for year in range(int(config["start"][:4]), int(config["end"][:4]) + 1):
        start, end = f"{year - 1}1201", f"{year - 1}1231"
        calendar = client.query("trade_cal", exchange="SSE", start_date=start, end_date=end)
        selection = max(row["cal_date"] for row in calendar if str(row["is_open"]) == "1")
        members = client.query("index_weight", index_code=index_code, start_date=start, end_date=end)
        require(bool(members), f"{year} 年度没有历史成分，请核对供应商指数代码和数据权限")
        snapshot_date = max(row["trade_date"] for row in members if row["trade_date"] <= selection)
        codes = sorted({row["con_code"] for row in members if row["trade_date"] == snapshot_date})
        require(len(codes) >= 20, f"{year} 年度成分不足")
        candidates = []
        for code in codes:
            records = client.query("daily_basic", ts_code=code, trade_date=selection)
            require(len(records) == 1 and records[0]["total_mv"] is not None,
                    f"{code}/{selection} 缺少年末总市值；不能用当前值回填")
            candidates.append({"symbol": code, "market_cap": str(Decimal(str(records[0]["total_mv"])) * 10000)})
        snapshots.append({"year": year, "selection_date": iso(selection),
                          "available_at": iso(selection) + "T18:00:00+08:00",
                          "membership_snapshot_date": iso(snapshot_date),
                          "membership_published_at": None, "membership_verified": False,
                          "source_uri": "https://tushare.pro/document/2?doc_id=96",
                          "candidate_count": len(candidates), "candidates": candidates})
    stocks = {c["symbol"] for row in snapshots for c in select_universe(row)}
    etfs = {row["symbol"] for row in config["etfs"]}
    instruments = []
    for code in sorted(stocks | etfs):
        api = "fund_basic" if code in etfs else "stock_basic"
        params = {"ts_code": code, "market": "E"} if code in etfs else {"ts_code": code, "list_status": ""}
        records = client.query(api, **params)
        records = [row for row in records if row["ts_code"] == code]
        if not records and code not in etfs:
            for status in ("D", "P"):
                records = client.query(api, ts_code=code, list_status=status)
                records = [row for row in records if row["ts_code"] == code]
                if records:
                    break
        require(len(records) == 1, f"{code} 上市/退市元数据缺失或不唯一")
        row = records[0]
        instruments.append({"symbol": code, "kind": "etf" if code in etfs else "stock",
                            "name": row["name"], "listed_on": iso(row["list_date"]),
                            "delisted_on": iso(row["delist_date"]) if row.get("delist_date") else None,
                            "source_uri": "https://tushare.pro/document/2?doc_id=" + ("19" if code in etfs else "25")})
    write_rows(root / "universe.jsonl", snapshots)
    write_rows(root / "instruments.jsonl", instruments)


def collect_prices(root, client):
    config = read_json(root / "config.json")
    snapshots = rows(root / "universe.jsonl")
    require(len(snapshots) == date.fromisoformat(config["end"]).year
            - date.fromisoformat(config["start"]).year + 1, "请先导入每年的历史候选池")
    instruments = {row["symbol"]: row for row in rows(root / "instruments.jsonl")}
    stocks = {c["symbol"] for row in snapshots for c in select_universe(row)}
    etfs = {e["symbol"] for e in config["etfs"]}
    require(stocks | etfs <= instruments.keys(), "请先导入股票和 ETF 上市/退市元数据")
    for name in ("calendar", "prices"):
        require(not (root / f"{name}.jsonl").exists(), f"{name}.jsonl 已存在，请在新目录采集")
    calendar, prices = [], []
    for year in range(date.fromisoformat(config["source_start"]).year,
                      date.fromisoformat(config["end"]).year + 1):
        start = max(config["source_start"], f"{year}-01-01").replace("-", "")
        end = min(config["end"], f"{year}-12-31").replace("-", "")
        for row in client.query("trade_cal", exchange="SSE", start_date=start, end_date=end):
            calendar.append({"date": iso(row["cal_date"]), "is_open": str(row["is_open"]) == "1",
                             "source_uri": "https://tushare.pro/document/2?doc_id=26"})
        for code in sorted(stocks | etfs):
            symbol(code)
            api = "fund_daily" if code in etfs else "daily"
            factor_api = "fund_adj" if code in etfs else "adj_factor"
            factors = {row["trade_date"]: row["adj_factor"] for row in client.query(
                factor_api, ts_code=code, start_date=start, end_date=end)}
            for row in client.query(api, ts_code=code, start_date=start, end_date=end):
                factor = factors.get(row["trade_date"])
                require(factor is not None, f"{code}/{row['trade_date']} 缺少复权因子")
                prices.append({"symbol": code, "date": iso(row["trade_date"]), "status": "trading",
                               **{key: str(row[key]) for key in ("open", "high", "low", "close")},
                               "volume": str(Decimal(str(row["vol"])) * 100),
                               "amount": str(Decimal(str(row["amount"])) * 1000),
                               "adj_factor": str(factor),
                               "adjustment_basis": "provider_total_return_proxy",
                               "available_at": iso(row["trade_date"]) + "T18:00:00+08:00",
                               "source_uri": "https://tushare.pro/document/2?doc_id="
                               + ("127" if code in etfs else "27")})
    # 供应商无日线可能表示停牌或遗漏，绝不自动把缺失记录填成停牌。
    write_rows(root / "calendar.jsonl", sorted(calendar, key=lambda r: r["date"]))
    write_rows(root / "prices.jsonl", sorted(prices, key=lambda r: (r["symbol"], r["date"])))


def fetch_documents(root):
    require(not (root / "reports.jsonl").exists(), "reports.jsonl 已存在，请保留版本后使用新目录")
    catalog = rows(root / "report_sources.jsonl")
    require(bool(catalog), "请先导入 report_sources.jsonl，保留原始/修订版披露时点")
    documents = root / "documents"
    documents.mkdir(exist_ok=True)
    output = []
    for row in catalog:
        host = urlparse(row["source_uri"]).hostname or ""
        require(urlparse(row["source_uri"]).scheme == "https" and any(
            host == domain or host.endswith("." + domain)
            for domain in ("cninfo.com.cn", "sse.com.cn", "szse.cn")), "仅下载交易所/巨潮财报 HTTPS 原件")
        # URL 缓存用于断点恢复，最终文件以内容哈希标识。
        cache_path = documents / (hashlib.sha256(row["source_uri"].encode()).hexdigest() + ".pdf")
        if not cache_path.exists():
            part = cache_path.with_suffix(".part")
            try:
                with urlopen(Request(row["source_uri"], headers={"User-Agent": "mario-benchmark/1"}),
                             timeout=60) as response, part.open("wb") as target:
                    final_url = urlparse(response.geturl())
                    final_host = final_url.hostname or ""
                    require(final_url.scheme == "https" and any(final_host == domain or final_host.endswith("." + domain)
                            for domain in ("cninfo.com.cn", "sse.com.cn", "szse.cn")), "重定向离开财报官方来源")
                    size = 0
                    while chunk := response.read(1024 * 1024):
                        size += len(chunk)
                        require(size <= 100 * 1024 * 1024, "财报超过 100 MiB 上限")
                        target.write(chunk)
                with part.open("rb") as handle:
                    require(handle.read(5) == b"%PDF-", "下载结果不是 PDF，可能被限流")
                part.replace(cache_path)
            finally:
                part.unlink(missing_ok=True)
        sha = digest(cache_path)
        if row.get("sha256"):
            require(row["sha256"] == sha, "源文件已变化，请新增修订记录，不得覆盖旧版")
        output.append({**row, "sha256": sha, "document_path": cache_path.relative_to(root).as_posix()})
    write_rows(root / "reports.jsonl", output)
