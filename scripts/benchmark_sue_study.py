"""已人工核验的普通股利润/恒定股数转单季EPS；不自动从PDF猜测会计口径。"""

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from benchmark.files import digest, read_json
from benchmark.financial_indicators import normalize, sue
from benchmark.raw_data import verify_sources


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ledger", type=Path, required=True)
    parser.add_argument("--evidence-root", type=Path, required=True)
    parser.add_argument("--cutoff", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    ledger = read_json(args.ledger)
    verify_sources(ledger, args.evidence_root)
    quarters, issues, latest = normalize(ledger, args.cutoff)
    report = {"symbol": ledger["symbol"], "cutoff": args.cutoff, "ledger_sha256": digest(args.ledger),
              "scope": "financial_input_pilot_not_scored_release",
              "method": "cn_reported_ordinary_eps_constant_shares_sue", "quarters": quarters,
              "issues": issues, "sue": sue(quarters, latest)}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("x", encoding="utf-8") as handle:
        json.dump(report, handle, ensure_ascii=False, indent=2)
        handle.write("\n")
    print(json.dumps({"symbol": report["symbol"], "sue": report["sue"]["value"], "issue": report["sue"]["issue"]}))


if __name__ == "__main__":
    main()
