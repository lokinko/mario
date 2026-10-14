// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { AssetAmountRow } from "./AssetAmountRow";
import { emptyHolding } from "./forms";
import { formatMoney } from "../../lib/format";
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
  fireEvent.click(screen.getByRole("button", { name: "编辑 基金" }));
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
  expect(screen.getByText("12.35")).toBeTruthy();
  expect(screen.getByText("2.35")).toBeTruthy();
  expect(screen.getByText("成本 1.23")).toBeTruthy();
  expect(screen.getByText(formatMoney(detailed.marketValue))).toBeTruthy();
  expect(JSON.stringify(detailed)).toBe(before);
});
it.each([
  [1000, 800, 200, "+25%"],
  [800, 1000, -200, "-20%"],
  [1000, 1000, 0, "0%"],
  [0, 1000, -1000, "-100%"],
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
  expect(screen.getAllByText(/已清仓/).length).toBeGreaterThan(0);
  expect(screen.queryByLabelText("删除 基金")).toBeNull();
  rerender(
    <AssetAmountRow holding={{ ...holding, costBasis: 0 }} {...callbacks()} />,
  );
  expect(screen.getByText("成本待补")).toBeTruthy();
  rerender(
    <AssetAmountRow
      holding={{ ...holding, assetClass: "现金" }}
      {...callbacks()}
    />,
  );
  expect(screen.getAllByRole("cell")).toHaveLength(3);
});
it("keeps delete in an explicit menu and disables actions while saving", () => {
  const props = callbacks();
  const { rerender } = render(<AssetAmountRow holding={holding} {...props} />);
  fireEvent.click(screen.getByLabelText("基金更多操作"));
  fireEvent.click(screen.getByRole("button", { name: "删除 基金" }));
  expect(props.onDelete).toHaveBeenCalledOnce();
  rerender(<AssetAmountRow holding={holding} {...props} disabled />);
  fireEvent.click(screen.getByRole("button", { name: "编辑 基金" }));
  expect(props.onEdit).not.toHaveBeenCalled();
});
