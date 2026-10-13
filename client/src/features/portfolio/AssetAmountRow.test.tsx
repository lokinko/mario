// @vitest-environment jsdom
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { AssetAmountRow } from "./AssetAmountRow";
import * as portfolioApi from "../../api/portfolio";
import fixture from "../../test/emptySnapshot.json";
import { emptyHolding } from "./forms";
import type { Snapshot } from "../../types";
import { formatMoney } from "../../lib/format";
vi.mock("../../api/portfolio", () => ({
  updateHoldingAmount: vi.fn(),
  getSnapshot: vi.fn(),
}));
afterEach(cleanup);
beforeEach(() => vi.clearAllMocks());
it("distinguishes a closed position from missing cost and retains settled profit", () => {
  render(
    <AssetAmountRow
      holding={{
        ...emptyHolding(),
        id: "closed",
        name: "中国银行",
        quantity: 0,
        costPrice: 0,
        costBasis: 0,
        currentPrice: 6.77,
        marketValue: 0,
      }}
      realizedPnl={1190}
      onUpdate={vi.fn()}
      onEdit={vi.fn()}
      onDelete={vi.fn()}
      canDelete={false}
    />,
  );
  expect(screen.getByText("已清仓")).toBeTruthy();
  expect(screen.queryByText("待补成本")).toBeNull();
  expect(screen.getByText(`总成本 ${formatMoney(0)}`)).toBeTruthy();
  expect(screen.getByLabelText("中国银行已结算盈亏").textContent).toContain(
    `+${formatMoney(1190)}`,
  );
  expect(screen.getByText(formatMoney(0))).toBeTruthy();
  expect(screen.queryByText(/NaN|Infinity/)).toBeNull();
});
const holding = {
  ...emptyHolding(),
  id: "a",
  name: "工资卡",
  marketValue: 100,
};
const snapshot = {
  ...fixture,
  holdingRevisions: { a: "v2" },
  holdings: [{ ...holding, marketValue: 0 }],
} as Snapshot;
function mount() {
  const onUpdate = vi.fn();
  render(
    <AssetAmountRow
      holding={holding}
      revision="v1"
      onEdit={vi.fn()}
      onDelete={vi.fn()}
      onUpdate={onUpdate}
    />,
  );
  return onUpdate;
}
it("rounds displayed prices and computed totals without saving rounded values", async () => {
  const detailed = {
    ...holding,
    quantity: 12.345678,
    costPrice: 1.234567,
    currentPrice: 2.345678,
    marketValue: 28.959860739684,
    costBasis: 15.241566651426,
  };
  const props = {
    revision: "v1",
    onEdit: vi.fn(),
    onDelete: vi.fn(),
    onUpdate: vi.fn(),
  };
  const view = render(<AssetAmountRow {...props} holding={detailed} />);
  expect(
    screen.getByRole("button", { name: "编辑 工资卡成本" }).textContent,
  ).toBe("1.23 CNY");
  expect(
    screen.getByRole("button", { name: "编辑 工资卡持仓数" }).textContent,
  ).toBe("12.35");
  expect(
    screen.getByRole("button", { name: "编辑 工资卡当前价格" }).textContent,
  ).toBe("2.35 CNY");
  const field = screen.getByLabelText("工资卡金额") as HTMLInputElement;
  expect(field.value).toBe(formatMoney(detailed.marketValue));
  fireEvent.focus(field);
  expect(field.value).toBe(formatMoney(detailed.marketValue));
  fireEvent.blur(field);
  expect(portfolioApi.updateHoldingAmount).not.toHaveBeenCalled();

  view.rerender(
    <AssetAmountRow
      {...props}
      holding={{ ...holding, marketValue: 100.123456 }}
    />,
  );
  expect(field.value).toBe("100.12");
  fireEvent.focus(field);
  expect(field.value).toBe("100.123456");
  fireEvent.blur(field);
  expect(portfolioApi.updateHoldingAmount).not.toHaveBeenCalled();
  vi.mocked(portfolioApi.updateHoldingAmount).mockResolvedValue(snapshot);
  fireEvent.focus(field);
  fireEvent.change(field, { target: { value: "120.123456" } });
  fireEvent.blur(field);
  await screen.findByText("已保存");
  expect(portfolioApi.updateHoldingAmount).toHaveBeenCalledWith(
    "a",
    120.123456,
    "v1",
    expect.any(String),
  );
  expect(field.value).toBe("120.12");
});
it.each([
  { marketValue: 1000, costBasis: 800, pnl: 200, pct: "+25.0%" },
  { marketValue: 800, costBasis: 1000, pnl: -200, pct: "-20.0%" },
  { marketValue: 1000, costBasis: 1000, pnl: 0, pct: "0.0%" },
  { marketValue: 0, costBasis: 1000, pnl: -1000, pct: "-100.0%" },
])(
  "shows saved investment P/L for $marketValue against cost $costBasis",
  ({ marketValue, costBasis, pnl, pct }) => {
    const onEdit = vi.fn();
    render(
      <AssetAmountRow
        holding={{
          ...holding,
          name: "基金",
          currency: "USD",
          marketValue,
          costBasis,
        }}
        revision="v1"
        onEdit={onEdit}
        onDelete={vi.fn()}
        onUpdate={vi.fn()}
      />,
    );
    const cost = screen.getByRole("button", { name: "编辑 基金成本" });
    expect(cost.textContent).toBe("待补成本价");
    expect(
      screen.getByText(`总成本 ${formatMoney(costBasis, "USD")}`),
    ).toBeTruthy();
    expect(
      screen.getByText(`${pnl > 0 ? "+" : ""}${formatMoney(pnl, "USD")}`),
    ).toBeTruthy();
    expect(screen.getByText(pct)).toBeTruthy();
    fireEvent.click(cost);
    expect(onEdit).toHaveBeenCalledOnce();
    fireEvent.change(screen.getByLabelText("基金金额"), {
      target: { value: "2000" },
    });
    expect(screen.getByText(pct)).toBeTruthy();
  },
);

it("does not calculate P/L for an unrecorded cost or show cost for cash", () => {
  const props = {
    holding,
    onEdit: vi.fn(),
    onDelete: vi.fn(),
    onUpdate: vi.fn(),
  };
  const { rerender } = render(<AssetAmountRow {...props} />);
  expect(screen.getByText("待补成本")).toBeTruthy();
  expect(screen.getByText("—")).toBeTruthy();
  rerender(
    <AssetAmountRow
      {...props}
      holding={{ ...holding, assetClass: "现金", costBasis: 80 }}
    />,
  );
  expect(screen.queryByRole("button", { name: /成本/ })).toBeNull();
  expect(screen.queryByText("浮动盈亏")).toBeNull();
});
it("saves explicit zero on blur and preserves blank or invalid drafts", async () => {
  vi.mocked(portfolioApi.updateHoldingAmount).mockResolvedValue(snapshot);
  const update = mount();
  const field = screen.getByLabelText("工资卡金额");
  for (const value of ["", "-1", "abc"]) {
    fireEvent.change(field, { target: { value } });
    fireEvent.blur(field);
    expect(portfolioApi.updateHoldingAmount).not.toHaveBeenCalled();
  }
  fireEvent.change(field, { target: { value: "0" } });
  fireEvent.blur(field);
  await screen.findByText("已保存");
  expect(portfolioApi.updateHoldingAmount).toHaveBeenCalledWith(
    "a",
    0,
    "v1",
    expect.any(String),
  );
  expect(update).toHaveBeenCalledWith(snapshot);
});
it("deduplicates Enter followed by blur and reuses the request ID after failure", async () => {
  let reject!: (error: Error) => void;
  vi.mocked(portfolioApi.updateHoldingAmount).mockImplementationOnce(
    () =>
      new Promise((_, no) => {
        reject = no;
      }),
  );
  mount();
  const field = screen.getByLabelText("工资卡金额");
  fireEvent.change(field, { target: { value: "120" } });
  fireEvent.keyDown(field, { key: "Enter" });
  fireEvent.blur(field);
  expect(portfolioApi.updateHoldingAmount).toHaveBeenCalledTimes(1);
  await act(async () => reject(new Error("连接中断")));
  expect((field as HTMLInputElement).value).toBe("120");
  const first = vi.mocked(portfolioApi.updateHoldingAmount).mock.calls[0];
  vi.mocked(portfolioApi.updateHoldingAmount).mockResolvedValue(snapshot);
  fireEvent.click(screen.getByText("重试"));
  await screen.findByText("已保存");
  expect(vi.mocked(portfolioApi.updateHoldingAmount).mock.calls[1]).toEqual(
    first,
  );
});
it("does not silently authorize an old draft after a remote revision", async () => {
  const props = {
    holding,
    revision: "v1",
    onEdit: vi.fn(),
    onDelete: vi.fn(),
    onUpdate: vi.fn(),
  };
  const view = render(<AssetAmountRow {...props} />);
  const field = screen.getByLabelText("工资卡金额");
  fireEvent.change(field, { target: { value: "120" } });
  view.rerender(
    <AssetAmountRow
      {...props}
      holding={{ ...holding, marketValue: 200 }}
      revision="remote"
    />,
  );
  vi.mocked(portfolioApi.updateHoldingAmount).mockRejectedValue(
    new Error("资产已更新"),
  );
  fireEvent.blur(field);
  await screen.findByRole("alert");
  expect(portfolioApi.updateHoldingAmount).toHaveBeenCalledWith(
    "a",
    120,
    "v1",
    expect.any(String),
  );
  vi.mocked(portfolioApi.getSnapshot).mockResolvedValue({
    ...snapshot,
    holdingRevisions: { a: "remote" },
  });
  fireEvent.click(screen.getByText("核对最新金额"));
  await waitFor(() =>
    expect(screen.getByRole("alert").textContent).toContain("核对后"),
  );
  vi.mocked(portfolioApi.updateHoldingAmount).mockResolvedValue(snapshot);
  fireEvent.click(screen.getByText("重试"));
  await screen.findByText("已保存");
  expect(
    vi.mocked(portfolioApi.updateHoldingAmount).mock.calls.at(-1)?.[2],
  ).toBe("remote");
});
it("Tab saves the current amount and focuses the next amount field", async () => {
  vi.mocked(portfolioApi.updateHoldingAmount).mockResolvedValue(snapshot);
  render(
    <>
      <AssetAmountRow
        holding={holding}
        revision="v1"
        onEdit={vi.fn()}
        onDelete={vi.fn()}
        onUpdate={vi.fn()}
      />
      <AssetAmountRow
        holding={{ ...holding, id: "b", name: "基金" }}
        revision="b1"
        onEdit={vi.fn()}
        onDelete={vi.fn()}
        onUpdate={vi.fn()}
      />
    </>,
  );
  const first = screen.getByLabelText("工资卡金额");
  first.focus();
  fireEvent.change(first, { target: { value: "130" } });
  fireEvent.keyDown(first, { key: "Tab" });
  expect(document.activeElement).toBe(screen.getByLabelText("基金金额"));
  await screen.findByText("已保存");
});

it("blocks deletion while an amount save is pending", async () => {
  let resolve!: (value: Snapshot) => void;
  vi.mocked(portfolioApi.updateHoldingAmount).mockImplementationOnce(
    () =>
      new Promise((yes) => {
        resolve = yes;
      }),
  );
  const onDelete = vi.fn();
  render(
    <AssetAmountRow
      holding={holding}
      revision="v1"
      onEdit={vi.fn()}
      onDelete={onDelete}
      onUpdate={vi.fn()}
    />,
  );
  const field = screen.getByLabelText("工资卡金额");
  fireEvent.change(field, { target: { value: "130" } });
  fireEvent.blur(field);
  const remove = screen.getByRole("button", { name: "删除 工资卡" });
  expect((remove as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(remove);
  expect(onDelete).not.toHaveBeenCalled();
  await act(async () => resolve(snapshot));
  fireEvent.click(remove);
  expect(onDelete).toHaveBeenCalledOnce();
});
