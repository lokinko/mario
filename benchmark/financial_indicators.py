"""已核验财务事实的时点投影、单季 EPS 和 SUE；不访问原件或行情。"""

import calendar
from datetime import date, datetime
from decimal import Decimal, InvalidOperation, localcontext

from .files import require


def instant(value):
    parsed = datetime.fromisoformat(value)
    require(parsed.tzinfo is not None, "披露/决策时间必须带时区")
    return parsed

def number(value):
    require(isinstance(value, str), "财务数值必须为十进制字符串")
    try:
        result = Decimal(value)
    except InvalidOperation as error:
        raise ValueError("无效财务数值") from error
    require(result.is_finite(), "财务数值必须有限")
    return result

def quarter(value):
    day = date.fromisoformat(value)
    require(day.month in (3, 6, 9, 12) and day.day == calendar.monthrange(day.year, day.month)[1],
            "只接受自然年季度末，不将半年/累计值当作单季")
    return day.year * 4 + day.month // 3 - 1

def period(index):
    year, offset = divmod(index, 4)
    month = (offset + 1) * 3
    return date(year, month, calendar.monthrange(year, month)[1]).isoformat()

def normalize(ledger, cutoff):
    """仅在已知、同口径且有全期间恒定股数证据时作利润差分；不差分已舍入EPS。"""
    boundary = instant(cutoff)
    facts, proofs, versions = {}, [], set()
    for record in ledger["cumulative"]:
        if instant(record["available_at"]) > boundary:
            continue
        index = quarter(record["period_end"])
        require(record["period_start"] == record["period_end"][:4] + "-01-01", "累计利润必须从年初起算")
        require(record["period_end"] <= instant(record["available_at"]).date().isoformat(), "财报尚未结束")
        require(record["unit"] == "CNY" and record["reviewed"] is True, "需要已核验的人民币元口径")
        number(record["ordinary_profit_ytd"])
        version = (index, instant(record["available_at"]))
        require(version not in versions, "同一期间同一时刻有歧义版本")
        versions.add(version)
        previous = facts.get(index)
        if previous is None or instant(record["available_at"]) > instant(previous["available_at"]):
            facts[index] = record
    for proof in ledger["capital_proofs"]:
        if instant(proof["available_at"]) > boundary:
            continue
        require(proof["reviewed"] is True, "股数证据尚未核验")
        require(number(proof["shares"]) > 0, "普通股股数必须为正")
        require(proof["start"] <= proof["end"] <= instant(proof["available_at"]).date().isoformat(), "股数证据期间无效")
        proofs.append(proof)
    output, issues = {}, {}
    with localcontext() as context:
        context.prec = 40
        for index, current in sorted(facts.items()):
            end = current["period_end"]
            previous = facts.get(index - 1) if index % 4 else None
            if index % 4 and previous is None:
                issues[end] = "missing_previous_ytd"
                continue
            if previous and previous["basis"] != current["basis"]:
                issues[end] = "incomparable_profit_basis"
                continue
            covering = [p for p in proofs if p["start"] <= current["period_start"] and p["end"] >= end
                        and p["basis"] == current["basis"]]
            overlapping = [p for p in proofs if p["start"] <= end and p["end"] >= current["period_start"]
                           and p["basis"] == current["basis"]]
            if any(p["constant"] is not True for p in overlapping):
                issues[end] = "known_share_change"
                continue
            eligible = covering
            if not eligible:
                issues[end] = "unverified_constant_ordinary_shares"
                continue
            if len({number(p["shares"]) for p in overlapping}) != 1:
                issues[end] = "conflicting_share_evidence"
                continue
            # 相同股数的证据取最早已知版本；不无故把可得时间推迟到更新的证明。
            proof = min(eligible, key=lambda p: instant(p["available_at"]))
            profit = number(current["ordinary_profit_ytd"])
            if previous:
                profit -= number(previous["ordinary_profit_ytd"])
            dependencies = [current, proof] + ([previous] if previous else [])
            output[end] = {"eps": str(profit / number(proof["shares"])), "profit": str(profit),
                           "shares": proof["shares"], "basis": current["basis"],
                           "available_at": max(dependencies, key=lambda p: instant(p["available_at"]))["available_at"],
                           "sources": sorted({source for p in dependencies for source in p["sources"]})}
    return output, issues, period(max(facts)) if facts else None

def sue(quarters, latest):
    """最新同比EPS差/最近八个日历季同比差的样本标准差；含当前季，至少六个。"""
    if latest is None:
        return {"value": None, "issue": "no_known_report", "innovations": []}
    end = quarter(latest)
    innovations, bases, shares = [], set(), set()
    for index in range(end - 7, end + 1):
        current, previous = quarters.get(period(index)), quarters.get(period(index - 4))
        value, issue = None, None
        if current is None or previous is None:
            issue = "missing_quarter_eps"
        elif current["basis"] != previous["basis"]:
            issue = "incomparable_profit_basis"
        elif number(current["shares"]) != number(previous["shares"]):
            issue = "unadjusted_share_change"
        else:
            with localcontext() as context:
                context.prec = 40
                value = number(current["eps"]) - number(previous["eps"])
            bases.add(current["basis"])
            shares.add(number(current["shares"]))
        innovations.append({"period_end": period(index), "value": str(value) if value is not None else None, "issue": issue})
    result = {"value": None, "latest": latest, "innovations": innovations,
              "observations": sum(r["value"] is not None for r in innovations),
              "standard_deviation": "sample_ddof_1_including_current"}
    if innovations[-1]["value"] is None:
        return dict(result, issue="latest_innovation_unavailable")
    if len(bases) != 1:
        return dict(result, issue="incomparable_window_basis")
    if len(shares) != 1:
        return dict(result, issue="unadjusted_window_share_change")
    if result["observations"] < 6:
        return dict(result, issue="fewer_than_six_innovations")
    with localcontext() as context:
        context.prec = 40
        values = [number(r["value"]) for r in innovations if r["value"] is not None]
        mean = sum(values) / len(values)
        variance = sum((v - mean) ** 2 for v in values) / (len(values) - 1)
        if variance == 0:
            return dict(result, issue="zero_innovation_variance")
        return dict(result, value=str(number(innovations[-1]["value"]) / variance.sqrt()), issue=None)
