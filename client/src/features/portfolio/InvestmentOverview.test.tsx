// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { InvestmentOverview } from "./InvestmentOverview";
import { portfolioOverview } from "./portfolioOverview";
import { emptyHolding } from "./forms";
import type { HoldingTradeSummary } from "../../types";
afterEach(cleanup);
const usd = {
  ...emptyHolding("USD"),
  id: "usd",
  marketValue: 100,
  costBasis: 80,
  fxRateToBase: 7,
  fxRateObservedOn: "2026-10-12",
};
const cny = { ...emptyHolding(), id: "cny", marketValue: 300, costBasis: 200 };
const summary = { settledProfit: "10" } as HoldingTradeSummary;
it("converts all four metrics using each holding's valuation rate", () => {
  const { totals } = portfolioOverview(
    [usd, cny],
    { usd: summary, cny: summary },
    "CNY",
  );
  expect(totals).toEqual({
    value: { value: 1000, missing: 0 },
    cost: { value: 760, missing: 0 },
    unrealized: { value: 240, missing: 0 },
    realized: { value: 80, missing: 0 },
  });
  const changed = portfolioOverview(
    [
      { ...usd, fxRateToBase: 99 },
      { ...cny, fxRateToBase: 1 / 7 },
    ],
    { usd: summary },
    "USD",
  );
  expect(changed.totals.value.value).toBeCloseTo(100 + 300 / 7);
});
it("marks missing FX, cost and trades separately and never fabricates zero", () => {
  const { totals } = portfolioOverview(
    [{ ...usd, fxRateToBase: null }, cny],
    {},
    "CNY",
  );
  expect(totals.value).toEqual({ value: 300, missing: 1 });
  expect(totals.realized).toEqual({ value: null, missing: 2 });
  const missing = portfolioOverview([{ ...cny, costBasis: 0 }], {}, "CNY");
  expect(missing.totals.cost.value).toBeNull();
  expect(missing.totals.unrealized.value).toBeNull();
  expect(
    portfolioOverview([{ ...usd, fxRateToBase: NaN }], {}, "CNY").totals.value
      .value,
  ).toBeNull();
});
it("retains realized profit for closed positions with zero cost and unrealized profit", () => {
  const { totals } = portfolioOverview(
    [{ ...usd, quantity: 0, costBasis: 0, marketValue: 0 }],
    { usd: summary },
    "CNY",
  );
  expect(totals.value.value).toBe(0);
  expect(totals.cost.value).toBe(0);
  expect(totals.unrealized.value).toBe(0);
  expect(totals.realized.value).toBe(70);
});
it("renders four independent cards and discloses original currencies and valuation dates", () => {
  const { container } = render(
    <InvestmentOverview
      holdings={[usd, cny]}
      summaries={{ usd: summary }}
      baseCurrency="CNY"
    />,
  );
  expect(
    container.querySelectorAll(".investment-overview-metric"),
  ).toHaveLength(4);
  expect(screen.getByText("¥1,000")).toBeTruthy();
  expect(screen.getByText("US$100")).toBeTruthy();
  expect(screen.getByText("2026-10-12")).toBeTruthy();
  expect(screen.getByText(/1 项交易或汇率待补/)).toBeTruthy();
  expect(container.querySelector("details")?.open).toBe(false);
});
it("shows zero for an empty portfolio and preserves a known zero without FX", () => {
  expect(portfolioOverview([], {}, "CNY").totals.value.value).toBe(0);
  expect(
    portfolioOverview(
      [{ ...usd, marketValue: 0, fxRateToBase: null }],
      {},
      "CNY",
    ).totals.value,
  ).toEqual({ value: 0, missing: 0 });
});
