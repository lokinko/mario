import type { Holding, HoldingTrade, Snapshot } from "../../types";

export function isClosedHolding(holding: Holding, hasTrades: boolean) {
  return hasTrades && holding.quantity === 0;
}

export function unrealizedProfit(holding: Holding, hasTrades: boolean) {
  if (isClosedHolding(holding, hasTrades)) return 0;
  return holding.costBasis > 0 ? holding.marketValue - holding.costBasis : null;
}

function newestTradeFirst(a: HoldingTrade, b: HoldingTrade) {
  return (
    b.occurredOn.localeCompare(a.occurredOn) ||
    b.sequence - a.sequence ||
    b.createdAt.localeCompare(a.createdAt)
  );
}

export function sortInvestmentHoldings(
  holdings: Holding[],
  snapshot: Pick<
    Snapshot,
    "profile" | "holdingTradeSummaries" | "holdingTrades"
  >,
) {
  const lastTrades = new Map<string, HoldingTrade>();
  for (const trade of snapshot.holdingTrades ?? []) {
    const latest = lastTrades.get(trade.holdingId);
    if (!latest || newestTradeFirst(trade, latest) < 0)
      lastTrades.set(trade.holdingId, trade);
  }
  const valueInBase = (holding: Holding) => {
    if (holding.currency === snapshot.profile.baseCurrency)
      return holding.marketValue;
    return holding.fxRateToBase != null && holding.fxRateToBase > 0
      ? holding.marketValue * holding.fxRateToBase
      : null;
  };
  return [...holdings].sort((a, b) => {
    const aClosed = isClosedHolding(
      a,
      Boolean(snapshot.holdingTradeSummaries?.[a.id]),
    );
    const bClosed = isClosedHolding(
      b,
      Boolean(snapshot.holdingTradeSummaries?.[b.id]),
    );
    if (aClosed !== bClosed) return aClosed ? 1 : -1;
    if (aClosed) {
      const aTrade = lastTrades.get(a.id);
      const bTrade = lastTrades.get(b.id);
      if (aTrade && bTrade) return newestTradeFirst(aTrade, bTrade);
      return aTrade ? -1 : bTrade ? 1 : 0;
    }
    const aValue = valueInBase(a);
    const bValue = valueInBase(b);
    // Keep assets without a usable conversion after comparable open positions.
    if (aValue == null || bValue == null)
      return aValue == null ? (bValue == null ? 0 : 1) : -1;
    return bValue - aValue;
  });
}
