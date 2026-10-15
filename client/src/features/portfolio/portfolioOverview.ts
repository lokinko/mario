import type { Holding, Snapshot } from "../../types";
import { unrealizedProfit } from "./holdingMetrics";
export const overviewMetrics = [
  "value",
  "cost",
  "unrealized",
  "realized",
] as const;
export type OverviewMetric = (typeof overviewMetrics)[number];
export type OverviewAmount = { value: number | null; missing: number };
export function portfolioOverview(
  holdings: Holding[],
  summaries: Snapshot["holdingTradeSummaries"],
  baseCurrency: string,
  accounting?: Snapshot["accounting"],
  cash = false,
) {
  if (accounting)
    return {
      totals: cash ? accounting.cash : accounting.investments,
      rows: holdings.map((holding) => {
        const exact = accounting.holdings[holding.id];
        return {
          holding,
          rate: exact?.fxRate ?? null,
          amounts: {
            value: exact?.usable ? exact.marketValue : null,
            cost:
              exact?.usable && exact.unrealizedProfit != null
                ? exact.costBasis
                : null,
            unrealized: exact?.usable ? exact.unrealizedProfit : null,
            realized: exact?.usable ? exact.settledProfit : null,
          },
        };
      }),
    };
  const totals = Object.fromEntries(
    overviewMetrics.map((key) => [
      key,
      { value: 0 as number | null, missing: 0 },
    ]),
  ) as Record<OverviewMetric, OverviewAmount>;
  const known = { value: 0, cost: 0, unrealized: 0, realized: 0 };
  const rows = holdings.map((holding) => {
    const summary = summaries?.[holding.id];
    const unrealized = unrealizedProfit(holding, Boolean(summary));
    const amounts = {
      value: holding.marketValue,
      cost: unrealized == null ? null : holding.costBasis,
      unrealized,
      realized: summary ? Number(summary.settledProfit) : null,
    };
    const rate =
      holding.currency === baseCurrency
        ? 1
        : holding.fxRateToBase != null &&
            Number.isFinite(holding.fxRateToBase) &&
            holding.fxRateToBase > 0
          ? holding.fxRateToBase
          : null;
    for (const key of overviewMetrics) {
      const amount = amounts[key];
      if (
        amount == null ||
        !Number.isFinite(amount) ||
        (rate == null && amount !== 0)
      )
        totals[key].missing++;
      else {
        totals[key].value! += amount * (rate ?? 1);
        known[key]++;
      }
    }
    return { holding, rate, amounts };
  });
  for (const key of overviewMetrics)
    if (holdings.length && !known[key]) totals[key].value = null;
  return { totals, rows };
}
