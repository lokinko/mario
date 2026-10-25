import { Eye, EyeOff } from "lucide-react";
import type { DataIssue, DailyHistory, Snapshot } from "../../types";
import type { DisplayPreferenceProps } from "../../components/DisplayPreferences";
import { formatAmount, formatMoney, formatSigned } from "../../lib/format";
import { DataStamp } from "../../components/DataStamp";
import { AssetTrend } from "./AssetTrend";
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
    <section className="asset-overview" aria-label="资产总览">
      <div className="hero-main">
        <span className="metric-label">总资产（{currency}）</span>
        <div className="hero-amount">
          <span className="currency-symbol">{symbol}</span>
          <strong className="num" data-money>
            {total == null ? "—" : formatAmount(total, currency)}
          </strong>
        </div>
        {latest && latest.liabilities > 0 && (
          <p className="hero-net">
            净资产{" "}
            <span className="num" data-money>
              {latest.netAssets == null
                ? "—"
                : formatMoney(
                    latest.exactNetAssets ?? latest.netAssets,
                    currency,
                  )}
            </span>{" "}
            · 已扣负债{" "}
            <span className="num" data-money>
              {formatMoney(
                latest.exactLiabilities ?? latest.liabilities,
                currency,
              )}
            </span>
          </p>
        )}
      </div>
      <div className="hero-changes">
        {[
          ["较昨日", latest?.previousDayChange],
          ["较上次实际更新", latest?.sinceLastUpdateChange],
        ].map(([label, value]) => (
          <div key={String(label)}>
            <span className="metric-label">{label}</span>
            <strong
              className={`num ${value == null || value === 0 ? "" : Number(value) > 0 ? "gain" : "loss"}`}
              data-money
            >
              {value == null
                ? "—"
                : formatSigned(value, (v) => formatAmount(v, currency))}
            </strong>
            {label === "较上次实际更新" && latest?.lastUpdatedDay && (
              <small>{latest.lastUpdatedDay} 起</small>
            )}
          </div>
        ))}
      </div>
      <button
        className="text-button hero-privacy"
        data-presentation="true"
        onClick={() => display.setMaskAmounts(!display.maskAmounts)}
        title="仅隐藏页面金额，复制和导出仍包含真实数据"
      >
        {display.maskAmounts ? <EyeOff size={16} /> : <Eye size={16} />}
        {display.maskAmounts ? "显示金额" : "隐藏金额"}
      </button>
      {history &&
        history.records.filter(
          (r) => r.baseCurrency === currency && r.totalAssets != null,
        ).length > 1 && (
          <button
            className="hero-trend"
            aria-label="查看完整资产趋势"
            onClick={() =>
              document
                .getElementById("asset-history")
                ?.scrollIntoView({ behavior: "smooth" })
            }
          >
            <AssetTrend
              records={history.records
                .filter((r) => r.baseCurrency === currency)
                .slice(0, 30)}
              mini
            />
            <span>30 日走势 ›</span>
          </button>
        )}
      {snapshot.valuationStatus.issues.length > 0 && (
        <details className="hero-issues">
          <summary>{snapshot.valuationStatus.issues.length} 项需要处理</summary>
          {snapshot.valuationStatus.issues.map((issue) => (
            <button
              className="text-button"
              key={`${issue.code}-${issue.holdingIds.join()}`}
              disabled={issuesDisabled}
              onClick={() => onLocateIssue?.(issue)}
            >
              定位{" "}
              {snapshot.holdings.find((h) => issue.holdingIds.includes(h.id))
                ?.name ?? issue.currency}
            </button>
          ))}
        </details>
      )}
      <div className="hero-stamp">
        <DataStamp
          date={latest?.day ?? snapshot.updatedAt.slice(0, 10)}
          source={
            latest
              ? latest.carried
                ? "今日沿用上次余额"
                : "今日已更新"
              : "当前会计快照"
          }
        />
        {latest && (
          <span>
            {latest.assets.filter((a) => a.carried && !a.removed).length}{" "}
            项资产沿用原值
          </span>
        )}
      </div>
    </section>
  );
}
