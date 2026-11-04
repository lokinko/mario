"""公告事件的市场超额反应；来源解析和采集归原始数据适配层。"""

from bisect import bisect_right
from datetime import date
from decimal import Decimal

from .files import require


def announcement_signal(bars, calendar, decision, events, market):
    """CAR3=sum(个股日收益-市场日收益)；公告日仅有日期，按当日23:59后可得。"""
    cutoff = decision + "T18:00:00+08:00"
    eligible = [r for r in events if r["available_at"] <= cutoff]
    if not eligible:
        return None, None, "no_prior_original_report"
    latest = max(eligible, key=lambda r: (r["available_at"], r["period_end"], r["revision_id"]))
    published = latest["available_at"][:10]
    age = (date.fromisoformat(decision) - date.fromisoformat(published)).days
    metadata = {k: latest[k] for k in ("period_end", "available_at", "revision_id", "source_uri")}
    metadata.update(age_days=age, version_verified=False)
    if age > 183:
        return None, metadata, "stale_report_event"
    # 可影响成交的事件日取公告日期之后第一个市场交易日，不假定开盘前已经发布。
    event_index = bisect_right(calendar, published)
    end_index = event_index + 1
    if event_index < 2 or end_index >= len(calendar) or calendar[end_index] > decision:
        return None, metadata, "announcement_window_not_yet_observed"
    window = calendar[event_index - 2:end_index + 1]
    if any(d not in bars or bars[d]["tradestatus"] != "1" or d not in market for d in window):
        return None, metadata, "missing_or_suspended_announcement_window"
    stock_returns, market_returns = [], []
    for previous, day in zip(window, window[1:]):
        x, y = Decimal(bars[previous]["close"]), Decimal(bars[day]["close"])
        a, b = Decimal(market[previous]), Decimal(market[day])
        require(all(v.is_finite() and v > 0 for v in (x, y, a, b)), "公告窗口存在无效价格")
        stock_returns.append(y / x - 1)
        market_returns.append(b / a - 1)
    metadata.update(effective_session=calendar[event_index], window_end=window[-1])
    return str(sum(stock_returns) - sum(market_returns)), metadata, None
