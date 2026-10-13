import type { Holding, Snapshot } from "../../types";
import { formatMoney } from "../../lib/format";
import { unrealizedProfit } from "./holdingMetrics";

export function InvestmentOverview({
  holdings,
  summaries,
}: {
  holdings: Holding[];
  summaries: Snapshot["holdingTradeSummaries"];
}) {
  const totals = new Map<
    string,
    {
      value: number;
      cost: number;
      unrealized: number;
      realized: number;
      knownCost: number;
      knownTrades: number;
    }
  >();
  let missingCost = 0;
  let missingTrades = 0;
  for (const holding of holdings) {
    const total = totals.get(holding.currency) ?? {
      value: 0,
      cost: 0,
      unrealized: 0,
      realized: 0,
      knownCost: 0,
      knownTrades: 0,
    };
    const summary = summaries?.[holding.id];
    const profit = unrealizedProfit(holding, Boolean(summary));
    total.value += holding.marketValue;
    total.cost += holding.costBasis;
    if (profit == null) missingCost++;
    else {
      total.unrealized += profit;
      total.knownCost++;
    }
    if (summary) {
      total.realized += Number(summary.settledProfit);
      total.knownTrades++;
    } else missingTrades++;
    totals.set(holding.currency, total);
  }
  return (
    <div className="investment-overview" aria-label="投资持仓概览">
      <div className="investment-overview-grid">
        {(
          [
            ["value", "当前总市值"],
            ["cost", "持仓总成本"],
            ["unrealized", "浮动盈亏"],
            ["realized", "已结算盈亏"],
          ] as const
        ).map(([key, label]) => (
          <div key={key} className={`investment-overview-metric metric-${key}`}>
            <span>{label}</span>
            {[...totals].map(([currency, total]) => {
              const profit = key === "unrealized" || key === "realized";
              const known =
                key === "cost" || key === "unrealized"
                  ? total.knownCost > 0
                  : key !== "realized" || total.knownTrades > 0;
              return (
                <div key={currency} className="investment-overview-value">
                  <strong
                    className={
                      known && profit ? (total[key] >= 0 ? "gain" : "loss") : ""
                    }
                  >
                    {known
                      ? `${profit && total[key] > 0 ? "+" : ""}${formatMoney(total[key], currency)}`
                      : "—"}
                  </strong>
                  <small>{currency}</small>
                </div>
              );
            })}
          </div>
        ))}
      </div>
      {missingCost > 0 || missingTrades > 0 ? (
        <p className="investment-overview-note">
          {missingCost > 0 &&
            `${missingCost} 项成本待补，成本与浮动盈亏仅含已知部分`}
          {missingCost > 0 && missingTrades > 0 ? " · " : ""}
          {missingTrades > 0 &&
            `${missingTrades} 项未录入交易，已结算盈亏仅含已记录部分`}
        </p>
      ) : null}
    </div>
  );
}
