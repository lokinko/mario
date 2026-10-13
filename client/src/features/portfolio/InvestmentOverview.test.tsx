// @vitest-environment jsdom
import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { InvestmentOverview } from "./InvestmentOverview";
import { emptyHolding } from "./forms";
import type { HoldingTradeSummary } from "../../types";
import { formatMoney } from "../../lib/format";

afterEach(cleanup);
const summary = { settledProfit: "1190" } as HoldingTradeSummary;
it("separates currencies, includes settled profit from closed positions, and marks incomplete totals", () => {
  render(
    <InvestmentOverview
      holdings={[
        {
          ...emptyHolding(),
          id: "closed",
          name: "已清仓股票",
          quantity: 0,
          marketValue: 0,
          costBasis: 0,
        },
        {
          ...emptyHolding(),
          id: "open",
          name: "持有股票",
          quantity: 100,
          marketValue: 1000,
          costBasis: 800,
        },
        {
          ...emptyHolding("USD"),
          id: "usd",
          name: "美元基金",
          marketValue: 200,
          costBasis: 150,
        },
        {
          ...emptyHolding(),
          id: "missing",
          name: "成本待补",
          marketValue: 300,
          costBasis: 0,
        },
      ]}
      summaries={{ closed: summary }}
    />,
  );
  const metrics = Array.from(
    screen
      .getByLabelText("投资持仓概览")
      .querySelectorAll(".investment-overview-metric"),
  );
  expect(
    within(metrics[0] as HTMLElement).getByText(formatMoney(1300)),
  ).toBeTruthy();
  expect(
    within(metrics[0] as HTMLElement).getByText(formatMoney(200, "USD")),
  ).toBeTruthy();
  expect(
    within(metrics[1] as HTMLElement).getByText(formatMoney(800)),
  ).toBeTruthy();
  expect(
    within(metrics[2] as HTMLElement).getByText(`+${formatMoney(200)}`),
  ).toBeTruthy();
  expect(
    within(metrics[3] as HTMLElement).getByText(`+${formatMoney(1190)}`),
  ).toBeTruthy();
  expect(within(metrics[3] as HTMLElement).getByText("—")).toBeTruthy();
  expect(screen.getByText(/1 项成本待补/).textContent).toContain(
    "3 项未录入交易",
  );
});

it("shows zero cost and unrealized profit for an entirely closed portfolio", () => {
  render(
    <InvestmentOverview
      holdings={[
        {
          ...emptyHolding(),
          id: "closed",
          name: "股票",
          quantity: 0,
          marketValue: 0,
          costBasis: 0,
        },
      ]}
      summaries={{ closed: summary }}
    />,
  );
  expect(screen.getAllByText(formatMoney(0))).toHaveLength(3);
  expect(screen.queryByText(/成本待补/)).toBeNull();
});
