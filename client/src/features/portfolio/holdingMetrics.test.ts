import { expect, it } from "vitest";
import { sortInvestmentHoldings } from "./holdingMetrics";
import { emptyHolding } from "./forms";
import fixture from "../../test/emptySnapshot.json";
import type { HoldingTrade, HoldingTradeSummary, Snapshot } from "../../types";

const snapshot = fixture as Snapshot;

it("sorts ascending in base currency with missing conversions and closed positions last", () => {
  const holdings = [
    {
      ...emptyHolding("USD"),
      id: "unknown",
      name: "Gamma",
      marketValue: 1,
      fxRateToBase: null,
    },
    {
      ...emptyHolding(),
      id: "closed",
      name: "Delta",
      quantity: 0,
      marketValue: 0,
    },
    {
      ...emptyHolding("USD"),
      id: "usd",
      name: "Alpha",
      marketValue: 50,
      fxRateToBase: 7,
    },
    { ...emptyHolding(), id: "cny", name: "Beta", marketValue: 200 },
  ];
  const next = {
    ...snapshot,
    holdingTradeSummaries: { closed: {} as HoldingTradeSummary },
  };
  expect(
    sortInvestmentHoldings(holdings, next, "value-asc").map((h) => h.id),
  ).toEqual(["cny", "usd", "unknown", "closed"]);
  expect(
    sortInvestmentHoldings(holdings, next, "name").map((h) => h.id),
  ).toEqual(["usd", "cny", "unknown", "closed"]);
});
function trade(
  holdingId: string,
  occurredOn: string,
  sequence = 1,
  createdAt = "2026-10-13T12:00:00Z",
): HoldingTrade {
  return {
    id: `${holdingId}-${occurredOn}-${sequence}`,
    holdingId,
    kind: "sell",
    currency: "CNY",
    occurredOn,
    sequence,
    quantity: "1",
    price: "1",
    grossAmount: "1",
    fees: "0",
    taxes: "0",
    account: "证券账户",
    externalId: "",
    note: "",
    createdAt,
    updatedAt: "2026-10-13T13:00:00Z",
  };
}

it("sorts by unrounded current value converted to base currency and preserves its input", () => {
  const holdings = [
    { ...emptyHolding(), id: "small", marketValue: 100, costBasis: 9999 },
    { ...emptyHolding("USD"), id: "usd", marketValue: 50, fxRateToBase: 7 },
    { ...emptyHolding(), id: "large", marketValue: 200 },
    { ...emptyHolding(), id: "precise", marketValue: 200.001 },
  ];
  expect(sortInvestmentHoldings(holdings, snapshot).map((h) => h.id)).toEqual([
    "usd",
    "precise",
    "large",
    "small",
  ]);
  expect(holdings.map((h) => h.id)).toEqual([
    "small",
    "usd",
    "large",
    "precise",
  ]);
});

it("places closed holdings last and uses the latest actual trade date rather than entry, edit, or valuation time", () => {
  const holdings = [
    { ...emptyHolding(), id: "old", quantity: 0, valuationDate: "2026-10-13" },
    { ...emptyHolding(), id: "new", quantity: 0, valuationDate: "2026-01-01" },
    { ...emptyHolding(), id: "open", quantity: 1, marketValue: 1 },
    { ...emptyHolding(), id: "no-trade", quantity: 0 },
  ];
  const next = {
    ...snapshot,
    holdingTradeSummaries: Object.fromEntries(
      ["old", "new", "no-trade"].map((id) => [id, {} as HoldingTradeSummary]),
    ),
    holdingTrades: [
      trade("new", "2026-10-12", 1, "2026-10-12T12:00:00Z"),
      trade("old", "2026-10-11"),
      trade("new", "2026-01-01"),
    ],
  };
  expect(sortInvestmentHoldings(holdings, next).map((h) => h.id)).toEqual([
    "open",
    "new",
    "old",
    "no-trade",
  ]);
});

it("uses same-day transaction sequence and then creation time to break closed-position ties", () => {
  const holdings = ["first", "second", "third"].map((id) => ({
    ...emptyHolding(),
    id,
    quantity: 0,
  }));
  const next = {
    ...snapshot,
    holdingTradeSummaries: Object.fromEntries(
      holdings.map((h) => [h.id, {} as HoldingTradeSummary]),
    ),
    holdingTrades: [
      trade("first", "2026-10-12", 1),
      trade("second", "2026-10-12", 2, "2026-10-12T11:00:00Z"),
      trade("third", "2026-10-12", 2, "2026-10-12T12:00:00Z"),
    ],
  };
  expect(sortInvestmentHoldings(holdings, next).map((h) => h.id)).toEqual([
    "third",
    "second",
    "first",
  ]);
});

it("keeps missing FX open positions after comparable ones and preserves equal-value order", () => {
  const holdings = [
    {
      ...emptyHolding("USD"),
      id: "unknown",
      marketValue: 10000,
      fxRateToBase: null,
    },
    { ...emptyHolding(), id: "a", marketValue: 100 },
    { ...emptyHolding(), id: "b", marketValue: 100 },
    {
      ...emptyHolding("HKD"),
      id: "invalid",
      marketValue: 9999,
      fxRateToBase: 0,
    },
  ];
  expect(sortInvestmentHoldings(holdings, snapshot).map((h) => h.id)).toEqual([
    "a",
    "b",
    "unknown",
    "invalid",
  ]);
});
