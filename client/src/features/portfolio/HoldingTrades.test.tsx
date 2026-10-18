// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { HoldingTrades } from "./HoldingTrades";
import { MyFacts } from "./MyFacts";
import * as portfolioApi from "../../api/portfolio";
import type { HoldingTrade, HoldingTradeSummary, Snapshot } from "../../types";
import fixture from "../../test/emptySnapshot.json";
import { emptyHolding } from "./forms";
vi.mock("../../api/portfolio", () => ({
  getFinancialHistory: vi
    .fn()
    .mockResolvedValue({ records: [], quarantine: null }),
  saveHoldingTrade: vi.fn(),
  updateHoldingTrade: vi.fn(),
  deleteHoldingTrade: vi.fn(),
  getFxSettings: vi.fn().mockResolvedValue({
    baseCurrency: "CNY",
    checkedAt: 0,
    requestedOn: "",
    quotes: [],
    manual: {},
    error: "",
  }),
  getDailyAssets: vi.fn().mockResolvedValue({
    timezone: "Asia/Shanghai",
    today: "2026-10-12",
    records: [],
    nextBefore: null,
  }),
  ensureDailyAssets: vi.fn().mockResolvedValue({
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
  valuationDate: "2026-10-12",
};
const buy: HoldingTrade = {
  id: "buy",
  holdingId: holding.id,
  kind: "buy",
  occurredOn: "2026-01-01",
  sequence: 1,
  quantity: "10",
  price: "10",
  grossAmount: "100",
  fees: "2",
  taxes: "1",
  currency: "CNY",
  account: "券商A",
  externalId: "receipt",
  note: "原备注",
  createdAt: "2026-01-01T00:00:00Z",
  updatedAt: "2026-01-01T00:00:00Z",
};
const summary: HoldingTradeSummary = {
  account: "券商A",
  quantity: "10",
  costBasis: "103",
  averageCost: "10.3",
  realizedProfit: "0",
  dividendIncome: "0",
  settledProfit: "0",
  totalFees: "2",
  totalTaxes: "1",
  netCashFlow: "-103",
  profits: {},
  cashFlows: { buy: "-103" },
};
const value: Snapshot = {
  ...base,
  holdings: [holding],
  holdingTrades: [buy],
  holdingRevisions: { [holding.id]: "initial" },
  holdingTradeSummaries: { [holding.id]: summary },
};
const change = (label: string, value: string) =>
  fireEvent.change(screen.getByLabelText(label), { target: { value } });
it("pages five newest executions per holding and can edit older pages without losing the full history", () => {
  const trades: HoldingTrade[] = Array.from({ length: 12 }, (_, index) => ({
    ...buy,
    id: `record-${index + 1}`,
    kind: index % 2 ? "sell" : "buy",
    occurredOn: `2026-01-${String(Math.floor(index / 2) + 1).padStart(2, "0")}`,
    sequence: (index % 2) + 1,
    externalId: `receipt-${index + 1}`,
    note: `note-${index + 1}`,
  }));
  const props = { holding, onUpdate: vi.fn(), flash: vi.fn() };
  const { container, rerender } = render(
    <HoldingTrades
      {...props}
      snapshot={{
        ...value,
        holdingTrades: [
          ...trades,
          { ...buy, holdingId: "other", externalId: "other" },
        ],
      }}
    />,
  );
  const receipts = () =>
    Array.from(container.querySelectorAll("tbody tr")).map(
      (row) => row.textContent?.match(/receipt-\d+/)?.[0],
    );
  expect(receipts()).toEqual([
    "receipt-12",
    "receipt-11",
    "receipt-10",
    "receipt-9",
    "receipt-8",
  ]);
  expect(screen.getByText("第 1 / 3 页")).toBeTruthy();
  expect(screen.getByRole("button", { name: "上一页" })).toHaveProperty(
    "disabled",
    true,
  );
  expect(container.querySelector("strong.trade-kind-buy")?.textContent).toBe(
    "买入",
  );
  expect(container.querySelector("strong.trade-kind-sell")?.textContent).toBe(
    "卖出",
  );
  fireEvent.click(screen.getByRole("button", { name: "下一页" }));
  expect(receipts()).toEqual([
    "receipt-7",
    "receipt-6",
    "receipt-5",
    "receipt-4",
    "receipt-3",
  ]);
  fireEvent.click(screen.getAllByRole("button", { name: "编辑交易" })[0]);
  expect(screen.getByLabelText("交易备注")).toHaveProperty("value", "note-7");
  fireEvent.click(screen.getByRole("button", { name: "关闭交易录入" }));
  fireEvent.click(screen.getByRole("button", { name: "下一页" }));
  expect(receipts()).toEqual(["receipt-2", "receipt-1"]);
  expect(screen.getByRole("button", { name: "下一页" })).toHaveProperty(
    "disabled",
    true,
  );
  fireEvent.click(screen.getByRole("button", { name: "上一页" }));
  expect(receipts()).toHaveLength(5);
  rerender(
    <HoldingTrades
      {...props}
      snapshot={{ ...value, holdingTrades: trades.slice(0, 2) }}
    />,
  );
  expect(screen.getByText("第 1 / 1 页")).toBeTruthy();
  expect(receipts()).toEqual(["receipt-2", "receipt-1"]);
});
it("confirms deletion, prevents duplicate writes and refreshes the empty position", async () => {
  const confirm = vi
    .spyOn(window, "confirm")
    .mockReturnValueOnce(false)
    .mockReturnValue(true);
  let complete!: (snapshot: Snapshot) => void;
  vi.mocked(portfolioApi.deleteHoldingTrade).mockImplementationOnce(
    () =>
      new Promise<Snapshot>((resolve) => {
        complete = resolve;
      }),
  );
  const onUpdate = vi.fn();
  const flash = vi.fn();
  const props = { holding, onUpdate, flash };
  const { rerender } = render(<HoldingTrades {...props} snapshot={value} />);
  fireEvent.click(screen.getByRole("button", { name: "删除交易" }));
  expect(portfolioApi.deleteHoldingTrade).not.toHaveBeenCalled();
  expect(confirm).toHaveBeenCalledWith(
    expect.stringContaining("2026-01-01 的第 1 笔买入"),
  );
  fireEvent.click(screen.getByRole("button", { name: "删除交易" }));
  fireEvent.click(screen.getByRole("button", { name: "删除交易" }));
  expect(portfolioApi.deleteHoldingTrade).toHaveBeenCalledTimes(1);
  expect(portfolioApi.deleteHoldingTrade).toHaveBeenCalledWith(buy.id, {
    expectedRevision: "initial",
  });
  expect(screen.getByRole("button", { name: "编辑交易" })).toHaveProperty(
    "disabled",
    true,
  );
  const updated = {
    ...value,
    holdingTrades: [],
    holdingTradeSummaries: {},
    holdings: [{ ...holding, quantity: 0, costBasis: 0, marketValue: 0 }],
  };
  complete(updated);
  await waitFor(() => expect(onUpdate).toHaveBeenCalledWith(updated));
  expect(flash).toHaveBeenCalledWith("交易已删除，持仓和盈亏已更新");
  rerender(
    <HoldingTrades
      {...props}
      holding={updated.holdings[0]}
      snapshot={updated}
    />,
  );
  expect(screen.queryByRole("button", { name: "删除交易" })).toBeNull();
  expect(screen.getByText(/还没有真实交易记录/)).toBeTruthy();
});
it("keeps a rejected deletion visible and allows retry after refreshing the holding", async () => {
  vi.spyOn(window, "confirm").mockReturnValue(true);
  vi.mocked(portfolioApi.deleteHoldingTrade)
    .mockRejectedValueOnce(new Error("无法删除这笔交易：卖出超过当时持仓"))
    .mockResolvedValueOnce(value);
  const props = { holding, onUpdate: vi.fn(), flash: vi.fn() };
  const { rerender } = render(<HoldingTrades {...props} snapshot={value} />);
  fireEvent.click(screen.getByRole("button", { name: "删除交易" }));
  expect((await screen.findByRole("alert")).textContent).toContain(
    "卖出超过当时持仓",
  );
  expect(screen.getByText("receipt")).toBeTruthy();
  expect(props.onUpdate).not.toHaveBeenCalled();
  rerender(
    <HoldingTrades
      {...props}
      snapshot={{ ...value, holdingRevisions: { [holding.id]: "refreshed" } }}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "删除交易" }));
  await waitFor(() => expect(props.onUpdate).toHaveBeenCalledWith(value));
  expect(portfolioApi.deleteHoldingTrade).toHaveBeenLastCalledWith(buy.id, {
    expectedRevision: "refreshed",
  });
  fireEvent.click(screen.getByRole("button", { name: "编辑交易" }));
  expect(screen.getByRole("button", { name: "删除交易" })).toHaveProperty(
    "disabled",
    true,
  );
});
it("deleting the only record on the last page returns to the preceding page", async () => {
  vi.spyOn(window, "confirm").mockReturnValue(true);
  const trades = Array.from({ length: 6 }, (_, i) => ({
    ...buy,
    id: `record-${i}`,
    sequence: i + 1,
    externalId: `receipt-${i}`,
  }));
  const updated = { ...value, holdingTrades: trades.slice(1) };
  vi.mocked(portfolioApi.deleteHoldingTrade).mockResolvedValueOnce(updated);
  const onUpdate = vi.fn();
  const props = { holding, onUpdate, flash: vi.fn() };
  const { rerender } = render(
    <HoldingTrades {...props} snapshot={{ ...value, holdingTrades: trades }} />,
  );
  fireEvent.click(screen.getByRole("button", { name: "下一页" }));
  expect(screen.getByText("第 2 / 2 页")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "删除交易" }));
  await waitFor(() => expect(onUpdate).toHaveBeenCalledWith(updated));
  rerender(<HoldingTrades {...props} snapshot={updated} />);
  expect(screen.getByText("第 1 / 1 页")).toBeTruthy();
  expect(screen.getAllByRole("button", { name: "删除交易" })).toHaveLength(5);
  expect(screen.queryByText("receipt-0")).toBeNull();
});
it("records exact real executions, separated charges and retry identity without an artificial opening", async () => {
  const onUpdate = vi.fn();
  vi.mocked(portfolioApi.saveHoldingTrade)
    .mockRejectedValueOnce(new Error("连接中断"))
    .mockResolvedValueOnce(base);
  render(<HoldingTrades snapshot={base} onUpdate={onUpdate} flash={vi.fn()} />);
  change("证券名称", "港股基金");
  change("账户 / 券商", "券商A");
  change("成交数量", "0.1");
  change("成交单价（CNY）", "0.2");
  change("成交日期", "2026-01-01");
  change("交易费用（CNY）", "0.01");
  change("税费（CNY）", "0.01");
  fireEvent.click(screen.getByRole("button", { name: "保存交易并更新持仓" }));
  await screen.findByRole("alert");
  expect(screen.getByLabelText("成交数量")).toHaveProperty("value", "0.1");
  const first = vi.mocked(portfolioApi.saveHoldingTrade).mock.calls[0][0];
  expect(first).toMatchObject({
    kind: "buy",
    sequence: 1,
    quantity: "0.1",
    price: "0.2",
    grossAmount: "0.02",
    fees: "0.01",
    taxes: "0.01",
    newHolding: {
      name: "港股基金",
      symbol: "",
      assetClass: "股票",
      currency: "CNY",
    },
  });
  expect(first).not.toHaveProperty("voidOf");
  expect(first.newHolding).not.toHaveProperty("currentPrice");
  fireEvent.click(screen.getByRole("button", { name: "保存交易并更新持仓" }));
  await waitFor(() => expect(onUpdate).toHaveBeenCalledWith(base));
  expect(
    vi.mocked(portfolioApi.saveHoldingTrade).mock.calls[1][0].requestId,
  ).toBe(first.requestId);
});
it("edits the original record and freezes its revision across a background refresh", async () => {
  const onUpdate = vi.fn();
  vi.mocked(portfolioApi.updateHoldingTrade)
    .mockRejectedValueOnce(new Error("连接中断"))
    .mockResolvedValueOnce(value);
  const { rerender } = render(
    <HoldingTrades
      snapshot={value}
      holding={holding}
      onUpdate={onUpdate}
      flash={vi.fn()}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "编辑交易" }));
  expect(screen.getByLabelText("成交数量")).toHaveProperty("value", "10");
  expect(screen.getByLabelText("当日顺序")).toHaveProperty("value", "1");
  change("成交数量", "12");
  change("成交金额（CNY）", "120");
  change("交易备注", "修正");
  fireEvent.click(screen.getByRole("button", { name: "保存修改并更新持仓" }));
  await screen.findByRole("alert");
  rerender(
    <HoldingTrades
      snapshot={{ ...value, holdingRevisions: { [holding.id]: "background" } }}
      holding={holding}
      onUpdate={onUpdate}
      flash={vi.fn()}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "保存修改并更新持仓" }));
  await waitFor(() => expect(onUpdate).toHaveBeenCalled());
  expect(portfolioApi.updateHoldingTrade).toHaveBeenNthCalledWith(2, buy.id, {
    expectedRevision: "initial",
    settlementMode: "history",
    kind: "buy",
    occurredOn: buy.occurredOn,
    sequence: 1,
    quantity: "12",
    price: "10",
    grossAmount: "120",
    fees: "2",
    taxes: "1",
    account: "券商A",
    externalId: "receipt",
    note: "修正",
  });
  expect(portfolioApi.saveHoldingTrade).not.toHaveBeenCalled();
  expect(screen.queryByRole("form")).toBeNull();
});
it("records cash dividends with zero share movement, using net income after both charges", async () => {
  vi.mocked(portfolioApi.saveHoldingTrade).mockResolvedValue(value);
  render(
    <HoldingTrades
      snapshot={value}
      holding={holding}
      onUpdate={vi.fn()}
      flash={vi.fn()}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "记现金分红" }));
  expect(screen.queryByLabelText("成交数量")).toBeNull();
  change("税前现金分红（CNY）", "20");
  change("交易费用（CNY）", "1");
  change("税费（CNY）", "3");
  change("分红到账日期", "2026-01-01");
  expect(screen.getByLabelText("当日顺序")).toHaveProperty("value", "2");
  expect(screen.getByText("¥16")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "保存交易并更新持仓" }));
  await waitFor(() => expect(portfolioApi.saveHoldingTrade).toHaveBeenCalled());
  expect(portfolioApi.saveHoldingTrade).toHaveBeenCalledWith(
    expect.objectContaining({
      kind: "dividend",
      quantity: "0",
      price: "0",
      grossAmount: "20",
      fees: "1",
      taxes: "3",
      sequence: 2,
    }),
  );
});
it("permits historical sells on a closed position and cancels edits without a write", () => {
  const closed = { ...holding, quantity: 0, costBasis: 0, marketValue: 0 };
  render(
    <HoldingTrades
      snapshot={value}
      holding={closed}
      onUpdate={vi.fn()}
      flash={vi.fn()}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "记卖出" }));
  change("成交日期", "2026-01-02");
  change("成交数量", "10");
  change("成交单价（CNY）", "20");
  expect(
    screen.getByRole("button", { name: "保存交易并更新持仓" }),
  ).toHaveProperty("disabled", false);
  expect(screen.getByLabelText("成交日期").getAttribute("min")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "关闭交易录入" }));
  fireEvent.click(screen.getByRole("button", { name: "编辑交易" }));
  fireEvent.click(screen.getByRole("button", { name: "关闭交易录入" }));
  expect(portfolioApi.updateHoldingTrade).not.toHaveBeenCalled();
});
it("shows authoritative sell profits and dividends in the holding overview and detail, including cleared-record notices", () => {
  const total = {
    ...summary,
    realizedProfit: "99",
    dividendIncome: "16",
    settledProfit: "115",
  };
  const s = {
    ...value,
    holdingTradeSummaries: { [holding.id]: total },
    accounting: {
      ...value.accounting!,
      investments: {
        ...value.accounting!.investments,
        realized: { value: "115", missing: 0 },
      },
    },
  };
  const props = { onUpdate: vi.fn(), flash: vi.fn() };
  const { rerender } = render(<MyFacts {...props} snapshot={s} />);
  const settled = screen.getByLabelText("投资持仓概览");
  expect(within(settled).getByText("+¥115")).toBeTruthy();
  expect(
    within(screen.getByRole("table", { name: "持仓列表" })).getByText("+¥115"),
  ).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "查看 指数基金" }));
  expect(screen.getByRole("heading", { name: "交易明细" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "关闭详情" }));
  rerender(
    <MyFacts
      {...props}
      snapshot={{
        ...s,
        holdingTradeSummaries: {
          [holding.id]: { ...total, settledProfit: "120" },
        },
        accounting: {
          ...s.accounting,
          investments: {
            ...s.accounting.investments,
            realized: { value: "120", missing: 0 },
          },
        },
      }}
    />,
  );
  expect(
    within(screen.getByLabelText("投资持仓概览")).getByText("+¥120"),
  ).toBeTruthy();
  expect(
    within(screen.getByRole("table", { name: "持仓列表" })).getByText("+¥120"),
  ).toBeTruthy();
  cleanup();
  render(
    <HoldingTrades
      snapshot={{
        ...value,
        holdingTrades: [],
        holdingTradeSummaries: {},
        clearedTradeHoldings: [holding.id],
      }}
      holding={holding}
      onUpdate={vi.fn()}
      flash={vi.fn()}
    />,
  );
  expect(screen.getByRole("status").textContent).toContain("交易链已隔离");
  expect(screen.queryByText("期初余额")).toBeNull();
  expect(screen.queryByRole("button", { name: "撤销误录" })).toBeNull();
});

it("keeps the asset editor unavailable while a transaction draft is open", async () => {
  render(<MyFacts snapshot={value} onUpdate={vi.fn()} flash={vi.fn()} />);
  fireEvent.click(screen.getByRole("button", { name: "查看 指数基金" }));
  fireEvent.click(screen.getByRole("button", { name: "编辑交易" }));
  expect(screen.getByRole("button", { name: "编辑资产" })).toHaveProperty(
    "disabled",
    true,
  );
  fireEvent.click(screen.getByRole("button", { name: "关闭交易录入" }));
  expect(screen.getByRole("button", { name: "编辑资产" })).toHaveProperty(
    "disabled",
    false,
  );
});

it("keeps an uncertain edit frozen and retries exactly the original content", async () => {
  vi.mocked(portfolioApi.updateHoldingTrade)
    .mockRejectedValueOnce(new Error("操作结果待确认"))
    .mockResolvedValueOnce(value);
  const onUpdate = vi.fn();
  render(
    <HoldingTrades
      snapshot={value}
      holding={holding}
      onUpdate={onUpdate}
      flash={vi.fn()}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "编辑交易" }));
  change("交易备注", "需要保留的原请求");
  fireEvent.click(screen.getByRole("button", { name: "保存修改并更新持仓" }));
  await screen.findByRole("button", { name: "再次核对原操作" });
  const first = vi.mocked(portfolioApi.updateHoldingTrade).mock.calls[0];
  expect(screen.getByLabelText("交易备注").closest("fieldset")).toHaveProperty(
    "disabled",
    true,
  );
  expect(screen.getByRole("button", { name: "关闭交易录入" })).toHaveProperty(
    "disabled",
    true,
  );
  expect(onUpdate).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "再次核对原操作" }));
  await waitFor(() => expect(onUpdate).toHaveBeenCalledWith(value));
  expect(vi.mocked(portfolioApi.updateHoldingTrade).mock.calls[1]).toEqual(
    first,
  );
  expect(portfolioApi.saveHoldingTrade).not.toHaveBeenCalled();
});
