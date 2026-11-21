import { Money } from "../../components/AmountPrivacy";
import { Card } from "antd";
import { useEffect, useState } from "react";
import { getDividendIncome, type DividendIncome } from "../../api/portfolio";
import type { DataIssue, DailyHistory, Snapshot } from "../../types/user";
import { formatAmount, formatMoney, formatNumber } from "../../lib/format";

export function AssetOverview({
  snapshot,
  history,
  onLocateIssue,
  issuesDisabled = false,
}: {
  snapshot: Snapshot;
  onLocateIssue?: (issue: DataIssue) => void;
  issuesDisabled?: boolean;
  history?: DailyHistory;
}) {
  const latest = snapshot.valuationStatus.comparable
    ? history?.records[0]
    : undefined;
  const [dividends, setDividends] = useState<DividendIncome | null>(null);
  useEffect(() => {
    let active = true;
    getDividendIncome()
      .then((next) => active && setDividends(next))
      .catch(() => active && setDividends(null));
    return () => {
      active = false;
    };
  }, [snapshot]);
  const currency = latest?.baseCurrency ?? snapshot.profile.baseCurrency;
  const total = !snapshot.valuationStatus.comparable
    ? null
    : latest
      ? (latest.exactTotalAssets ?? latest.totalAssets)
      : snapshot.accounting?.totalValue;
  const cashValue = snapshot.accounting?.cash.value.value ?? null;
  const totalForRatio =
    snapshot.accounting?.totalValue != null &&
    Number(snapshot.accounting.totalValue) > 0
      ? Number(snapshot.accounting.totalValue)
      : total != null && Number(total) > 0
        ? Number(total)
        : null;
  const cashPct =
    cashValue != null && totalForRatio != null
      ? formatNumber((Number(cashValue) / totalForRatio) * 100, 2, true)
      : null;
  const dividendNote = dividends
    ? [
        `按买卖记录与 ${dividends.since} 后的除权日推算`,
        dividends.missing.length > 0
          ? `${dividends.missing.join("、")}未计入`
          : "",
      ]
        .filter(Boolean)
        .join(" · ")
    : undefined;
  const symbol =
    new Intl.NumberFormat("zh-CN", { style: "currency", currency })
      .formatToParts(0)
      .find((p) => p.type === "currency")?.value ?? currency;
  return (
    <section className="portfolio-summary" aria-label="资产总览">
      <Card size="small">
        <div className="asset-overview">
          <div className="hero-main">
            <span className="metric-label">总资产（{currency}）</span>
            <div className="hero-amount">
              <span className="currency-symbol">{symbol}</span>
              <Money as="strong" className="num">
                {total == null ? "—" : formatAmount(total, currency)}
              </Money>
            </div>
            {latest && latest.liabilities > 0 && (
              <p className="hero-net">
                净资产{" "}
                <Money className="num">
                  {latest.netAssets == null
                    ? "—"
                    : formatMoney(
                        latest.exactNetAssets ?? latest.netAssets,
                        currency,
                      )}
                </Money>{" "}
                · 已扣负债{" "}
                <Money className="num">
                  {formatMoney(
                    latest.exactLiabilities ?? latest.liabilities,
                    currency,
                  )}
                </Money>
              </p>
            )}
          </div>
          <div className="hero-changes">
            <div title={dividendNote}>
              <span className="metric-label">近 12 个月分红（税前）</span>
              <Money as="strong" className="num">
                {dividends == null
                  ? "—"
                  : formatAmount(dividends.value, dividends.baseCurrency)}
              </Money>
              {dividends?.assetsPct != null && (
                <small>占总资产 {dividends.assetsPct}%</small>
              )}
            </div>
            <div>
              <span className="metric-label">现金价值（{currency}）</span>
              <Money as="strong" className="num">
                {cashValue == null ? "—" : formatAmount(cashValue, currency)}
              </Money>
              {cashPct != null && <small>占总资产 {cashPct}%</small>}
            </div>
          </div>
          {snapshot.valuationStatus.issues.length > 0 && (
            <details className="hero-issues">
              <summary>
                {snapshot.valuationStatus.issues.length} 项需要处理
              </summary>
              {snapshot.valuationStatus.issues.map((issue) => (
                <button
                  className="text-button"
                  key={`${issue.code}-${issue.holdingIds.join()}`}
                  disabled={issuesDisabled}
                  onClick={() => onLocateIssue?.(issue)}
                >
                  定位{" "}
                  {snapshot.holdings.find((h) =>
                    issue.holdingIds.includes(h.id),
                  )?.name ?? issue.currency}
                </button>
              ))}
            </details>
          )}
        </div>
      </Card>
    </section>
  );
}
