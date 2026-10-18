import type { Holding, Snapshot } from "../../types";
export const overviewMetrics = [
  "value",
  "cost",
  "unrealized",
  "realized",
] as const;
export function portfolioOverview(
  holdings: Holding[],
  accounting: NonNullable<Snapshot["accounting"]>,
  cash = false,
) {
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
}
