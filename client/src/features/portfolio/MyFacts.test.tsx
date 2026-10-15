// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { MyFacts } from "./MyFacts";
import * as portfolioApi from "../../api/portfolio";
import type { HoldingTrade, HoldingTradeSummary, Snapshot } from "../../types";
import fixture from "../../test/emptySnapshot.json";
import { emptyHolding } from "./forms";
import { formatMoney } from "../../lib/format";
vi.mock("../../api/portfolio", () => ({
  getFinancialHistory: vi
    .fn()
    .mockResolvedValue({ records: [], quarantine: null }),
  ensureDailyAssets: vi.fn().mockResolvedValue({
    timezone: "Asia/Shanghai",
    today: "2026-09-13",
    records: [],
    nextBefore: null,
  }),
  getDailyAssets: vi.fn().mockResolvedValue({
    timezone: "Asia/Shanghai",
    today: "2026-09-13",
    records: [],
    nextBefore: null,
  }),
  updateHoldingAmount: vi.fn(),
  saveProfile: vi.fn(),
  saveHolding: vi.fn(),
  updateHolding: vi.fn(),
  deleteHolding: vi.fn(),
  refreshSecurityPrices: vi.fn(),
  getFxSettings: vi.fn(),
}));
beforeEach(() => {
  vi.mocked(portfolioApi.getFinancialHistory).mockResolvedValue({
    records: [],
    quarantine: null,
  });
  vi.mocked(portfolioApi.getFxSettings).mockResolvedValue({
    baseCurrency: "CNY",
    checkedAt: 0,
    requestedOn: "",
    quotes: [],
    manual: {},
    error: "",
  });
  vi.mocked(portfolioApi.getDailyAssets).mockResolvedValue({
    timezone: "UTC",
    today: "2026-09-13",
    records: [],
    nextBefore: null,
  });
});
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
  vi.restoreAllMocks();
});
const snapshot = fixture as Snapshot;

it("searches the aligned holding list without changing portfolio totals or saving data", async () => {
  const holdings = [
    {
      ...emptyHolding("USD"),
      id: "apple",
      name: "Apple",
      symbol: "AAPL",
      quantity: 10,
      marketValue: 100,
      costBasis: 80,
      fxRateToBase: 7,
    },
    {
      ...emptyHolding(),
      id: "fund",
      name: "宽基基金",
      symbol: "159915.SZ",
      quantity: 20,
      marketValue: 300,
      costBasis: 200,
    },
  ];
  const view = mount({ ...snapshot, holdings });
  const list = screen.getByRole("table", { name: "持仓列表" });
  expect(within(list).getAllByRole("columnheader")).toHaveLength(6);
  const overview = await screen.findByLabelText("投资持仓概览");
  const totals = overview.textContent;
  const search = screen.getByRole("searchbox", { name: "搜索持仓" });
  fireEvent.change(search, { target: { value: " aapl " } });
  expect(within(list).getByRole("button", { name: "编辑 Apple" })).toBeTruthy();
  expect(
    within(list).queryByRole("button", { name: "编辑 宽基基金" }),
  ).toBeNull();
  expect(overview.textContent).toBe(totals);
  fireEvent.change(search, { target: { value: "cny" } });
  expect(
    within(list).getByRole("button", { name: "编辑 宽基基金" }),
  ).toBeTruthy();
  fireEvent.change(search, { target: { value: "找不到" } });
  expect(within(list).getByText("没有找到匹配的持仓。")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "清除筛选" }));
  fireEvent.change(screen.getByRole("combobox", { name: "持仓排序" }), {
    target: { value: "value-asc" },
  });
  const order = () =>
    Array.from(
      view.container.querySelectorAll(".investment-assets .asset-name strong"),
    ).map((el) => el.textContent);
  expect(order()).toEqual(["宽基基金", "Apple"]);
  fireEvent.change(screen.getByRole("combobox", { name: "持仓排序" }), {
    target: { value: "value-desc" },
  });
  expect(order()).toEqual(["Apple", "宽基基金"]);
  expect(portfolioApi.updateHoldingAmount).not.toHaveBeenCalled();
  expect(portfolioApi.updateHolding).not.toHaveBeenCalled();
});

it("displays sorted positions in all filters and reorders after values or trade dates change", async () => {
  const holdings = [
    { ...emptyHolding(), id: "old", name: "较早清仓", quantity: 0 },
    {
      ...emptyHolding(),
      id: "small",
      name: "小额持仓",
      quantity: 1,
      marketValue: 100,
    },
    { ...emptyHolding(), id: "new", name: "近期清仓", quantity: 0 },
    {
      ...emptyHolding(),
      id: "large",
      name: "大额持仓",
      quantity: 10,
      marketValue: 1000,
    },
  ];
  const trades: HoldingTrade[] = ["old", "new"].map((id, index) => ({
    id: `trade-${id}`,
    holdingId: id,
    kind: "sell",
    currency: "CNY",
    occurredOn: index ? "2026-10-12" : "2026-10-10",
    sequence: 1,
    quantity: "1",
    price: "1",
    grossAmount: "1",
    fees: "0",
    taxes: "0",
    account: "账户",
    externalId: "",
    note: "",
    createdAt: "2026-10-13T12:00:00Z",
    updatedAt: "2026-10-13T12:00:00Z",
  }));
  const next = {
    ...snapshot,
    holdings,
    holdingTrades: trades,
    holdingTradeSummaries: {
      old: {
        settledProfit: "0",
        cashFlows: {},
        profits: {},
      } as HoldingTradeSummary,
      new: {
        settledProfit: "0",
        cashFlows: {},
        profits: {},
      } as HoldingTradeSummary,
    },
  };
  const view = mount(next);
  const order = () =>
    Array.from(
      view.container.querySelectorAll(".investment-assets .asset-name strong"),
    ).map((el) => el.textContent);
  await screen.findByLabelText("投资持仓概览");
  expect(order()).toEqual(["大额持仓", "小额持仓", "近期清仓", "较早清仓"]);
  fireEvent.click(screen.getByRole("button", { name: "未清仓 2" }));
  expect(order()).toEqual(["大额持仓", "小额持仓"]);
  fireEvent.click(screen.getByRole("button", { name: "已清仓 2" }));
  expect(order()).toEqual(["近期清仓", "较早清仓"]);
  view.rerender(
    <MyFacts
      snapshot={{
        ...next,
        holdings: holdings.map((h) =>
          h.id === "small" ? { ...h, marketValue: 2000 } : h,
        ),
        holdingTrades: trades.map((t) =>
          t.holdingId === "old" ? { ...t, occurredOn: "2026-10-13" } : t,
        ),
      }}
      onUpdate={vi.fn()}
      navigate={vi.fn()}
      flash={vi.fn()}
    />,
  );
  expect(order()).toEqual(["较早清仓", "近期清仓"]);
  fireEvent.click(screen.getByRole("button", { name: "全部 4" }));
  expect(order()).toEqual(["小额持仓", "大额持仓", "较早清仓", "近期清仓"]);
});

it("filters closed positions without changing the full portfolio overview", async () => {
  const closed = {
    ...emptyHolding(),
    id: "closed",
    name: "中国银行",
    quantity: 0,
    marketValue: 0,
  };
  const open = {
    ...emptyHolding(),
    id: "open",
    name: "持有基金",
    quantity: 100,
    marketValue: 1000,
    costBasis: 800,
  };
  const view = mount({
    ...snapshot,
    holdings: [closed, open],
    holdingTradeSummaries: {
      closed: { settledProfit: "1190" } as HoldingTradeSummary,
    },
  });
  await screen.findByLabelText("投资持仓概览");
  fireEvent.click(screen.getByRole("button", { name: "未清仓 1" }));
  expect(screen.queryByRole("button", { name: "编辑 中国银行" })).toBeNull();
  expect(screen.getByRole("button", { name: "编辑 持有基金" })).toBeTruthy();
  expect(screen.getByLabelText("投资持仓概览").textContent).toContain(
    `+${formatMoney(1190)}`,
  );
  fireEvent.click(screen.getByRole("button", { name: "已清仓 1" }));
  expect(screen.queryByRole("button", { name: "编辑 持有基金" })).toBeNull();
  expect(screen.getByRole("button", { name: "编辑 中国银行" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "全部 2" }));
  expect(screen.getByRole("button", { name: "编辑 持有基金" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "已清仓 1" }));
  view.rerender(
    <MyFacts
      snapshot={{
        ...snapshot,
        holdings: [{ ...closed, quantity: 100, marketValue: 677 }, open],
        holdingTradeSummaries: {
          closed: { settledProfit: "1190" } as HoldingTradeSummary,
        },
      }}
      onUpdate={vi.fn()}
      navigate={vi.fn()}
      flash={vi.fn()}
    />,
  );
  expect(
    screen.getByRole("button", { name: "全部 2" }).getAttribute("aria-pressed"),
  ).toBe("true");
  expect(
    (screen.getByRole("button", { name: "已清仓 0" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
  expect(screen.getByRole("button", { name: "编辑 中国银行" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "编辑 持有基金" })).toBeTruthy();
});

it("prioritizes holdings and keeps asset history below without reserving space for exchange rates", async () => {
  const { container } = render(
    <MyFacts
      snapshot={{
        ...snapshot,
        holdings: [{ ...emptyHolding("HKD"), id: "hk", name: "港币账户" }],
      }}
      onUpdate={vi.fn()}
      navigate={vi.fn()}
      flash={vi.fn()}
    />,
  );
  await screen.findByRole("region", { name: "每日资产追踪" });
  const panels = Array.from(container.querySelectorAll(".panel"));
  expect(
    panels[0].contains(screen.getByRole("table", { name: "持仓列表" })),
  ).toBe(true);
  expect(panels.at(-1)?.getAttribute("aria-label")).toBe("每日资产追踪");
  expect(screen.queryByRole("region", { name: "汇率设置" })).toBeNull();
});

it("saves a code and quantity without requiring a manual price", async () => {
  vi.mocked(portfolioApi.saveHolding).mockResolvedValue(snapshot);
  mount();
  fireEvent.click(screen.getByRole("button", { name: "投资持仓" }));
  fireEvent.click(screen.getByRole("button", { name: "添加持仓" }));
  fireEvent.change(screen.getByRole("textbox", { name: "名称" }), {
    target: { value: "贵州茅台" },
  });
  fireEvent.change(
    screen.getByRole("textbox", { name: "证券代码（自动价格，选填）" }),
    { target: { value: "600519.SS" } },
  );
  fireEvent.change(
    screen.getByRole("spinbutton", { name: "持仓数 份 / 股 / 单位" }),
    { target: { value: "100" } },
  );
  fireEvent.click(screen.getByRole("button", { name: "保存这笔资产" }));
  await waitFor(() =>
    expect(portfolioApi.saveHolding).toHaveBeenCalledWith(
      expect.objectContaining({
        symbol: "600519.SS",
        quantity: 100,
        currentPrice: null,
      }),
    ),
  );
});

it("refreshes prices and displays the source date and partial failures", async () => {
  const holding = {
    ...emptyHolding("CNY"),
    id: "quoted",
    name: "股票",
    assetClass: "股票" as const,
    symbol: "600519.SS",
    quantity: 100,
    currentPrice: 1000,
    marketValue: 100000,
  };
  const value = {
    ...snapshot,
    holdings: [holding],
    priceRefreshErrors: ["另一个标的暂时不可用"],
    holdingValuations: [
      {
        holdingId: holding.id,
        symbol: holding.symbol,
        quantity: 100,
        unitPrice: 1000,
        marketValue: 100000,
        currency: "CNY",
        requestedOn: "2026-10-12",
        observedOn: "2026-10-09",
        stalenessDays: 3,
        providerCode: "yahoo_daily_close",
        providerName: "Yahoo Finance",
        exchange: "SHH",
        micCode: "",
        instrumentType: "EQUITY",
        priceBasis: "split_adjusted_daily_close",
        sourceUrl: "https://query1.finance.yahoo.com",
        methodologyUrl: "",
        disclaimer: "参考日收盘价",
        capturedAt: "2026-10-12T00:00:00Z",
      },
    ],
  };
  vi.mocked(portfolioApi.refreshSecurityPrices).mockResolvedValue({
    snapshot: value,
    updatedCount: 1,
    errors: value.priceRefreshErrors,
  });
  const { onUpdate } = mount(value);
  fireEvent.click(screen.getByRole("button", { name: /查看 / }));
  expect(screen.getByText("Yahoo Finance")).toBeTruthy();
  expect(screen.getByText(/收盘日 2026-10-09/)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "关闭详情" }));
  expect(screen.getByText(/部分价格更新失败，已保留原值/)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "更新自动价格" }));
  await waitFor(() => expect(onUpdate).toHaveBeenCalledWith(value));
  expect(screen.getByRole("alert").textContent).toContain(
    "另一个标的暂时不可用",
  );
});
function mount(value = snapshot) {
  const onUpdate = vi.fn();
  const flash = vi.fn();
  return {
    ...render(
      <MyFacts
        snapshot={value}
        onUpdate={onUpdate}
        navigate={vi.fn()}
        flash={flash}
      />,
    ),
    onUpdate,
    flash,
  };
}
it.each([
  ["", "600519.SS"],
  ["600519.SZ", "600519.SS"],
  ["600519.SS", ""],
])(
  "edits a trade-managed security code from %s to %s",
  async (symbol, nextSymbol) => {
    const holding = {
      ...emptyHolding("CNY"),
      id: "traded",
      name: "贵州茅台",
      assetClass: "股票" as const,
      symbol,
      quantity: 100,
      costPrice: 8,
      currentPrice: 10,
      costBasis: 800,
      marketValue: 1000,
    };
    const value: Snapshot = {
      ...snapshot,
      holdings: [holding],
      holdingTrades: [
        {
          id: "buy-1",
          holdingId: holding.id,
          kind: "buy",
          occurredOn: holding.valuationDate,
          sequence: 1,
          quantity: "100",
          price: "8",
          grossAmount: "800",
          fees: "0",
          taxes: "0",
          currency: "CNY",
          account: "券商",
          externalId: "",
          note: "",
          createdAt: "2026-01-01T00:00:00Z",
          updatedAt: "2026-01-01T00:00:00Z",
        },
      ],
    };
    const updated = {
      ...value,
      holdings: [{ ...holding, symbol: nextSymbol }],
    };
    vi.mocked(portfolioApi.updateHolding).mockResolvedValue(updated);
    const { onUpdate } = mount(value);
    fireEvent.click(screen.getByRole("button", { name: "编辑 贵州茅台" }));
    const code = screen.getByRole("textbox", {
      name: "证券代码（自动价格，选填）",
    }) as HTMLInputElement;
    expect(code.readOnly).toBe(false);
    for (const name of ["成本价（选填） CNY", "持仓数 份 / 股 / 单位"]) {
      expect(
        (screen.getByRole("spinbutton", { name }) as HTMLInputElement).readOnly,
      ).toBe(true);
    }
    fireEvent.change(code, { target: { value: nextSymbol } });
    fireEvent.click(screen.getByRole("button", { name: "保存这笔资产" }));
    const { id, ...input } = holding;
    await waitFor(() =>
      expect(portfolioApi.updateHolding).toHaveBeenCalledWith(
        id,
        expect.objectContaining({
          ...input,
          symbol: nextSymbol,
        }),
      ),
    );
    expect(onUpdate).toHaveBeenCalledWith(updated);
  },
);

it("updates the manual OPPO price without rounding or recomputing execution costs", async () => {
  const holding = {
    ...emptyHolding("CNY"),
    id: "oppo",
    name: "OPPO 股票",
    assetClass: "股票" as const,
    quantity: 17,
    costPrice: 221.91 / 17,
    costBasis: 221.91,
    currentPrice: 20,
    marketValue: 340,
  };
  const trades: HoldingTrade[] = [
    { quantity: "10", price: "10", grossAmount: "100", fees: "2" },
    { quantity: "7", price: "17.13", grossAmount: "119.91", fees: "0" },
  ].map((values, index) => ({
    ...values,
    id: `buy-${index}`,
    holdingId: holding.id,
    kind: "buy",
    occurredOn: holding.valuationDate,
    sequence: index + 1,
    taxes: "0",
    currency: "CNY",
    account: "内部股票账户",
    externalId: "",
    note: "",
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
  }));
  const updated = {
    ...snapshot,
    holdings: [
      { ...holding, currentPrice: 30.12345678, marketValue: 512.09876526 },
    ],
    holdingTrades: trades,
  };
  vi.mocked(portfolioApi.updateHolding).mockResolvedValue(updated);
  const { onUpdate } = mount({
    ...snapshot,
    holdings: [holding],
    holdingTrades: trades,
  });
  fireEvent.click(screen.getByRole("button", { name: "编辑 OPPO 股票" }));
  const price = screen.getByRole("spinbutton", { name: "当前价格 CNY" });
  expect((price as HTMLInputElement).readOnly).toBe(false);
  fireEvent.focus(price);
  fireEvent.change(price, { target: { value: "30.12345678" } });
  fireEvent.blur(price);
  expect((price as HTMLInputElement).value).toBe("30.12");
  fireEvent.click(screen.getByRole("button", { name: "保存这笔资产" }));
  const { id, ...input } = holding;
  await waitFor(() =>
    expect(portfolioApi.updateHolding).toHaveBeenCalledWith(
      id,
      expect.objectContaining({
        ...input,
        currentPrice: 30.12345678,
        marketValue: holding.quantity * 30.12345678,
      }),
    ),
  );
  expect(onUpdate).toHaveBeenCalledWith(updated);
});

it("only asks for monthly facts and preserves existing investment preferences", async () => {
  vi.mocked(portfolioApi.saveProfile).mockResolvedValue(snapshot);
  mount();
  fireEvent.click(screen.getByRole("button", { name: "收入与支出" }));
  expect(
    screen
      .getAllByRole("spinbutton")
      .filter((input) => !input.closest("details")),
  ).toHaveLength(2);
  expect(
    screen.getByText("有负债或预留备用金？（选填）").closest("details")?.open,
  ).toBe(false);
  fireEvent.change(screen.getByRole("spinbutton", { name: "月收入 CNY" }), {
    target: { value: "8000" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存收支" }));
  await waitFor(() =>
    expect(portfolioApi.saveProfile).toHaveBeenCalledWith(
      expect.objectContaining({
        ...snapshot.profile,
        monthlyIncome: 8000,
      }),
    ),
  );
});
it("stores deposits once as cash holdings, without adding them again to the profile", async () => {
  vi.mocked(portfolioApi.saveHolding).mockResolvedValue(snapshot);
  mount();
  fireEvent.click(screen.getByRole("button", { name: "存款与现金" }));
  fireEvent.click(screen.getByRole("button", { name: "编辑 CNY 存款" }));
  expect(screen.queryByRole("spinbutton", { name: /持仓成本/ })).toBeNull();
  fireEvent.change(screen.getByRole("spinbutton", { name: "当前金额 CNY" }), {
    target: { value: "50000" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存这笔资产" }));
  await waitFor(() =>
    expect(portfolioApi.saveHolding).toHaveBeenCalledWith(
      expect.objectContaining({
        name: "CNY 存款",
        assetClass: "现金",
        marketValue: 50000,
      }),
    ),
  );
  expect(portfolioApi.saveProfile).not.toHaveBeenCalled();
});
it("edits all investment preferences at the unified facts entry and validates before submission", async () => {
  vi.mocked(portfolioApi.saveProfile).mockResolvedValue(snapshot);
  mount();
  fireEvent.click(screen.getByRole("button", { name: "收入与支出" }));
  fireEvent.click(screen.getByText("投资偏好与记账币种"));
  const horizon = screen.getByRole("spinbutton", { name: "投资期限 年" });
  fireEvent.change(horizon, { target: { value: "81" } });
  fireEvent.submit(horizon.closest("form")!);
  expect(portfolioApi.saveProfile).not.toHaveBeenCalled();
  fireEvent.change(horizon, { target: { value: "12" } });
  fireEvent.change(
    screen.getByRole("spinbutton", { name: "可承受最大回撤 %" }),
    { target: { value: "20" } },
  );
  fireEvent.change(screen.getByRole("combobox", { name: "风险倾向" }), {
    target: { value: "均衡" },
  });
  fireEvent.change(screen.getByRole("combobox", { name: "基准币种" }), {
    target: { value: "USD" },
  });
  fireEvent.submit(horizon.closest("form")!);
  await waitFor(() =>
    expect(portfolioApi.saveProfile).toHaveBeenCalledWith(
      expect.objectContaining({
        horizonYears: 12,
        maxDrawdownPct: 20,
        riskLevel: "均衡",
        baseCurrency: "USD",
      }),
    ),
  );
});
it("preserves the user's entered balance on failure and keeps existing cost and target intact", async () => {
  const holding = {
    ...emptyHolding(),
    id: "existing",
    name: "已有基金",
    marketValue: 1000,
    costBasis: 750,
    targetPct: 12,
  };
  vi.mocked(portfolioApi.updateHolding).mockRejectedValue(
    new Error("保存失败"),
  );
  mount({ ...snapshot, holdings: [holding] });
  fireEvent.click(screen.getByRole("button", { name: "投资持仓" }));
  fireEvent.click(screen.getByRole("button", { name: "编辑 已有基金" }));
  fireEvent.change(screen.getByRole("spinbutton", { name: "当前市值 CNY" }), {
    target: { value: "1200" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存这笔资产" }));
  await screen.findByRole("alert");
  expect(screen.getByDisplayValue("1200")).toBeTruthy();
  expect(portfolioApi.updateHolding).toHaveBeenCalledWith(
    "existing",
    expect.objectContaining({
      marketValue: 1200,
      costBasis: 750,
      targetPct: 12,
    }),
  );
});
it("shows exactly three currency balances and accepts zero without an FX rate", async () => {
  vi.mocked(portfolioApi.saveHolding).mockResolvedValue(snapshot);
  mount();
  fireEvent.click(screen.getByRole("button", { name: "存款与现金" }));
  for (const currency of ["CNY", "HKD", "USD"]) {
    expect(
      screen.getByRole("button", { name: `编辑 ${currency} 存款` }),
    ).toBeTruthy();
  }
  expect(screen.queryByRole("button", { name: "添加存款" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "编辑 USD 存款" }));
  expect(screen.queryByRole("combobox", { name: "币种" })).toBeNull();
  expect(screen.queryByRole("textbox", { name: "名称" })).toBeNull();
  const input = screen.getByRole("spinbutton", { name: "当前金额 USD" });
  fireEvent.change(input, { target: { value: "" } });
  const save = screen.getByRole("button", { name: "保存这笔资产" });
  expect((save as HTMLButtonElement).disabled).toBe(true);
  fireEvent.change(input, { target: { value: "0" } });
  expect((save as HTMLButtonElement).disabled).toBe(false);
  fireEvent.click(save);
  await waitFor(() =>
    expect(portfolioApi.saveHolding).toHaveBeenCalledWith(
      expect.objectContaining({
        marketValue: 0,
        currency: "USD",
        fxRateToBase: null,
      }),
    ),
  );
});

it("keeps asset categories fixed while an asset form is open, then unlocks on cancel", () => {
  mount();
  fireEvent.click(screen.getByRole("button", { name: "存款与现金" }));
  fireEvent.click(screen.getByRole("button", { name: "编辑 CNY 存款" }));
  expect(
    (screen.getByRole("button", { name: "投资持仓" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "关闭详情" }));
  expect(
    (screen.getByRole("button", { name: "投资持仓" }) as HTMLButtonElement)
      .disabled,
  ).toBe(false);
});

it("blocks negative monthly facts and re-enables saving after correction", async () => {
  vi.mocked(portfolioApi.saveProfile).mockResolvedValue(snapshot);
  mount();
  fireEvent.click(screen.getByRole("button", { name: "收入与支出" }));
  const input = screen.getByRole("spinbutton", { name: "月收入 CNY" });
  fireEvent.change(input, { target: { value: "-1" } });
  expect(
    (screen.getByRole("button", { name: "保存收支" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
  expect(portfolioApi.saveProfile).not.toHaveBeenCalled();
  fireEvent.change(input, { target: { value: "8000" } });
  fireEvent.click(screen.getByRole("button", { name: "保存收支" }));
  await waitFor(() => expect(portfolioApi.saveProfile).toHaveBeenCalledOnce());
});

it("blocks invalid asset amounts even when a form is submitted with Enter", () => {
  mount();
  fireEvent.click(screen.getByRole("button", { name: "存款与现金" }));
  fireEvent.click(screen.getByRole("button", { name: "编辑 CNY 存款" }));
  fireEvent.change(screen.getByRole("spinbutton", { name: "当前金额 CNY" }), {
    target: { value: "1000000000000001" },
  });
  const save = screen.getByRole("button", { name: "保存这笔资产" });
  expect((save as HTMLButtonElement).disabled).toBe(true);
  fireEvent.submit(save.closest("form")!);
  expect(portfolioApi.saveHolding).not.toHaveBeenCalled();
});

const investment = {
  ...emptyHolding(),
  id: "investment",
  name: "指数基金",
  marketValue: 1000,
};

it("records investment cost in the holding's currency and shows the saved profit", async () => {
  const saved = {
    ...investment,
    currency: "USD",
    costBasis: 800,
    targetPct: 12,
  };
  const updated = { ...snapshot, holdings: [saved] };
  vi.mocked(portfolioApi.updateHolding).mockResolvedValue(updated);
  const { onUpdate, flash, rerender } = mount({
    ...snapshot,
    holdings: [{ ...saved, costBasis: 0 }],
  });
  expect(screen.getByText("成本待补")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "编辑 指数基金" }));
  fireEvent.change(
    screen.getByRole("spinbutton", { name: "持仓总成本（选填） USD" }),
    { target: { value: "800" } },
  );
  fireEvent.click(screen.getByRole("button", { name: "保存这笔资产" }));
  const { id, ...input } = saved;
  await waitFor(() =>
    expect(portfolioApi.updateHolding).toHaveBeenCalledWith(
      id,
      expect.objectContaining(input),
    ),
  );
  expect(onUpdate).toHaveBeenCalledWith(updated);
  rerender(
    <MyFacts
      snapshot={updated}
      onUpdate={onUpdate}
      navigate={vi.fn()}
      flash={flash}
    />,
  );
  expect(screen.getByRole("button", { name: "编辑 指数基金" })).toBeTruthy();
  expect(screen.getByText("+25%")).toBeTruthy();
  expect(screen.queryByText("成本待补")).toBeNull();
});

it("includes cost when adding an investment and keeps it optional", async () => {
  vi.mocked(portfolioApi.saveHolding).mockResolvedValue(snapshot);
  mount();
  fireEvent.click(screen.getByRole("button", { name: "投资持仓" }));
  fireEvent.click(screen.getByRole("button", { name: "添加持仓" }));
  fireEvent.change(screen.getByRole("textbox", { name: "名称" }), {
    target: { value: "新基金" },
  });
  fireEvent.change(
    screen.getByRole("spinbutton", { name: "持仓数 份 / 股 / 单位" }),
    { target: { value: "100" } },
  );
  fireEvent.change(screen.getByRole("spinbutton", { name: "当前价格 CNY" }), {
    target: { value: "10" },
  });
  const save = screen.getByRole("button", { name: "保存这笔资产" });
  expect((save as HTMLButtonElement).disabled).toBe(false);
  fireEvent.change(
    screen.getByRole("spinbutton", { name: "持仓总成本（选填） CNY" }),
    { target: { value: "1200" } },
  );
  fireEvent.click(save);
  await waitFor(() =>
    expect(portfolioApi.saveHolding).toHaveBeenCalledWith(
      expect.objectContaining({
        name: "新基金",
        marketValue: 1000,
        costBasis: 1200,
      }),
    ),
  );
});

it("blocks negative or excessive costs, then allows clearing an unrecorded cost", async () => {
  vi.mocked(portfolioApi.updateHolding).mockResolvedValue(snapshot);
  mount({ ...snapshot, holdings: [{ ...investment, costBasis: 800 }] });
  fireEvent.click(screen.getByRole("button", { name: "编辑 指数基金" }));
  const cost = screen.getByRole("spinbutton", {
    name: "持仓总成本（选填） CNY",
  });
  const save = screen.getByRole("button", { name: "保存这笔资产" });
  expect((cost as HTMLInputElement).value).toBe("800");
  for (const value of ["-1", "1000000000000001"]) {
    fireEvent.change(cost, { target: { value } });
    expect((save as HTMLButtonElement).disabled).toBe(true);
    fireEvent.submit(save.closest("form")!);
    expect(portfolioApi.updateHolding).not.toHaveBeenCalled();
  }
  fireEvent.change(cost, { target: { value: "" } });
  expect((save as HTMLButtonElement).disabled).toBe(false);
  fireEvent.click(save);
  await waitFor(() =>
    expect(portfolioApi.updateHolding).toHaveBeenCalledWith(
      investment.id,
      expect.objectContaining({ costBasis: 0, marketValue: 1000 }),
    ),
  );
});

it("preserves an edited cost after a save failure", async () => {
  vi.mocked(portfolioApi.updateHolding).mockRejectedValue(
    new Error("保存失败"),
  );
  mount({ ...snapshot, holdings: [investment] });
  fireEvent.click(screen.getByRole("button", { name: "编辑 指数基金" }));
  const cost = screen.getByRole("spinbutton", {
    name: "持仓总成本（选填） CNY",
  });
  fireEvent.change(cost, { target: { value: "800" } });
  fireEvent.click(screen.getByRole("button", { name: "保存这笔资产" }));
  await screen.findByRole("alert");
  expect((cost as HTMLInputElement).value).toBe("800");
  expect(portfolioApi.updateHolding).toHaveBeenCalledWith(
    investment.id,
    expect.objectContaining({ costBasis: 800 }),
  );
});

it("leaves holdings unchanged when deletion is cancelled", () => {
  const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
  const { onUpdate } = mount({ ...snapshot, holdings: [investment] });
  fireEvent.click(screen.getByRole("button", { name: "删除 指数基金" }));
  expect(confirm).toHaveBeenCalledWith(expect.stringContaining("指数基金"));
  expect(portfolioApi.deleteHolding).not.toHaveBeenCalled();
  expect(onUpdate).not.toHaveBeenCalled();
  expect(screen.getByRole("button", { name: "编辑 指数基金" })).toBeTruthy();
});

it("refreshes the portfolio after deletion and closes the deleted asset's edit form", async () => {
  vi.spyOn(window, "confirm").mockReturnValue(true);
  vi.mocked(portfolioApi.deleteHolding).mockResolvedValue(snapshot);
  const { onUpdate, flash, rerender } = mount({
    ...snapshot,
    holdings: [investment],
  });
  fireEvent.click(screen.getByRole("button", { name: "编辑 指数基金" }));
  fireEvent.click(screen.getByRole("button", { name: "关闭详情" }));
  fireEvent.click(screen.getByLabelText("指数基金更多操作"));
  fireEvent.click(screen.getByRole("button", { name: "删除 指数基金" }));
  await waitFor(() => expect(onUpdate).toHaveBeenCalledWith(snapshot));
  expect(portfolioApi.deleteHolding).toHaveBeenCalledExactlyOnceWith(
    "investment",
  );
  expect(flash).toHaveBeenCalledWith("资产已删除");
  expect(screen.queryByRole("region", { name: "记录资产" })).toBeNull();
  rerender(
    <MyFacts
      snapshot={snapshot}
      onUpdate={onUpdate}
      navigate={vi.fn()}
      flash={flash}
    />,
  );
  expect(screen.queryByRole("button", { name: "删除 指数基金" })).toBeNull();
  expect(screen.getByText("暂无资产，点击上方添加。")).toBeTruthy();
});

it("shows deletion failures and allows retrying without losing the holding", async () => {
  vi.spyOn(window, "confirm").mockReturnValue(true);
  vi.mocked(portfolioApi.deleteHolding).mockRejectedValueOnce(
    new Error("网络中断"),
  );
  const { onUpdate } = mount({ ...snapshot, holdings: [investment] });
  fireEvent.click(screen.getByRole("button", { name: "删除 指数基金" }));
  expect((await screen.findByRole("alert")).textContent).toContain("删除失败");
  expect(onUpdate).not.toHaveBeenCalled();
  vi.mocked(portfolioApi.deleteHolding).mockResolvedValue(snapshot);
  fireEvent.click(screen.getByRole("button", { name: "删除 指数基金" }));
  await waitFor(() => expect(onUpdate).toHaveBeenCalledWith(snapshot));
  expect(portfolioApi.deleteHolding).toHaveBeenCalledTimes(2);
});

it("updates a currency balance with the form revision and preserves input on a conflict", async () => {
  const cash = {
    ...investment,
    id: "cash",
    name: "CNY 存款",
    assetClass: "现金" as const,
    valuationDate: "",
  };
  vi.mocked(portfolioApi.updateHoldingAmount).mockRejectedValue(
    new Error("该资产已更新"),
  );
  const { onUpdate, flash, rerender } = mount({
    ...snapshot,
    holdings: [cash],
    holdingRevisions: { cash: "old" },
  });
  fireEvent.click(screen.getByRole("button", { name: "存款与现金" }));
  expect(screen.queryByRole("button", { name: "删除 CNY 存款" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "编辑 CNY 存款" }));
  const input = screen.getByRole("spinbutton", { name: "当前金额 CNY" });
  fireEvent.change(input, { target: { value: "2000" } });
  rerender(
    <MyFacts
      snapshot={{
        ...snapshot,
        holdings: [{ ...cash, marketValue: 900 }],
        holdingRevisions: { cash: "new" },
      }}
      onUpdate={onUpdate}
      flash={flash}
      navigate={vi.fn()}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "保存这笔资产" }));
  await screen.findByRole("alert");
  expect(portfolioApi.updateHoldingAmount).toHaveBeenCalledWith(
    "cash",
    "2000",
    "old",
    expect.any(String),
  );
  expect(screen.getByDisplayValue("2000")).toBeTruthy();
  expect(portfolioApi.updateHolding).not.toHaveBeenCalled();
});

it("saves manually entered unit prices and fractional quantities with calculated totals", async () => {
  vi.mocked(portfolioApi.saveHolding).mockResolvedValue(snapshot);
  mount();
  fireEvent.click(screen.getByRole("button", { name: "投资持仓" }));
  fireEvent.click(screen.getByRole("button", { name: "添加持仓" }));
  fireEvent.change(screen.getByRole("textbox", { name: "名称" }), {
    target: { value: "新基金" },
  });
  fireEvent.change(
    screen.getByRole("spinbutton", { name: "成本价（选填） CNY" }),
    { target: { value: "8" } },
  );
  const save = screen.getByRole("button", {
    name: "保存这笔资产",
  }) as HTMLButtonElement;
  expect(save.disabled).toBe(true);
  fireEvent.change(
    screen.getByRole("spinbutton", { name: "持仓数 份 / 股 / 单位" }),
    { target: { value: "100.5" } },
  );
  expect(save.disabled).toBe(true);
  fireEvent.change(screen.getByRole("spinbutton", { name: "当前价格 CNY" }), {
    target: { value: "10" },
  });
  const value = screen.getByRole("spinbutton", {
    name: "当前市值 CNY",
  }) as HTMLInputElement;
  expect(value.value).toBe("1005");
  expect(value.readOnly).toBe(true);
  expect(save.disabled).toBe(false);
  fireEvent.click(save);
  await waitFor(() =>
    expect(portfolioApi.saveHolding).toHaveBeenCalledWith(
      expect.objectContaining({
        costPrice: 8,
        quantity: 100.5,
        currentPrice: 10,
        costBasis: 804,
        marketValue: 1005,
      }),
    ),
  );
});

it("keeps detailed inputs on failure and validates zero values, missing prices and total limits", async () => {
  const detailed = {
    ...investment,
    costPrice: 8,
    quantity: 100,
    currentPrice: 10,
    costBasis: 800,
  };
  vi.mocked(portfolioApi.updateHolding).mockRejectedValue(
    new Error("保存失败"),
  );
  mount({ ...snapshot, holdings: [detailed] });
  fireEvent.click(screen.getByRole("button", { name: "编辑 指数基金" }));
  const price = screen.getByRole("spinbutton", { name: "当前价格 CNY" });
  const quantity = screen.getByRole("spinbutton", {
    name: "持仓数 份 / 股 / 单位",
  });
  const save = screen.getByRole("button", {
    name: "保存这笔资产",
  }) as HTMLButtonElement;
  for (const value of ["-1", "", "1000000000000000"]) {
    fireEvent.change(price, { target: { value } });
    expect(save.disabled).toBe(true);
    fireEvent.submit(save.closest("form")!);
    expect(portfolioApi.updateHolding).not.toHaveBeenCalled();
  }
  fireEvent.change(price, { target: { value: "0" } });
  fireEvent.change(quantity, { target: { value: "0" } });
  expect(save.disabled).toBe(false);
  fireEvent.click(save);
  await screen.findByRole("alert");
  expect((price as HTMLInputElement).value).toBe("0");
  expect((quantity as HTMLInputElement).value).toBe("0");
  expect(portfolioApi.updateHolding).toHaveBeenCalledWith(
    investment.id,
    expect.objectContaining({
      currentPrice: 0,
      quantity: 0,
      marketValue: 0,
      costBasis: 0,
    }),
  );
});

it("shows and saves the same total when returning from detailed prices to legacy amounts", async () => {
  vi.mocked(portfolioApi.updateHolding).mockResolvedValue(snapshot);
  mount({
    ...snapshot,
    holdings: [
      {
        ...investment,
        quantity: 100,
        costPrice: 8,
        currentPrice: 10,
        costBasis: 800,
      },
    ],
  });
  fireEvent.click(screen.getByRole("button", { name: "编辑 指数基金" }));
  fireEvent.change(screen.getByRole("spinbutton", { name: "当前价格 CNY" }), {
    target: { value: "12" },
  });
  for (const name of [
    "成本价（选填） CNY",
    "持仓数 份 / 股 / 单位",
    "当前价格 CNY",
  ]) {
    fireEvent.change(screen.getByRole("spinbutton", { name }), {
      target: { value: "" },
    });
  }
  expect(
    (
      screen.getByRole("spinbutton", {
        name: "当前市值 CNY",
      }) as HTMLInputElement
    ).value,
  ).toBe("1200");
  fireEvent.click(screen.getByRole("button", { name: "保存这笔资产" }));
  await waitFor(() =>
    expect(portfolioApi.updateHolding).toHaveBeenCalledWith(
      investment.id,
      expect.objectContaining({
        marketValue: 1200,
        costBasis: 800,
        costPrice: null,
        quantity: null,
        currentPrice: null,
      }),
    ),
  );
});
