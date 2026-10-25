// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { AssetAmountRow } from "./AssetAmountRow";
import { emptyHolding } from "./forms";
import { formatAmount as formatMoney } from "../../lib/format";
afterEach(cleanup);
const holding = {
  ...emptyHolding(),
  id: "a",
  name: "基金",
  marketValue: 1000,
  costBasis: 800,
};
const callbacks = () => ({
  onUpdate: vi.fn(),
  onEdit: vi.fn(),
  onOpen: vi.fn(),
  onDelete: vi.fn(),
});
it("separates detail and edit actions and never renders an editable amount", () => {
  const props = callbacks();
  const { container } = render(<AssetAmountRow holding={holding} {...props} />);
  expect(container.querySelector("input")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "查看 基金" }));
  expect(props.onOpen).toHaveBeenCalledOnce();
  expect(props.onEdit).not.toHaveBeenCalled();
  fireEvent.click(screen.getByLabelText("基金更多操作"));
  fireEvent.click(screen.getByRole("menuitem", { name: "编辑 基金" }));
  expect(props.onEdit).toHaveBeenCalledOnce();
  expect(props.onUpdate).not.toHaveBeenCalled();
});
it("rounds numbers only for display without changing the holding", () => {
  const detailed = {
    ...holding,
    quantity: 12.345678,
    costPrice: 1.234567,
    currentPrice: 2.345678,
    marketValue: 28.959860739684,
  };
  const before = JSON.stringify(detailed);
  render(<AssetAmountRow holding={detailed} {...callbacks()} />);
  expect(screen.getByText("12.3457")).toBeTruthy();
  expect(screen.getAllByText("2.3457")).toHaveLength(2);
  expect(screen.getByText("成本")).toBeTruthy();
  expect(screen.getByText(formatMoney(detailed.marketValue))).toBeTruthy();
  expect(JSON.stringify(detailed)).toBe(before);
});
it.each([
  [1000, 800, 200, "+25.00%"],
  [800, 1000, -200, "−20.00%"],
  [1000, 1000, 0, "0.00%"],
  [0, 1000, -1000, "−100.00%"],
])("shows saved P/L for %s against %s", (value, cost, pnl, pct) => {
  render(
    <AssetAmountRow
      holding={{
        ...holding,
        marketValue: value as number,
        costBasis: cost as number,
      }}
      {...callbacks()}
    />,
  );
  expect(
    screen.getByText(
      ((pnl as number) > 0 ? "+" : "") + formatMoney(pnl as number),
    ),
  ).toBeTruthy();
  expect(screen.getByText(pct)).toBeTruthy();
});
it("distinguishes closed positions from missing cost and omits investment fields for cash", () => {
  const { rerender } = render(
    <AssetAmountRow
      holding={{ ...holding, quantity: 0, costBasis: 0, marketValue: 0 }}
      realizedPnl={1190}
      canDelete={false}
      {...callbacks()}
    />,
  );
  expect(screen.queryByText("成本待补")).toBeNull();
  expect(screen.getByText("+" + formatMoney(1190))).toBeTruthy();
  expect(screen.getAllByText(/已清仓/).length).toBeGreaterThan(0);
  expect(screen.queryByLabelText("删除 基金")).toBeNull();
  rerender(
    <AssetAmountRow holding={{ ...holding, costBasis: 0 }} {...callbacks()} />,
  );
  expect(screen.getByText("成本待补")).toBeTruthy();
  expect(
    screen
      .getAllByRole("cell")
      .find((cell) => cell.dataset.label === "已结算收益")?.textContent,
  ).toBe("—");
  rerender(
    <AssetAmountRow
      holding={{ ...holding, assetClass: "现金" }}
      {...callbacks()}
    />,
  );
  expect(screen.getAllByRole("cell")).toHaveLength(4);
});
it("keeps delete in an explicit menu and disables actions while saving", () => {
  const props = callbacks();
  const { rerender } = render(<AssetAmountRow holding={holding} {...props} />);
  fireEvent.click(screen.getByLabelText("基金更多操作"));
  fireEvent.click(screen.getByRole("menuitem", { name: "删除 基金" }));
  expect(props.onDelete).toHaveBeenCalledOnce();
  rerender(<AssetAmountRow holding={holding} {...props} disabled />);
  expect(
    (screen.getByLabelText("基金更多操作") as HTMLButtonElement).disabled,
  ).toBe(true);
  expect(props.onEdit).not.toHaveBeenCalled();
});

it("hides uncertain quantities, prices and profit percentages only for an unusable holding", () => {
  const exact = {
    marketValue: "1000",
    costBasis: "800",
    quantity: "100",
    currentPrice: "10",
    costPrice: "8",
    priceStatus: "provisional",
    fxRate: "1",
    unrealizedProfit: "200",
    settledProfit: "0",
    usable: false,
  };
  const { container, rerender } = render(
    <AssetAmountRow
      holding={{ ...holding, quantity: 100, currentPrice: 10, costPrice: 8 }}
      {...callbacks()}
      exact={exact}
    />,
  );
  expect(container.querySelector(".pnl-percent")).toBeNull();
  expect(
    container.querySelector(".desktop-quantity")?.textContent?.trim(),
  ).toBe("—");
  expect(screen.queryByText("10.00")).toBeNull();
  rerender(
    <AssetAmountRow
      holding={{ ...holding, quantity: 100, currentPrice: 10, costPrice: 8 }}
      {...callbacks()}
      exact={{ ...exact, usable: true }}
    />,
  );
  expect(container.querySelector(".pnl-percent")?.textContent).toBe("+25.00%");
  expect(
    container.querySelector(".desktop-quantity")?.textContent?.trim(),
  ).toBe("100");
});
