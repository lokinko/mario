"""Build the public watch bootstrap from verified acquisition originals, offline."""
import argparse
from datetime import date, datetime
from decimal import Decimal
import hashlib
import json
from pathlib import Path


def read_verified(root, name, manifest):
    path = (root / name).resolve()
    if not path.is_relative_to(root.resolve()):
        raise ValueError("归档路径越界")
    content = path.read_bytes()
    if hashlib.sha256(content).hexdigest() != manifest["files"].get(name):
        raise ValueError(f"归档哈希不符：{name}")
    return json.loads(content, parse_float=Decimal)


def amount(value):
    # Same source contract as the online adapter: null is an absent component.
    result = Decimal(str(value)) if value is not None else Decimal(0)
    if not result.is_finite() or result < 0:
        raise ValueError("金额或送转比例无效")
    return format(result, "f")


def prepare(root):
    root = Path(root)
    manifest = json.loads((root / "manifest.json").read_bytes())
    coverage = read_verified(root, "coverage.json", manifest)
    cutoff = date.fromisoformat(coverage["end"])
    if coverage["window_basis"] != "ex_dividend_date":
        raise ValueError("分红窗口口径不符")
    stocks = []
    for name in sorted(manifest["files"]):
        if not name.startswith("stocks/") or not name.endswith(".json"):
            continue
        stock = read_verified(root, name, manifest)
        summary = stock["summary"]
        code = summary["symbol"]
        entry = {"symbol": code.replace(".SH", ".SS"), "name": summary.get("security_name") or code,
                 "status": summary["status"], "history": None, "rawFiles": {}, "issue": None}
        originals = []
        for raw_name, expected_hash in sorted(summary.get("raw_files", {}).items()):
            if manifest["files"].get("raw/" + raw_name) != expected_hash:
                raise ValueError("逐股来源哈希与清单不一致")
            raw = read_verified(root, "raw/" + raw_name, manifest)
            if raw["symbol"] != code or raw["page"] != len(originals) + 1:
                raise ValueError("分红分页请求身份不符")
            originals.append(raw)
            entry["rawFiles"][raw_name] = expected_hash
        try:
            if summary["status"] in ("source_no_records", "source_unavailable"):
                entry["issue"] = summary["status"]
            else:
                if not originals:
                    raise ValueError("缺少来源原件")
                first = originals[0]["response"]["result"]
                records = []
                for raw in originals:
                    response = raw["response"]
                    page = response["result"]
                    if (response["success"] is not True or page["count"] != first["count"]
                            or page["pages"] != len(originals)):
                        raise ValueError("分页元数据不完整或已变化")
                    records.extend(page["data"])
                if len(records) != first["count"]:
                    raise ValueError("来源记录总数不符")
                events, seen = [], set()
                for row in records:
                    if row["SECUCODE"] != code or row["SECURITY_CODE"] != code[:6]:
                        raise ValueError("来源证券身份不符")
                    if row["ASSIGN_PROGRESS"] != "实施分配":
                        continue
                    if not row.get("EX_DIVIDEND_DATE"):
                        raise ValueError("已实施历史记录缺少除息日")
                    ex_date = date.fromisoformat(row["EX_DIVIDEND_DATE"][:10])
                    if ex_date > cutoff:
                        continue
                    report_date = date.fromisoformat(row["REPORT_DATE"][:10])
                    key = (ex_date, report_date)
                    if key in seen:
                        raise ValueError("报告期或重复分红记录无效")
                    seen.add(key)
                    events.append({"reportDate": report_date.isoformat(), "exDate": ex_date.isoformat(),
                                   "cashPerTen": amount(row.get("PRETAX_BONUS_RMB")),
                                   "bonusPerTen": amount(row.get("BONUS_RATIO")),
                                   "transferPerTen": amount(row.get("IT_RATIO"))})
                entry["history"] = {"asOf": cutoff.isoformat(),
                                    "fetchedAt": min((raw["retrieved_at"] for raw in originals), key=datetime.fromisoformat),
                                    "sources": [raw["source_uri"] for raw in originals],
                                    "events": sorted(events, key=lambda e: (e["exDate"], e["reportDate"]))}
                entry["status"] = "ready"
        except (ValueError, KeyError, TypeError) as error:
            entry["status"], entry["issue"] = "source_incomplete", str(error)
        stocks.append(entry)
    if len(stocks) != coverage["securities"] or len({s["symbol"] for s in stocks}) != len(stocks):
        raise ValueError("证券覆盖数量不符")
    return {"formatVersion": 1, "asOf": cutoff.isoformat(),
            "sourceManifestSha256": hashlib.sha256((root / "manifest.json").read_bytes()).hexdigest(),
            "stocks": stocks}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()
    result = prepare(args.archive)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, ensure_ascii=False, separators=(",", ":")) + "\n", encoding="utf-8", newline="\n")
    ready = [s for s in result["stocks"] if s["history"] is not None]
    print(json.dumps({"securities": len(result["stocks"]), "ready": len(ready),
                      "events": sum(len(s["history"]["events"]) for s in ready),
                      "unavailable": [{"symbol": s["symbol"], "issue": s["issue"]}
                                      for s in result["stocks"] if s["history"] is None]}, ensure_ascii=False))
