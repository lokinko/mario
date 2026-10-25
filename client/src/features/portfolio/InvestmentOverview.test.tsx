// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { InvestmentOverview } from "./InvestmentOverview";
import { emptyHolding } from "./forms";
import type { Snapshot } from "../../types";
afterEach(cleanup);
const holding = {
  ...emptyHolding("USD"),
  id: "usd",
  name: "美元资产",
  // Numeric projections deliberately disagree: the server's exact data is authoritative.
  marketValue: 999,
  costBasis: 888,
  fxRateToBase: 99,
  fxRateObservedOn: "2026-10-12",
};
const accounting: NonNullable<Snapshot["accounting"]> = {
  totalValue: "1200",
  holdings: {
    usd: {
      marketValue: "100.12345678",
      costBasis: "80",
      quantity: "10",
      costPrice: "8",
      currentPrice: "10.012345678",
      fxRate: "7",
      unrealizedProfit: "20.12345678",
      settledProfit: "10",
      usable: true,
    },
  },
  investments: {
    value: { value: "700.86419746", missing: 0 },
    cost: { value: "560", missing: 0 },
    unrealized: { value: "140.86419746", missing: 0 },
    realized: { value: "70", missing: 0 },
  },
  cash: {
    value: { value: "499.13580254", missing: 0 },
    cost: { value: null, missing: 1 },
    unrealized: { value: null, missing: 1 },
    realized: { value: null, missing: 1 },
  },
};
it("renders the server's exact totals and source amounts without recalculating projections", () => {
  const { container } = render(
    <InvestmentOverview holdings={[holding]} accounting={accounting} />,
  );
  expect(
    container.querySelectorAll(".investment-overview-metric"),
  ).toHaveLength(4);
  expect(screen.getByText("¥700.86")).toBeTruthy();
  expect(screen.queryByText("100.12")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "原币明细 ›" }));
  expect(screen.getByText("100.12")).toBeTruthy();
  expect(screen.getByText("2026-10-12")).toBeTruthy();
  expect(container.querySelector("details")).toBeNull();
});
it("discloses unavailable accounting instead of calculating or fabricating a zero", () => {
  const { container } = render(<InvestmentOverview holdings={[holding]} />);
  expect(screen.getByRole("alert").textContent).toContain("缺少精确会计数据");
  expect(
    container.querySelectorAll(".investment-overview-metric"),
  ).toHaveLength(0);
});
it("preserves server missing-data status and hides unusable per-holding amounts", () => {
  render(
    <InvestmentOverview
      holdings={[holding]}
      accounting={{
        ...accounting,
        investments: {
          ...accounting.investments,
          realized: { value: null, missing: 1 },
        },
        holdings: { usd: { ...accounting.holdings.usd, usable: false } },
      }}
    />,
  );
  expect(screen.getByText(/1 项交易或汇率待补/)).toBeTruthy();
  expect(screen.queryByText("100.12")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "待补 1" }));
  expect(screen.getAllByText("—")).toHaveLength(5);
});
it("uses the server's cash total and shows only the cash metric", () => {
  const { container } = render(
    <InvestmentOverview holdings={[]} accounting={accounting} cash />,
  );
  expect(
    container.querySelectorAll(".investment-overview-metric"),
  ).toHaveLength(1);
  expect(screen.getByText("¥499.14")).toBeTruthy();
});
