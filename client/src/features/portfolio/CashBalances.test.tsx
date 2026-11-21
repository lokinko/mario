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
import type { Snapshot } from "../../types/user";
import fixture from "../../test/emptySnapshot.json";
import { emptyHolding } from "./forms";

vi.mock("../../api/portfolio", () => ({
  getDividendIncome: vi.fn().mockResolvedValue({
    baseCurrency: "CNY",
    since: "2025-09-13",
    value: "0",
    assetsPct: null,
    missing: [],
  }),
  getAllocation: vi.fn().mockResolvedValue({
    baseCurrency: "CNY",
    byClass: [],
    byIndustry: [],
    missing: 0,
  }),
  getFinancialHistory: vi.fn(),
  ensureDailyAssets: vi.fn(),
  getDailyAssets: vi.fn(),
  saveHolding: vi.fn(),
  updateHolding: vi.fn(),
  updateHoldingAmount: vi.fn(),
  deleteHolding: vi.fn(),
}));
const snapshot = fixture as Snapshot;
beforeEach(() => {
  vi.mocked(portfolioApi.getFinancialHistory).mockResolvedValue({
    records: [],
    quarantine: null,
  });
  const history = {
    timezone: "Asia/Shanghai",
    today: "2026-09-13",
    records: [],
    nextBefore: null,
  };
  vi.mocked(portfolioApi.ensureDailyAssets).mockResolvedValue(history);
  vi.mocked(portfolioApi.getDailyAssets).mockResolvedValue(history);
  vi.mocked(portfolioApi.getDividendIncome).mockResolvedValue({
    baseCurrency: "CNY",
    since: "2025-09-13",
    value: "0",
    assetsPct: null,
    missing: [],
  });
});
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});
function mount(value: Snapshot) {
  return render(
    <MyFacts
      snapshot={value}
      section="savings"
      onUpdate={vi.fn()}
      flash={vi.fn()}
    />,
  );
}

it("adds a named deposit account from the empty state with currency and balance", async () => {
  vi.mocked(portfolioApi.saveHolding).mockResolvedValue(snapshot);
  mount(snapshot);
  expect(screen.getByText("还没有存款账户")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "添加第一个存款账户" }));
  expect(screen.getByRole("dialog", { name: "添加存款账户" })).toBeTruthy();
  const save = screen.getByRole("button", {
    name: "保存这笔资产",
  }) as HTMLButtonElement;
  expect(save.disabled).toBe(true);
  fireEvent.change(screen.getByRole("textbox", { name: "账户名称" }), {
    target: { value: "微信零钱" },
  });
  fireEvent.change(screen.getByRole("combobox", { name: "币种" }), {
    target: { value: "USD" },
  });
  fireEvent.change(screen.getByRole("spinbutton", { name: "当前金额 USD" }), {
    target: { value: "50000" },
  });
  expect(save.disabled).toBe(false);
  fireEvent.click(save);
  await waitFor(() =>
    expect(portfolioApi.saveHolding).toHaveBeenCalledWith(
      expect.objectContaining({
        name: "微信零钱",
        currency: "USD",
        assetClass: "现金",
        cashAmount: "50000",
        fxRateToBase: null,
      }),
    ),
  );
});

it("groups multiple deposit accounts by currency with subtotals and per-account confirmed dates", () => {
  const holdings = [
    {
      ...emptyHolding("CNY"),
      id: "wechat",
      name: "微信零钱",
      assetClass: "现金" as const,
      marketValue: 12000.5,
      valuationDate: "2026-09-10",
    },
    {
      ...emptyHolding("CNY"),
      id: "fund-housing",
      name: "公积金账户",
      assetClass: "现金" as const,
      marketValue: 80000.25,
      valuationDate: "2026-09-12",
    },
    {
      ...emptyHolding("USD"),
      id: "hsbc-usd",
      name: "汇丰美元",
      assetClass: "现金" as const,
      marketValue: 1500,
      valuationDate: "2026-09-11",
    },
  ];
  mount({
    ...snapshot,
    holdings,
    cashBalances: [
      {
        currency: "CNY",
        holdingId: "wechat",
        amount: "12000.50",
        baselineId: "b1",
        confirmedOn: "2026-09-10",
        confirmedAt: "2026-09-10T00:00:00Z",
        needsReconciliation: false,
      },
      {
        currency: "CNY",
        holdingId: "fund-housing",
        amount: "80000.25",
        baselineId: "b2",
        confirmedOn: "2026-09-12",
        confirmedAt: "2026-09-12T00:00:00Z",
        needsReconciliation: false,
      },
      {
        currency: "USD",
        holdingId: "hsbc-usd",
        amount: "1500",
        baselineId: "b3",
        confirmedOn: "2026-09-11",
        confirmedAt: "2026-09-11T00:00:00Z",
        needsReconciliation: false,
      },
    ],
  });
  const table = screen.getByRole("table", { name: "现金列表" });
  expect(
    within(table)
      .getAllByRole("rowgroup")
      .map((g) => g.getAttribute("aria-label")),
  ).toEqual(["CNY 存款账户", "USD 存款账户"]);
  const cnyGroup = within(table).getByRole("rowgroup", {
    name: "CNY 存款账户",
  });
  expect(within(cnyGroup).getByText("2 个账户")).toBeTruthy();
  expect(within(cnyGroup).getByText("92,000.75")).toBeTruthy();
  const cnyNames = Array.from(
    cnyGroup.querySelectorAll(".asset-name strong"),
  ).map((el) => el.textContent);
  expect(cnyNames).toEqual(["公积金账户", "微信零钱"]);
  expect(within(cnyGroup).getByText("2026-09-10")).toBeTruthy();
  expect(within(cnyGroup).getByText("2026-09-12")).toBeTruthy();
});

it("renames and changes currency of an existing deposit account via updateHolding and allows deletion", async () => {
  const cash = {
    ...emptyHolding("CNY"),
    id: "cash-1",
    name: "CNY 存款",
    assetClass: "现金" as const,
    marketValue: 1234,
    valuationDate: "2026-09-10",
  };
  vi.mocked(portfolioApi.updateHolding).mockResolvedValue(snapshot);
  vi.mocked(portfolioApi.deleteHolding).mockResolvedValue(snapshot);
  const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
  mount({
    ...snapshot,
    holdings: [cash],
    holdingRevisions: { "cash-1": "rev-1" },
  });
  const table = screen.getByRole("table", { name: "现金列表" });
  fireEvent.click(within(table).getByRole("button", { name: "查看 CNY 存款" }));
  fireEvent.change(screen.getByRole("textbox", { name: "账户名称" }), {
    target: { value: "招商银行工资卡" },
  });
  fireEvent.change(screen.getByRole("combobox", { name: "币种" }), {
    target: { value: "HKD" },
  });
  fireEvent.change(screen.getByRole("spinbutton", { name: "当前金额 HKD" }), {
    target: { value: "2500" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存这笔资产" }));
  await waitFor(() =>
    expect(portfolioApi.updateHolding).toHaveBeenCalledWith(
      "cash-1",
      expect.objectContaining({
        name: "招商银行工资卡",
        currency: "HKD",
        cashAmount: "2500",
        fxRateToBase: null,
      }),
    ),
  );
  expect(portfolioApi.updateHoldingAmount).not.toHaveBeenCalled();

  fireEvent.click(screen.getByRole("button", { name: "CNY 存款更多操作" }));
  fireEvent.click(screen.getByRole("menuitem", { name: "删除 CNY 存款" }));
  expect(confirm).toHaveBeenCalledWith(
    expect.stringContaining("确认删除存款账户“CNY 存款”"),
  );
  await waitFor(() =>
    expect(portfolioApi.deleteHolding).toHaveBeenCalledWith("cash-1", "rev-1"),
  );
});
