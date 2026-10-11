import { describe, expect, it } from "vitest";
import type { Holding, PortfolioEventRecord, Snapshot } from "../../types";
import emptySnapshot from "../../test/emptySnapshot.json";
import {
  analyzePortfolio,
  rangeStart,
  simulateShock,
  summarizeEvents,
} from "./analytics";

export const holding = (values: Partial<Holding> = {}): Holding => ({
  id: "cash",
  name: "存款",
  symbol: "",
  assetClass: "现金",
  marketValue: 10000,
  costBasis: 0,
  targetPct: 20,
  currency: "CNY",
  fxRateToBase: null,
  valuationDate: "2026-10-11",
  fxRateSource: "",
  fxRateObservedOn: "",
  ...values,
});
export const portfolio = (): Snapshot => ({
  ...(emptySnapshot as Snapshot),
  profile: {
    ...(emptySnapshot.profile as Snapshot["profile"]),
    monthlyIncome: 10000,
    monthlyExpense: 6000,
    emergencyFund: 8000,
    liabilities: 5000,
  },
  totalValue: 45000,
  holdings: [
    holding(),
    holding({
      id: "stock",
      name: "股票",
      assetClass: "股票",
      marketValue: 20000,
      costBasis: 25000,
      targetPct: 40,
    }),
    holding({
      id: "usd",
      name: "海外基金",
      assetClass: "基金",
      currency: "USD",
      fxRateToBase: 7.5,
      marketValue: 2000,
      costBasis: 1000,
      targetPct: 40,
    }),
  ],
});

describe("portfolio analysis", () => {
  it("counts cash once, deducts debt, and excludes cash from cost-based P/L", () => {
    const snapshot = portfolio();
    const a = analyzePortfolio(snapshot);
    expect(a.total).toBe(45000);
    expect(a.netAssets).toBe(40000);
    expect(a.cash).toBe(10000);
    expect(a.investments).toBe(35000);
    expect(a.freeCash).toBe(2000);
    expect(a.pnl).toBe(2500);
    expect(a.pnlPct).toBeCloseTo((2500 / 32500) * 100);
    expect(a.costCoveragePct).toBe(100);
    expect(a.rows[0].pnl).toBeNull();
    expect(a.byCurrency.find((group) => group.name === "USD")?.value).toBe(
      15000,
    );
    expect(a.top3Pct).toBe(100);
    expect(a.targetsValid).toBe(true);
  });
  it("treats absent costs as unknown instead of a 100% gain", () => {
    const snapshot = portfolio();
    snapshot.holdings[1].costBasis = 0;
    const a = analyzePortfolio(snapshot);
    expect(a.costedCount).toBe(1);
    expect(a.pnl).toBe(7500);
    expect(a.costCoveragePct).toBeCloseTo((15000 / 35000) * 100);
    expect(a.rows[1].pnlPct).toBeNull();
    snapshot.holdings[2].costBasis = 0;
    expect(analyzePortfolio(snapshot).pnl).toBeNull();
  });
  it("keeps a zero-value investment with recorded cost as a full loss", () => {
    const snapshot = portfolio();
    snapshot.holdings[1].marketValue = 0;
    expect(analyzePortfolio(snapshot).rows[1].pnlPct).toBe(-100);
  });
  it("stops aggregate calculations when an FX rate is missing", () => {
    const snapshot = portfolio();
    snapshot.holdings[2].fxRateToBase = null;
    snapshot.valuationStatus = {
      ...snapshot.valuationStatus,
      comparable: false,
      missingFxHoldings: ["海外基金"],
    };
    const a = analyzePortfolio(snapshot);
    for (const value of [
      a.total,
      a.netAssets,
      a.cash,
      a.pnl,
      a.foreignPct,
      a.top3Pct,
    ])
      expect(value).toBeNull();
    expect(a.byClass).toEqual([]);
    expect(a.rows[2].value).toBeNull();
    expect(a.surplus).toBe(4000);
    expect(simulateShock(snapshot, 20, 5)).toBeNull();
  });
  it("avoids ratios with no denominator and flags overcommitted cash", () => {
    const snapshot = portfolio();
    snapshot.profile.monthlyIncome = 0;
    snapshot.profile.monthlyExpense = 0;
    snapshot.profile.emergencyFund = 12000;
    const a = analyzePortfolio(snapshot);
    expect(a.savingsRate).toBeNull();
    expect(a.emergencyMonths).toBeNull();
    expect(a.reserveShortfall).toBe(2000);
    expect(a.freeCash).toBe(0);
    expect(analyzePortfolio(emptySnapshot as Snapshot).debtRatio).toBeNull();
  });
  it("shows the funding gap when goal contributions exceed surplus", () => {
    const snapshot = portfolio();
    snapshot.goals = [
      {
        id: "goal",
        name: "购房",
        targetAmount: 100000,
        currentAmount: 0,
        monthlyContribution: 5000,
        targetDate: "2028-01-01",
        priority: "刚性",
      },
    ];
    expect(analyzePortfolio(snapshot).uncommittedMonthly).toBe(-1000);
  });
  it("applies market and FX shocks multiplicatively without changing cash or debt", () => {
    const result = simulateShock(portfolio(), 20, 10)!;
    expect(result.total).toBe(10000 + 20000 * 0.8 + 15000 * 0.8 * 0.9);
    expect(result.netAssets).toBe(result.total - 5000);
    expect(result.loss).toBe(8200);
    expect(result.lossPct).toBeCloseTo((8200 / 45000) * 100);
  });
});

describe("frozen ledger totals", () => {
  const event = (
    values: Partial<PortfolioEventRecord> = {},
  ): PortfolioEventRecord => ({
    id: "event",
    eventType: "deposit",
    baseCurrency: "CNY",
    baseAmount: 100,
    amount: 100,
    currency: "CNY",
    fxRateToBase: null,
    fxRateSource: "",
    fxRateObservedOn: "",
    occurredOn: "2026-10-11",
    source: "",
    externalId: "",
    assetName: "",
    note: "",
    createdAt: "",
    reversalOfEventId: null,
    reversedByEventId: null,
    ...values,
  });
  it("separates cash flows, income, expenses and internal trading; sums reversals", () => {
    const result = summarizeEvents(
      [
        event({ reversedByEventId: "reversal" }),
        event({ id: "reversal", baseAmount: -100, reversalOfEventId: "event" }),
        event({ eventType: "deposit", baseAmount: 1000 }),
        event({ eventType: "withdrawal", baseAmount: 400 }),
        event({ eventType: "dividend", baseAmount: 30 }),
        event({ eventType: "interest", baseAmount: 10 }),
        event({ eventType: "fee", baseAmount: 5 }),
        event({ eventType: "tax", baseAmount: 3 }),
        event({ eventType: "buy", baseAmount: 200 }),
        event({ eventType: "sell", baseAmount: 300 }),
        event({ baseCurrency: "USD", baseAmount: 999 }),
        event({ occurredOn: "2026-07-01" }),
        event({ occurredOn: "2026-10-12" }),
      ],
      "CNY",
      "2026-10-01",
      "2026-10-11",
    );
    expect(result).toEqual({
      count: 10,
      excluded: 1,
      deposits: 1000,
      withdrawals: 400,
      netFlow: 600,
      income: 40,
      costs: 8,
      trading: 500,
    });
  });
  it("retains a reversal in its own date range when the original precedes it", () => {
    const result = summarizeEvents(
      [
        event({ occurredOn: "2026-09-01" }),
        event({ baseAmount: -100, reversalOfEventId: "event" }),
      ],
      "CNY",
      "2026-10-01",
      "2026-10-11",
    );
    expect(result.netFlow).toBe(-100);
  });
  it("handles inclusive date ranges across leap years", () => {
    expect(rangeStart("2024-03-01", 2)).toBe("2024-02-29");
    expect(rangeStart("2026-01-01", 1)).toBe("2026-01-01");
    expect(rangeStart("2026-10-11", 0)).toBeNull();
  });
});
