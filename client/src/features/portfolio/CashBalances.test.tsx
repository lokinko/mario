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
import type { Snapshot } from "../../types";
import fixture from "../../test/emptySnapshot.json";
import { emptyHolding } from "./forms";

vi.mock("../../api/portfolio", () => ({
  getFinancialHistory: vi.fn(),
  ensureDailyAssets: vi.fn(),
  getDailyAssets: vi.fn(),
  saveHolding: vi.fn(),
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

it("keeps a CNY recording entry when only HKD and USD deposits exist", async () => {
  const holdings = ["HKD", "USD"].map((currency) => ({
    ...emptyHolding(currency),
    id: currency,
    name: `${currency} 存款`,
    assetClass: "现金" as const,
    marketValue: 100,
  }));
  vi.mocked(portfolioApi.saveHolding).mockResolvedValue(snapshot);
  mount({ ...snapshot, holdings });
  const table = screen.getByRole("table", { name: "现金列表" });
  expect(within(table).getAllByRole("row")).toHaveLength(4);
  const row = within(table).getByText("CNY").closest('[role="row"]')!;
  expect(within(row as HTMLElement).getByText("未记录")).toBeTruthy();
  fireEvent.click(
    within(row as HTMLElement).getByRole("button", { name: "添加" }),
  );
  fireEvent.change(screen.getByRole("spinbutton", { name: "当前金额 CNY" }), {
    target: { value: "50000" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存这笔资产" }));
  await waitFor(() =>
    expect(portfolioApi.saveHolding).toHaveBeenCalledWith(
      expect.objectContaining({
        currency: "CNY",
        assetClass: "现金",
        cashAmount: "50000",
      }),
    ),
  );
});

it("identifies an existing deposit by currency even when its original name omits CNY", () => {
  const cash = {
    ...emptyHolding("CNY"),
    id: "cash-cny",
    name: "工资卡",
    assetClass: "现金" as const,
    marketValue: 1234,
  };
  mount({ ...snapshot, holdings: [cash] });
  const table = screen.getByRole("table", { name: "现金列表" });
  const entry = within(table).getByRole("button", { name: "查看 CNY 存款" });
  expect(entry.textContent).toBe("CNY");
  fireEvent.click(entry);
  expect(
    (
      screen.getByRole("spinbutton", {
        name: "当前金额 CNY",
      }) as HTMLInputElement
    ).value,
  ).toBe("1234");
  expect(portfolioApi.saveHolding).not.toHaveBeenCalled();
});
