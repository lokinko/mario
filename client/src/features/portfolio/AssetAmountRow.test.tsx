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
import * as api from "../../api";
import fixture from "../../test/emptySnapshot.json";
import { emptyHolding } from "./forms";
import type { Snapshot } from "../../types";
import { formatMoney } from "../../lib/format";
vi.mock("../../api", () => ({
  updateHoldingAmount: vi.fn(),
  getSnapshot: vi.fn(),
}));
afterEach(cleanup);
beforeEach(() => vi.clearAllMocks());
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
  vi.mocked(api.updateHoldingAmount).mockResolvedValue(snapshot);
  const update = mount();
  const field = screen.getByLabelText("工资卡金额");
  for (const value of ["", "-1", "abc"]) {
    fireEvent.change(field, { target: { value } });
    fireEvent.blur(field);
    expect(api.updateHoldingAmount).not.toHaveBeenCalled();
  }
  fireEvent.change(field, { target: { value: "0" } });
  fireEvent.blur(field);
  await screen.findByText("已保存");
  expect(api.updateHoldingAmount).toHaveBeenCalledWith(
    "a",
    0,
    "v1",
    expect.any(String),
  );
  expect(update).toHaveBeenCalledWith(snapshot);
});
it("deduplicates Enter followed by blur and reuses the request ID after failure", async () => {
  let reject!: (error: Error) => void;
  vi.mocked(api.updateHoldingAmount).mockImplementationOnce(
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
  expect(api.updateHoldingAmount).toHaveBeenCalledTimes(1);
  await act(async () => reject(new Error("连接中断")));
  expect((field as HTMLInputElement).value).toBe("120");
  const first = vi.mocked(api.updateHoldingAmount).mock.calls[0];
  vi.mocked(api.updateHoldingAmount).mockResolvedValue(snapshot);
  fireEvent.click(screen.getByText("重试"));
  await screen.findByText("已保存");
  expect(vi.mocked(api.updateHoldingAmount).mock.calls[1]).toEqual(first);
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
  vi.mocked(api.updateHoldingAmount).mockRejectedValue(new Error("资产已更新"));
  fireEvent.blur(field);
  await screen.findByRole("alert");
  expect(api.updateHoldingAmount).toHaveBeenCalledWith(
    "a",
    120,
    "v1",
    expect.any(String),
  );
  vi.mocked(api.getSnapshot).mockResolvedValue({
    ...snapshot,
    holdingRevisions: { a: "remote" },
  });
  fireEvent.click(screen.getByText("核对最新金额"));
  await waitFor(() =>
    expect(screen.getByRole("alert").textContent).toContain("核对后"),
  );
  vi.mocked(api.updateHoldingAmount).mockResolvedValue(snapshot);
  fireEvent.click(screen.getByText("重试"));
  await screen.findByText("已保存");
  expect(vi.mocked(api.updateHoldingAmount).mock.calls.at(-1)?.[2]).toBe(
    "remote",
  );
});
it("Tab saves the current amount and focuses the next amount field", async () => {
  vi.mocked(api.updateHoldingAmount).mockResolvedValue(snapshot);
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
  vi.mocked(api.updateHoldingAmount).mockImplementationOnce(
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
