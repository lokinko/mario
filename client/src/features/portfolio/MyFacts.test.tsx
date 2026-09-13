// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { MyFacts } from "./MyFacts";
import * as api from "../../api";
import type { Snapshot } from "../../types";
import fixture from "../../test/emptySnapshot.json";
import { emptyHolding } from "./forms";
vi.mock("../../api", () => ({
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
  compareDailyAssets: vi.fn(),
  updateHoldingAmount: vi.fn(),
  saveProfile: vi.fn(),
  saveHolding: vi.fn(),
  updateHolding: vi.fn(),
}));
beforeEach(() => {
  vi.mocked(api.getDailyAssets).mockResolvedValue({
    timezone: "UTC",
    today: "2026-09-13",
    records: [],
    nextBefore: null,
  });
});
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});
const snapshot = fixture as Snapshot;
function mount(value = snapshot) {
  return render(
    <MyFacts
      snapshot={value}
      onUpdate={vi.fn()}
      navigate={vi.fn()}
      flash={vi.fn()}
    />,
  );
}
it("only asks for monthly facts and preserves existing investment preferences", async () => {
  vi.mocked(api.saveProfile).mockResolvedValue(snapshot);
  mount();
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
    expect(api.saveProfile).toHaveBeenCalledWith({
      ...snapshot.profile,
      monthlyIncome: 8000,
    }),
  );
});
it("stores deposits once as cash holdings, without adding them again to the profile", async () => {
  vi.mocked(api.saveHolding).mockResolvedValue(snapshot);
  mount();
  fireEvent.click(screen.getByRole("button", { name: "存款与现金" }));
  fireEvent.click(screen.getByRole("button", { name: "添加存款" }));
  fireEvent.change(screen.getByRole("spinbutton", { name: "当前金额 CNY" }), {
    target: { value: "50000" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存这笔资产" }));
  await waitFor(() =>
    expect(api.saveHolding).toHaveBeenCalledWith(
      expect.objectContaining({
        name: "银行存款",
        assetClass: "现金",
        marketValue: 50000,
      }),
    ),
  );
  expect(api.saveProfile).not.toHaveBeenCalled();
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
  vi.mocked(api.updateHolding).mockRejectedValue(new Error("保存失败"));
  mount({ ...snapshot, holdings: [holding] });
  fireEvent.click(screen.getByRole("button", { name: "投资持仓" }));
  fireEvent.click(screen.getByRole("button", { name: /已有基金/ }));
  fireEvent.change(screen.getByRole("spinbutton", { name: "当前金额 CNY" }), {
    target: { value: "1200" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存这笔资产" }));
  await screen.findByRole("alert");
  expect(screen.getByDisplayValue("1200")).toBeTruthy();
  expect(api.updateHolding).toHaveBeenCalledWith(
    "existing",
    expect.objectContaining({
      marketValue: 1200,
      costBasis: 750,
      targetPct: 12,
    }),
  );
});
it("requires an explicit amount and accepts a zero balance without an FX rate", async () => {
  vi.mocked(api.saveHolding).mockResolvedValue(snapshot);
  mount();
  fireEvent.click(screen.getByRole("button", { name: "存款与现金" }));
  fireEvent.click(screen.getByRole("button", { name: "添加存款" }));
  const save = screen.getByRole("button", { name: "保存这笔资产" });
  expect((save as HTMLButtonElement).disabled).toBe(true);
  fireEvent.change(screen.getByRole("combobox", { name: "币种" }), {
    target: { value: "USD" },
  });
  fireEvent.change(screen.getByRole("spinbutton", { name: "当前金额 USD" }), {
    target: { value: "0" },
  });
  expect((save as HTMLButtonElement).disabled).toBe(false);
  fireEvent.click(save);
  await waitFor(() =>
    expect(api.saveHolding).toHaveBeenCalledWith(
      expect.objectContaining({
        marketValue: 0,
        currency: "USD",
        fxRateToBase: null,
      }),
    ),
  );
});
