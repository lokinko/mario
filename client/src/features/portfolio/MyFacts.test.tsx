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
import { MyFacts } from "./MyFacts";
import * as api from "../../api";
import type { Snapshot } from "../../types";
import fixture from "../../test/emptySnapshot.json";
import { emptyHolding } from "./forms";
import { formatMoney } from "../../lib/format";
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
  deleteHolding: vi.fn(),
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
  vi.restoreAllMocks();
});
const snapshot = fixture as Snapshot;
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
  expect(screen.queryByRole("spinbutton", { name: /持仓成本/ })).toBeNull();
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
  fireEvent.click(screen.getByRole("button", { name: "编辑 已有基金" }));
  fireEvent.change(screen.getByRole("spinbutton", { name: "当前市值 CNY" }), {
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

it("keeps asset categories fixed while an asset form is open, then unlocks on cancel", () => {
  mount();
  fireEvent.click(screen.getByRole("button", { name: "存款与现金" }));
  fireEvent.click(screen.getByRole("button", { name: "添加存款" }));
  expect(
    (screen.getByRole("button", { name: "投资持仓" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "取消记录资产" }));
  expect(
    (screen.getByRole("button", { name: "投资持仓" }) as HTMLButtonElement)
      .disabled,
  ).toBe(false);
});

it("blocks negative monthly facts and re-enables saving after correction", async () => {
  vi.mocked(api.saveProfile).mockResolvedValue(snapshot);
  mount();
  const input = screen.getByRole("spinbutton", { name: "月收入 CNY" });
  fireEvent.change(input, { target: { value: "-1" } });
  expect(
    (screen.getByRole("button", { name: "保存收支" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
  expect(api.saveProfile).not.toHaveBeenCalled();
  fireEvent.change(input, { target: { value: "8000" } });
  fireEvent.click(screen.getByRole("button", { name: "保存收支" }));
  await waitFor(() => expect(api.saveProfile).toHaveBeenCalledOnce());
});

it("blocks invalid asset amounts even when a form is submitted with Enter", () => {
  mount();
  fireEvent.click(screen.getByRole("button", { name: "存款与现金" }));
  fireEvent.click(screen.getByRole("button", { name: "添加存款" }));
  fireEvent.change(screen.getByRole("spinbutton", { name: "当前金额 CNY" }), {
    target: { value: "1000000000000001" },
  });
  const save = screen.getByRole("button", { name: "保存这笔资产" });
  expect((save as HTMLButtonElement).disabled).toBe(true);
  fireEvent.submit(save.closest("form")!);
  expect(api.saveHolding).not.toHaveBeenCalled();
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
  vi.mocked(api.updateHolding).mockResolvedValue(updated);
  const { onUpdate, flash, rerender } = mount({
    ...snapshot,
    holdings: [{ ...saved, costBasis: 0 }],
  });
  expect(screen.getByText("待补成本")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "编辑 指数基金成本" }));
  fireEvent.change(
    screen.getByRole("spinbutton", { name: "持仓总成本（选填） USD" }),
    { target: { value: "800" } },
  );
  fireEvent.click(screen.getByRole("button", { name: "保存这笔资产" }));
  const { id, ...input } = saved;
  await waitFor(() =>
    expect(api.updateHolding).toHaveBeenCalledWith(id, input),
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
  expect(
    screen.getByRole("button", { name: "编辑 指数基金成本" }).textContent,
  ).toBe("待补成本价");
  expect(screen.getByText(`总成本 ${formatMoney(800, "USD")}`)).toBeTruthy();
  expect(screen.getByText("+25.0%")).toBeTruthy();
  expect(screen.queryByText("待补成本")).toBeNull();
});

it("includes cost when adding an investment and keeps it optional", async () => {
  vi.mocked(api.saveHolding).mockResolvedValue(snapshot);
  mount();
  fireEvent.click(screen.getByRole("button", { name: "投资持仓" }));
  fireEvent.click(screen.getByRole("button", { name: "添加持仓" }));
  fireEvent.change(screen.getByRole("textbox", { name: "名称" }), {
    target: { value: "新基金" },
  });
  fireEvent.change(screen.getByRole("spinbutton", { name: "当前市值 CNY" }), {
    target: { value: "1000" },
  });
  const save = screen.getByRole("button", { name: "保存这笔资产" });
  expect((save as HTMLButtonElement).disabled).toBe(false);
  fireEvent.change(
    screen.getByRole("spinbutton", { name: "持仓总成本（选填） CNY" }),
    { target: { value: "1200" } },
  );
  fireEvent.click(save);
  await waitFor(() =>
    expect(api.saveHolding).toHaveBeenCalledWith(
      expect.objectContaining({
        name: "新基金",
        marketValue: 1000,
        costBasis: 1200,
      }),
    ),
  );
});

it("blocks negative or excessive costs, then allows clearing an unrecorded cost", async () => {
  vi.mocked(api.updateHolding).mockResolvedValue(snapshot);
  mount({ ...snapshot, holdings: [{ ...investment, costBasis: 800 }] });
  fireEvent.click(screen.getByRole("button", { name: "编辑 指数基金成本" }));
  const cost = screen.getByRole("spinbutton", {
    name: "持仓总成本（选填） CNY",
  });
  const save = screen.getByRole("button", { name: "保存这笔资产" });
  expect((cost as HTMLInputElement).value).toBe("800");
  for (const value of ["-1", "1000000000000001"]) {
    fireEvent.change(cost, { target: { value } });
    expect((save as HTMLButtonElement).disabled).toBe(true);
    fireEvent.submit(save.closest("form")!);
    expect(api.updateHolding).not.toHaveBeenCalled();
  }
  fireEvent.change(cost, { target: { value: "" } });
  expect((save as HTMLButtonElement).disabled).toBe(false);
  fireEvent.click(save);
  await waitFor(() =>
    expect(api.updateHolding).toHaveBeenCalledWith(
      investment.id,
      expect.objectContaining({ costBasis: 0, marketValue: 1000 }),
    ),
  );
});

it("preserves an edited cost after a save failure", async () => {
  vi.mocked(api.updateHolding).mockRejectedValue(new Error("保存失败"));
  mount({ ...snapshot, holdings: [investment] });
  fireEvent.click(screen.getByRole("button", { name: "编辑 指数基金成本" }));
  const cost = screen.getByRole("spinbutton", {
    name: "持仓总成本（选填） CNY",
  });
  fireEvent.change(cost, { target: { value: "800" } });
  fireEvent.click(screen.getByRole("button", { name: "保存这笔资产" }));
  await screen.findByRole("alert");
  expect((cost as HTMLInputElement).value).toBe("800");
  expect(api.updateHolding).toHaveBeenCalledWith(
    investment.id,
    expect.objectContaining({ costBasis: 800 }),
  );
});

it("leaves holdings unchanged when deletion is cancelled", () => {
  const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
  const { onUpdate } = mount({ ...snapshot, holdings: [investment] });
  fireEvent.click(screen.getByRole("button", { name: "删除 指数基金" }));
  expect(confirm).toHaveBeenCalledWith(expect.stringContaining("指数基金"));
  expect(api.deleteHolding).not.toHaveBeenCalled();
  expect(onUpdate).not.toHaveBeenCalled();
  expect(screen.getByRole("button", { name: "编辑 指数基金" })).toBeTruthy();
});

it("refreshes the portfolio after deletion and closes the deleted asset's edit form", async () => {
  vi.spyOn(window, "confirm").mockReturnValue(true);
  vi.mocked(api.deleteHolding).mockResolvedValue(snapshot);
  const { onUpdate, flash, rerender } = mount({
    ...snapshot,
    holdings: [investment],
  });
  fireEvent.click(screen.getByRole("button", { name: "编辑 指数基金" }));
  fireEvent.click(screen.getByRole("button", { name: "删除 指数基金" }));
  await waitFor(() => expect(onUpdate).toHaveBeenCalledWith(snapshot));
  expect(api.deleteHolding).toHaveBeenCalledExactlyOnceWith("investment");
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
  vi.mocked(api.deleteHolding).mockRejectedValueOnce(new Error("网络中断"));
  const { onUpdate } = mount({ ...snapshot, holdings: [investment] });
  fireEvent.click(screen.getByRole("button", { name: "删除 指数基金" }));
  expect((await screen.findByRole("alert")).textContent).toContain("删除失败");
  expect(onUpdate).not.toHaveBeenCalled();
  vi.mocked(api.deleteHolding).mockResolvedValue(snapshot);
  fireEvent.click(screen.getByRole("button", { name: "删除 指数基金" }));
  await waitFor(() => expect(onUpdate).toHaveBeenCalledWith(snapshot));
  expect(api.deleteHolding).toHaveBeenCalledTimes(2);
});

it("also supports deleting cash and blocks duplicate actions during deletion", async () => {
  vi.spyOn(window, "confirm").mockReturnValue(true);
  let resolve!: (value: Snapshot) => void;
  vi.mocked(api.deleteHolding).mockImplementationOnce(
    () =>
      new Promise((yes) => {
        resolve = yes;
      }),
  );
  const cash = {
    ...investment,
    id: "cash",
    name: "银行存款",
    assetClass: "现金" as const,
  };
  mount({ ...snapshot, holdings: [cash] });
  const remove = screen.getByRole("button", { name: "删除 银行存款" });
  fireEvent.click(remove);
  expect((remove as HTMLButtonElement).disabled).toBe(true);
  expect(
    (screen.getByLabelText("银行存款金额") as HTMLInputElement).readOnly,
  ).toBe(true);
  expect(
    (screen.getByRole("button", { name: "编辑 银行存款" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
  fireEvent.click(remove);
  expect(api.deleteHolding).toHaveBeenCalledExactlyOnceWith("cash");
  await act(async () => resolve(snapshot));
});

it("saves manually entered unit prices and fractional quantities with calculated totals", async () => {
  vi.mocked(api.saveHolding).mockResolvedValue(snapshot);
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
    expect(api.saveHolding).toHaveBeenCalledWith(
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
  vi.mocked(api.updateHolding).mockRejectedValue(new Error("保存失败"));
  mount({ ...snapshot, holdings: [detailed] });
  fireEvent.click(
    screen.getByRole("button", { name: "编辑 指数基金当前价格" }),
  );
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
    expect(api.updateHolding).not.toHaveBeenCalled();
  }
  fireEvent.change(price, { target: { value: "0" } });
  fireEvent.change(quantity, { target: { value: "0" } });
  expect(save.disabled).toBe(false);
  fireEvent.click(save);
  await screen.findByRole("alert");
  expect((price as HTMLInputElement).value).toBe("0");
  expect((quantity as HTMLInputElement).value).toBe("0");
  expect(api.updateHolding).toHaveBeenCalledWith(
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
  vi.mocked(api.updateHolding).mockResolvedValue(snapshot);
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
    expect(api.updateHolding).toHaveBeenCalledWith(
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
