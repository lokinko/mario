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
  saveHoldingTrade: vi.fn(),
  updateHoldingTrade: vi.fn(),
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
  const s = { ...value, holdingTradeSummaries: { [holding.id]: total } };
  const props = { onUpdate: vi.fn(), navigate: vi.fn(), flash: vi.fn() };
  const { rerender } = render(<MyFacts {...props} snapshot={s} />);
  const settled = screen.getByLabelText("指数基金已结算盈亏");
  expect(within(settled).getByText("+¥115")).toBeTruthy();
  expect(screen.getByText("交易明细 · 1 笔")).toBeTruthy();
  rerender(
    <MyFacts
      {...props}
      snapshot={{
        ...s,
        holdingTradeSummaries: {
          [holding.id]: { ...total, settledProfit: "120" },
        },
      }}
    />,
  );
  expect(
    within(screen.getByLabelText("指数基金已结算盈亏")).getByText("+¥120"),
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
  expect(screen.getByRole("status").textContent).toContain("旧交易记录");
  expect(screen.queryByText("期初余额")).toBeNull();
  expect(screen.queryByRole("button", { name: "撤销误录" })).toBeNull();
});
