"""预测排序指标；不将复权收益代理冒充可成交组合收益。"""

from collections import defaultdict
from decimal import Decimal
from math import sqrt
from statistics import mean

from .files import require


def ranks(values):
    positions = defaultdict(list)
    for index, value in enumerate(sorted(values)):
        positions[value].append(index + 1)
    return [mean(positions[value]) for value in values]


def correlation(left, right):
    x, y = ranks(left), ranks(right)
    mx, my = mean(x), mean(y)
    covariance = sum((a - mx) * (b - my) for a, b in zip(x, y, strict=True))
    denominator = sqrt(sum((a - mx) ** 2 for a in x) * sum((b - my) ** 2 for b in y))
    return covariance / denominator if denominator else None


def baseline(samples, horizon):
    for sample in samples:
        for stock in sample["stocks"]:
            if stock["momentum"] is not None:
                yield {"sample_id": sample["id"], "symbol": stock["symbol"],
                       "horizon": horizon, "score": stock["momentum"]}


def evaluate(samples, labels, predictions, reference_etf, split, horizon, top_k=5):
    require(split in ("train", "validation", "test"), "split 错误")
    require(type(top_k) is int and 1 <= top_k <= 20, "top_k 必须在 1–20")
    eligible = {s["id"]: {stock["symbol"] for stock in s["stocks"]} for s in samples}
    targets = {(r["sample_id"], r["symbol"]): r for r in labels
               if r["split"] == split and r["horizon"] == horizon}
    require(bool(targets), "该 split/horizon 没有标签，可能已按边界清除")
    by_sample = defaultdict(dict)
    seen = set()
    for prediction in predictions:
        key = (prediction["sample_id"], prediction["symbol"], prediction["horizon"])
        require(key not in seen, "预测键重复")
        seen.add(key)
        require(key[0] in eligible and key[1] in eligible[key[0]], "预测引用未知样本或非候选股票")
        require(isinstance(prediction["score"], str), "预测 score 必须为十进制字符串")
        score = Decimal(prediction["score"])
        require(score.is_finite(), "预测 score 必须有限")
        if key[2] == horizon and key[:2] in targets:
            by_sample[key[0]][key[1]] = score
    periods, excluded = [], []
    for sample_id in sorted({key[0] for key in targets}):
        candidates = eligible[sample_id]
        scores = by_sample[sample_id]
        # 少交或跳过难预测的股票不能提高指标；整期排除并显示原因。
        if set(scores) != candidates:
            excluded.append({"sample_id": sample_id, "reason": "incomplete_predictions"})
            continue
        actual = {code: targets.get((sample_id, code), {}).get("adjusted_return_proxy") for code in candidates}
        ref = targets.get((sample_id, reference_etf), {}).get("adjusted_return_proxy")
        if ref is None or any(value is None for value in actual.values()):
            excluded.append({"sample_id": sample_id, "reason": "unavailable_outcome"})
            continue
        codes = sorted(candidates)
        selected = sorted(codes, key=lambda code: (-scores[code], code))[:top_k]
        outcomes = {code: float(Decimal(value)) for code, value in actual.items()}
        selected_return = mean(outcomes[code] for code in selected)
        periods.append({"sample_id": sample_id, "selected": selected,
                        "rank_ic": correlation([scores[code] for code in codes],
                                               [Decimal(actual[code]) for code in codes]),
                        "top_k_mean_return_proxy": selected_return,
                        "equal_weight_mean_return_proxy": mean(outcomes.values()),
                        "reference_etf_return_proxy": float(Decimal(ref)),
                        "excess_return_proxy": selected_return - float(Decimal(ref))})
    correlations = [row["rank_ic"] for row in periods if row["rank_ic"] is not None]
    return {"split": split, "horizon": horizon, "top_k": top_k,
            "eligible_periods": len(periods) + len(excluded), "evaluated_periods": len(periods),
            "mean_rank_ic": mean(correlations) if correlations else None,
            "mean_excess_return_proxy": mean(p["excess_return_proxy"] for p in periods) if periods else None,
            "periods": periods, "excluded": excluded,
            "limitations": ["季度预测窗口可能重叠，不连乘、不年化、不报告组合 Sharpe",
                            "复权收益代理不含手续费、税、涨跌停成交约束和实际股息到账时点",
                            "LLM 可能记得历史结果；时间截断不能消除预训练数据污染"]}
