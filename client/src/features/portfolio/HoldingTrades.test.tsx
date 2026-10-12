// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { HoldingTrades } from "./HoldingTrades";
import { MyFacts } from "./MyFacts";
import * as api from "../../api";
import type { HoldingTrade, Snapshot } from "../../types";
import fixture from "../../test/emptySnapshot.json";
import { emptyHolding } from "./forms";
vi.mock("../../api", () => ({
  saveHoldingTrade: vi.fn(),
  getDailyAssets: vi
    .fn()
    .mockResolvedValue({
      timezone: "Asia/Shanghai",
      today: "2026-10-12",
      records: [],
      nextBefore: null,
    }),
  ensureDailyAssets: vi
    .fn()
    .mockResolvedValue({
      timezone: "Asia/Shanghai",
      today: "2026-10-12",
      records: [],
      nextBefore: null,
    }),
}));
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  vi.restoreAllMocks();
});
const base = fixture as Snapshot;
const holding = {
  ...emptyHolding("CNY"),
  id: "asset-1",
  name: "指数基金",
  assetClass: "基金" as const,
  quantity: 10,
  costPrice: 10,
  currentPrice: 12,
  costBasis: 100,
  marketValue: 120,
  valuationDate: "2026-01-01",
};
function record(
  id: string,
  kind: HoldingTrade["kind"],
  quantity: number,
  price: number,
  date: string,
  extra = {},
): HoldingTrade {
  return {
    id,
    holdingId: holding.id,
    kind,
    quantity,
    price,
    occurredOn: date,
    fees: 0,
    currency: "CNY",
    account: kind === "opening" ? "" : "账户A",
    externalId: "",
    note: "",
    voidOf: null,
    createdAt: `${date}T00:00:00Z`,
    ...extra,
  };
}
it("creates a holding atomically from a real execution and preserves the retry ID after a lost response", async () => {
  const onUpdate = vi.fn();
  vi.mocked(api.saveHoldingTrade)
    .mockRejectedValueOnce(new Error("连接中断"))
    .mockResolvedValueOnce(base);
  render(<HoldingTrades snapshot={base} onUpdate={onUpdate} flash={vi.fn()} />);
  fireEvent.change(screen.getByRole("textbox", { name: "资产名称" }), {
    target: { value: "港股基金" },
  });
  fireEvent.change(screen.getByRole("textbox", { name: "账户 / 券商" }), {
    target: { value: "账户A" },
  });
  fireEvent.change(screen.getByRole("spinbutton", { name: "成交数量" }), {
    target: { value: "3.5" },
  });
  fireEvent.change(
    screen.getByRole("spinbutton", { name: "成交单价（CNY）" }),
    { target: { value: "10" } },
  );
  fireEvent.click(screen.getByText("费用、成交编号和备注（选填）"));
  fireEvent.change(
    screen.getByRole("spinbutton", { name: "手续费与税费合计（CNY）" }),
    { target: { value: "2" } },
  );
  fireEvent.change(screen.getByRole("textbox", { name: "券商成交编号" }), {
    target: { value: "receipt-123" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存交易并更新持仓" }));
  await screen.findByRole("alert");
  expect(screen.getByRole("spinbutton", { name: "成交数量" })).toHaveProperty(
    "value",
    "3.5",
  );
  const first = vi.mocked(api.saveHoldingTrade).mock.calls[0][0];
  expect(first).toMatchObject({
    holdingId: "",
    kind: "buy",
    quantity: 3.5,
    price: 10,
    fees: 2,
    account: "账户A",
    externalId: "receipt-123",
    newHolding: { name: "港股基金", quantity: 0, costBasis: 0, marketValue: 0 },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存交易并更新持仓" }));
  await waitFor(() => expect(onUpdate).toHaveBeenCalledWith(base));
  expect(vi.mocked(api.saveHoldingTrade).mock.calls[1][0].requestId).toBe(
    first.requestId,
  );
});
it("uses frozen opening balances and excludes voided executions from realized profit", () => {
  const trades = [
    record("o", "opening", 10, 10, "2026-01-01"),
    record("b", "buy", 10, 20, "2026-01-02"),
    record("s", "sell", 5, 30, "2026-01-03", { fees: 1 }),
    record("v", "void", 0, 0, "2026-01-04", { voidOf: "b", note: "重复买入" }),
  ];
  const value = {
    ...base,
    holdings: [{ ...holding, quantity: 5, costBasis: 50 }],
    holdingTrades: trades,
    holdingRevisions: { [holding.id]: "latest" },
  };
  render(
    <HoldingTrades
      snapshot={value}
      holding={value.holdings[0]}
      onUpdate={vi.fn()}
      flash={vi.fn()}
    />,
  );
  expect(screen.getByText("买入 · 已撤销")).toBeTruthy();
  expect(screen.getByText("¥99.00")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "记卖出" }));
  fireEvent.change(screen.getByRole("spinbutton", { name: "成交数量" }), {
    target: { value: "6" },
  });
  fireEvent.change(
    screen.getByRole("spinbutton", { name: "成交单价（CNY）" }),
    { target: { value: "30" } },
  );
  expect(
    screen.getByRole("button", { name: "保存交易并更新持仓" }),
  ).toHaveProperty("disabled", true);
});
it("shows trade details inside MyFacts and locks transaction quantities while allowing price updates", () => {
  const value = {
    ...base,
    holdings: [holding],
    holdingTrades: [
      record("o", "opening", 0, 0, "2026-01-01"),
      record("b", "buy", 10, 10, "2026-01-01"),
    ],
    holdingRevisions: { [holding.id]: "latest" },
  };
  render(
    <MyFacts
      snapshot={value}
      onUpdate={vi.fn()}
      navigate={vi.fn()}
      flash={vi.fn()}
    />,
  );
  expect(screen.getByText("交易明细 · 1 笔")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "删除 指数基金" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "编辑 指数基金" }));
  expect(
    screen.getByRole("spinbutton", { name: "持仓数 份 / 股 / 单位" }),
  ).toHaveProperty("readOnly", true);
  expect(
    screen.getByRole("spinbutton", { name: "成本价（选填） CNY" }),
  ).toHaveProperty("readOnly", true);
  expect(
    screen.getByRole("spinbutton", { name: "当前价格 CNY" }),
  ).toHaveProperty("readOnly", false);
});
