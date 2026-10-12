import type { PortfolioEventRecord, Snapshot } from "../../types";

export function ratio(numerator: number, denominator: number) {
  return denominator > 0 ? (numerator / denominator) * 100 : null;
}

export function analyzePortfolio(snapshot: Snapshot) {
  const { profile, holdings, valuationStatus } = snapshot;
  const rows = holdings.map((holding) => {
    const rate =
      holding.currency === profile.baseCurrency ? 1 : holding.fxRateToBase;
    const value = rate != null && rate > 0 ? holding.marketValue * rate : null;
    // Zero is also the default for an unrecorded cost. Cash has no investment P/L.
    const cost =
      holding.assetClass !== "现金" &&
      holding.costBasis > 0 &&
      rate != null &&
      rate > 0
        ? holding.costBasis * rate
        : null;
    return {
      holding,
      value,
      cost,
      pnl: value != null && cost != null ? value - cost : null,
      pnlPct: cost != null && value != null ? ratio(value - cost, cost) : null,
      weight:
        valuationStatus.comparable && value != null
          ? ratio(value, snapshot.totalValue)
          : null,
    };
  });
  const comparable =
    valuationStatus.comparable && rows.every((row) => row.value != null);
  const total = comparable
    ? rows.reduce((sum, row) => sum + row.value!, 0)
    : null;
  const cash = comparable
    ? rows
        .filter((row) => row.holding.assetClass === "现金")
        .reduce((sum, row) => sum + row.value!, 0)
    : null;
  const investments = total != null && cash != null ? total - cash : null;
  const costed = rows.filter((row) => row.cost != null && row.value != null);
  const knownCost = costed.reduce((sum, row) => sum + row.cost!, 0);
  const knownValue = costed.reduce((sum, row) => sum + row.value!, 0);
  const pnl = comparable && costed.length ? knownValue - knownCost : null;
  const group = (dimension: "assetClass" | "currency") => {
    if (!comparable) return [];
    const values = new Map<string, number>();
    rows.forEach((row) => {
      const name = row.holding[dimension];
      values.set(name, (values.get(name) ?? 0) + row.value!);
    });
    return [...values]
      .map(([name, value]) => ({ name, value, pct: ratio(value, total!) ?? 0 }))
      .sort((a, b) => b.value - a.value);
  };
  const sorted = [...rows].sort((a, b) => (b.value ?? -1) - (a.value ?? -1));
  const top3 = comparable
    ? sorted.slice(0, 3).reduce((sum, row) => sum + row.value!, 0)
    : null;
  const surplus = profile.monthlyIncome - profile.monthlyExpense;
  const committed = snapshot.goals.reduce(
    (sum, goal) => sum + goal.monthlyContribution,
    0,
  );
  const targetTotal = holdings.reduce(
    (sum, holding) => sum + holding.targetPct,
    0,
  );
  return {
    rows,
    sorted,
    comparable,
    total,
    cash,
    investments,
    pnl,
    pnlPct: pnl != null ? ratio(pnl, knownCost) : null,
    costedCount: costed.length,
    investmentCount: holdings.filter((holding) => holding.assetClass !== "现金")
      .length,
    costCoveragePct:
      comparable && investments != null ? ratio(knownValue, investments) : null,
    netAssets: total != null ? total - profile.liabilities : null,
    debtRatio: total != null ? ratio(profile.liabilities, total) : null,
    cashPct: cash != null && total != null ? ratio(cash, total) : null,
    freeCash: cash != null ? Math.max(0, cash - profile.emergencyFund) : null,
    reserveShortfall:
      cash != null ? Math.max(0, profile.emergencyFund - cash) : null,
    emergencyMonths:
      profile.monthlyExpense > 0
        ? profile.emergencyFund / profile.monthlyExpense
        : null,
    savingsRate: ratio(surplus, profile.monthlyIncome),
    surplus,
    committed,
    uncommittedMonthly: surplus - committed,
    top3Pct: top3 != null && total != null ? ratio(top3, total) : null,
    foreignPct:
      comparable && total != null
        ? ratio(
            rows
              .filter((row) => row.holding.currency !== profile.baseCurrency)
              .reduce((sum, row) => sum + row.value!, 0),
            total,
          )
        : null,
    targetTotal,
    targetsValid: holdings.length > 0 && Math.abs(targetTotal - 100) < 0.01,
    byClass: group("assetClass"),
    byCurrency: group("currency"),
  };
}

export function summarizeEvents(
  events: PortfolioEventRecord[],
  currency: string,
  from: string | null,
  to: string,
) {
  const inRange = events.filter(
    (event) => (!from || event.occurredOn >= from) && event.occurredOn <= to,
  );
  const current = inRange.filter((event) => event.baseCurrency === currency);
  const amount = (types: PortfolioEventRecord["eventType"][]) =>
    current
      .filter((event) => types.includes(event.eventType))
      .reduce((sum, event) => sum + event.baseAmount, 0);
  // Include both originals and reversals by date, preserving the frozen ledger.
  return {
    count: current.length,
    excluded: inRange.length - current.length,
    deposits: amount(["deposit"]),
    withdrawals: amount(["withdrawal"]),
    netFlow: amount(["deposit"]) - amount(["withdrawal"]),
    income: amount(["dividend", "interest"]),
    costs: amount(["fee", "tax"]),
    trading: amount(["buy", "sell"]),
  };
}

export function rangeStart(today: string, days: number) {
  if (!days) return null;
  const date = new Date(`${today}T00:00:00Z`);
  date.setUTCDate(date.getUTCDate() - days + 1);
  return date.toISOString().slice(0, 10);
}
