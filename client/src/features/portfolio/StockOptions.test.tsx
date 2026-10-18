// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { StockOptions } from "./StockOptions";
import { HoldingTrades } from "./HoldingTrades";
import { MyFacts } from "./MyFacts";
import * as api from "../../api/portfolio";
import type { Holding, OptionContract, Snapshot } from "../../types";
import fixture from "../../test/emptySnapshot.json";
import { emptyHolding } from "./forms";
vi.mock("../../api/portfolio", () => ({
  saveHoldingTrade: vi.fn(),
  updateOptionPrice: vi.fn(),
  getFinancialHistory: vi
    .fn()
    .mockResolvedValue({ records: [], quarantine: null }),
  getDailyAssets: vi.fn().mockResolvedValue({
    records: [],
    timezone: "Asia/Shanghai",
    today: "2026-10-18",
    nextBefore: null,
  }),
  ensureDailyAssets: vi.fn().mockResolvedValue({
    records: [],
    timezone: "Asia/Shanghai",
    today: "2026-10-18",
    nextBefore: null,
  }),
}));
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});
const stock: Holding = {
  ...emptyHolding("CNY"),
  id: "stock",
  name: "标的股票",
  assetClass: "股票",
  quantity: 0,
  costBasis: 0,
  marketValue: 0,
};
const contract: OptionContract = {
  underlyingId: "stock",
  account: "券商A",
  optionType: "put",
  side: "short",
  strike: "10",
  expiresOn: "2026-01-30",
  multiplier: "100",
};
const option: Holding = {
  ...stock,
  id: "option",
  name: "认沽空头",
  assetClass: "期权",
  quantity: 2,
  currentPrice: 3,
  costPrice: 2.99,
  marketValue: -600,
  costBasis: -598,
};
const snapshot: Snapshot = {
  ...(fixture as Snapshot),
  holdings: [stock, option],
  optionContracts: { option: contract },
  stockOptionTotals: {
    stock: {
      marketValue: "-600",
      costBasis: "-598",
      unrealizedProfit: "-2",
      settledProfit: "0",
      optionSettledProfit: "0",
      optionUnrealizedProfit: "-2",
      usable: true,
      openContracts: 1,
    },
  },
  holdingRevisions: { option: "rev" },
  holdingTradeSummaries: {
    option: {
      account: "券商A",
      quantity: "2",
      costBasis: "-598",
      averageCost: "2.99",
      settledProfit: "0",
      realizedProfit: "0",
      dividendIncome: "0",
      totalFees: "2",
      totalTaxes: "0",
      netCashFlow: "598",
      profits: {},
      cashFlows: {},
    },
    stock: {
      account: "券商A",
      quantity: "0",
      costBasis: "0",
      averageCost: "0",
      settledProfit: "0",
      realizedProfit: "0",
      dividendIncome: "0",
      totalFees: "0",
      totalTaxes: "0",
      netCashFlow: "0",
      profits: {},
      cashFlows: {},
    },
  },
  accounting: {
    totalValue: "-600",
    holdings: {
      stock: {
        marketValue: "0",
        costBasis: "0",
        quantity: "0",
        costPrice: "0",
        currentPrice: "10",
        fxRate: "1",
        unrealizedProfit: "0",
        settledProfit: "0",
        usable: true,
      },
      option: {
        marketValue: "-600",
        costBasis: "-598",
        quantity: "2",
        costPrice: "2.99",
        currentPrice: "3",
        fxRate: "1",
        unrealizedProfit: "-2",
        settledProfit: "0",
        usable: true,
      },
    },
    investments: {
      value: { value: "-600", missing: 0 },
      cost: { value: "-598", missing: 0 },
      unrealized: { value: "-2", missing: 0 },
      realized: { value: "0", missing: 0 },
    },
    cash: {
      value: { value: "0", missing: 0 },
      cost: { value: "0", missing: 0 },
      unrealized: { value: "0", missing: 0 },
      realized: { value: "0", missing: 0 },
    },
  },
};
const callbacks = () => ({
  onUpdate: vi.fn(),
  onEditingChange: vi.fn(),
  flash: vi.fn(),
});
it("opens a short option with contract multiplier and inherited account/currency", async () => {
  vi.mocked(api.saveHoldingTrade).mockResolvedValue(snapshot);
  render(
    <HoldingTrades
      snapshot={snapshot}
      newOption={{
        contract: { ...contract, expiresOn: "2027-01-30" },
        name: "新认沽空头",
        currency: "CNY",
      }}
      {...callbacks()}
    />,
  );
  fireEvent.change(screen.getByLabelText("成交张数"), {
    target: { value: "2" },
  });
  fireEvent.change(screen.getByLabelText("每单位权利金（CNY）"), {
    target: { value: "3" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存交易并更新持仓" }));
  await waitFor(() => expect(api.saveHoldingTrade).toHaveBeenCalled());
  expect(vi.mocked(api.saveHoldingTrade).mock.calls[0][0]).toMatchObject({
    kind: "sell",
    account: "券商A",
    quantity: "2",
    price: "3",
    grossAmount: "600.00",
    newHolding: {
      assetClass: "期权",
      option: { multiplier: "100", underlyingId: "stock", side: "short" },
    },
  });
});
it("records expiry with zero premium and the remaining contract count", async () => {
  vi.mocked(api.saveHoldingTrade).mockResolvedValue(snapshot);
  render(
    <HoldingTrades snapshot={snapshot} holding={option} {...callbacks()} />,
  );
  fireEvent.click(screen.getByRole("button", { name: "记到期作废" }));
  fireEvent.click(screen.getByRole("button", { name: "保存交易并更新持仓" }));
  await waitFor(() => expect(api.saveHoldingTrade).toHaveBeenCalled());
  expect(vi.mocked(api.saveHoldingTrade).mock.calls[0][0]).toMatchObject({
    kind: "expire",
    occurredOn: contract.expiresOn,
    quantity: "2",
    price: "0",
    grossAmount: "0",
  });
  expect(screen.queryByRole("button", { name: "记现金分红" })).toBeNull();
});
it("retries an uncertain quote with exactly the same request and frozen revision", async () => {
  const cb = callbacks();
  vi.mocked(api.updateOptionPrice)
    .mockRejectedValueOnce(new Error("结果待确认"))
    .mockResolvedValueOnce(snapshot);
  render(
    <StockOptions snapshot={snapshot} stock={stock} disabled={false} {...cb} />,
  );
  fireEvent.click(screen.getByRole("button", { name: "更新期权报价" }));
  fireEvent.change(screen.getByLabelText("当前每单位权利金（CNY）"), {
    target: { value: "1.25" },
  });
  fireEvent.change(screen.getByLabelText("报价日期"), {
    target: { value: "2026-01-29" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存期权报价" }));
  await screen.findByRole("button", { name: "核对原报价保存结果" });
  expect(
    (
      screen.getByLabelText("当前每单位权利金（CNY）") as HTMLInputElement
    ).matches(":disabled"),
  ).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "核对原报价保存结果" }));
  await waitFor(() => expect(cb.onUpdate).toHaveBeenCalledWith(snapshot));
  expect(vi.mocked(api.updateOptionPrice).mock.calls[1]).toEqual(
    vi.mocked(api.updateOptionPrice).mock.calls[0],
  );
});
it("keeps a zero-share stock visible while options are open and groups their returns once", () => {
  render(<MyFacts snapshot={snapshot} onUpdate={vi.fn()} flash={vi.fn()} />);
  const list = screen.getByRole("table", { name: "持仓列表" });
  expect(
    within(list).getByRole("button", { name: "查看 标的股票" }),
  ).toBeTruthy();
  expect(within(list).queryByText("认沽空头")).toBeNull();
  expect(within(list).getByText(/含关联期权/)).toBeTruthy();
  expect(within(list).getByText("-¥600")).toBeTruthy();
  expect(within(list).getByText("-¥2")).toBeTruthy();
});
