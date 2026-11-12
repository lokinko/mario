import { Money } from "../../components/AmountPrivacy";
import { Eye, EyeOff } from "lucide-react";
import { Button, Card } from "antd";
import type { DataIssue, DailyHistory, Snapshot } from "../../types";
import type { DisplayPreferenceProps } from "../../components/DisplayPreferences";
import { formatAmount, formatMoney, formatSigned } from "../../lib/format";
export function AssetOverview({
  snapshot,
  history,
  display,
  onLocateIssue,
  issuesDisabled = false,
}: {
  snapshot: Snapshot;
  onLocateIssue?: (issue: DataIssue) => void;
  issuesDisabled?: boolean;
  history?: DailyHistory;
  display: Pick<DisplayPreferenceProps, "maskAmounts" | "setMaskAmounts">;
}) {
  const latest = snapshot.valuationStatus.comparable
    ? history?.records[0]
    : undefined;
  const change = latest?.sinceLastUpdateChange;
  const currency = latest?.baseCurrency ?? snapshot.profile.baseCurrency;
  const total = !snapshot.valuationStatus.comparable
    ? null
    : latest
      ? (latest.exactTotalAssets ?? latest.totalAssets)
      : snapshot.accounting?.totalValue;
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
            <div>
              <span className="metric-label">较上次实际更新</span>
              <Money
                as="strong"
                className={`num ${change == null || change === 0 ? "" : change > 0 ? "gain" : "loss"}`}
              >
                {change == null
                  ? "—"
                  : formatSigned(change, (v) => formatAmount(v, currency))}
              </Money>
              {latest?.lastUpdatedDay && (
                <small>{latest.lastUpdatedDay} 起</small>
              )}
            </div>
          </div>
          <Button
            type="text"
            size="small"
            className="hero-privacy"
            icon={
              display.maskAmounts ? <EyeOff size={15} /> : <Eye size={15} />
            }
            data-presentation="true"
            onClick={() => display.setMaskAmounts(!display.maskAmounts)}
            title="隐藏页面和读屏中的金额，复制和导出仍包含真实数据"
          >
            {display.maskAmounts ? "显示金额" : "隐藏金额"}
          </Button>
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
