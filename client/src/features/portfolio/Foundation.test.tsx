// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { Foundation } from "./Foundation";
import * as portfolioApi from "../../api/portfolio";
import type { Snapshot } from "../../types";
import emptySnapshot from "../../test/emptySnapshot.json";
import { emptyHolding } from "./forms";
vi.mock("../../api/portfolio", async (original) => ({
  ...(await original<typeof import("../../api/portfolio")>()),
  saveProfile: vi.fn(),
  saveHolding: vi.fn(),
  updateHolding: vi.fn(),
}));
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

function renderFoundation(snapshot = emptySnapshot as Snapshot) {
  return render(
    <Foundation snapshot={snapshot} onUpdate={vi.fn()} flash={vi.fn()} />,
  );
}

function fillHolding() {
  fireEvent.change(screen.getByLabelText("资产名称"), {
    target: { value: " 测试基金 " },
  });
  fireEvent.change(
    screen.getByRole("spinbutton", { name: "持仓数 份 / 股 / 单位" }),
    { target: { value: "100" } },
  );
  fireEvent.change(screen.getByRole("spinbutton", { name: "当前价格 CNY" }), {
    target: { value: "12.3456" },
  });
}

it("keeps unsaved holding input while switching financial sections", () => {
  renderFoundation();
  fillHolding();
  fireEvent.click(screen.getByRole("button", { name: "收支与风险" }));
  expect(screen.queryByRole("textbox", { name: "资产名称" })).toBeNull();
  expect(screen.getByRole("spinbutton", { name: "月收入 CNY" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: /^目标/ }));
  expect(screen.getByRole("textbox", { name: "目标名称" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: /^持仓/ }));
  expect((screen.getByLabelText("资产名称") as HTMLInputElement).value).toBe(
    " 测试基金 ",
  );
  expect(screen.getByDisplayValue("1234.56")).toBeTruthy();
});

it("opens goals directly when arriving from the overview goal action", () => {
  render(
    <Foundation
      snapshot={emptySnapshot as Snapshot}
      onUpdate={vi.fn()}
      flash={vi.fn()}
      initialSection="goals"
    />,
  );
  expect(screen.getByRole("textbox", { name: "目标名称" })).toBeTruthy();
  expect(screen.queryByRole("textbox", { name: "资产名称" })).toBeNull();
});

it("calculates market value immediately and clears the preview when details are missing", () => {
  renderFoundation();
  const amount = screen.getByRole("spinbutton", {
    name: "当前市值（CNY） CNY",
  }) as HTMLInputElement;
  const quantity = screen.getByRole("spinbutton", {
    name: "持仓数 份 / 股 / 单位",
  });
  const price = screen.getByRole("spinbutton", { name: "当前价格 CNY" });
  const save = screen.getByRole("button", {
    name: "加入组合",
  }) as HTMLButtonElement;
  expect(amount.readOnly).toBe(true);
  expect(amount.value).toBe("");
  fillHolding();
  expect(amount.value).toBe("1234.56");
  fireEvent.change(quantity, { target: { value: "800" } });
  fireEvent.change(price, { target: { value: "5.13" } });
  expect(amount.value).toBe("4104");
  fireEvent.change(price, { target: { value: "" } });
  expect(amount.value).toBe("");
  expect(save.disabled).toBe(true);
  fireEvent.change(price, { target: { value: "0" } });
  expect(amount.value).toBe("0");
  expect(save.disabled).toBe(false);
  fireEvent.change(quantity, { target: { value: "" } });
  expect(amount.value).toBe("");
  expect(save.disabled).toBe(true);
});

it("saves code and quantity without a placeholder price or manual market value", async () => {
  vi.mocked(portfolioApi.saveHolding).mockResolvedValue(
    emptySnapshot as Snapshot,
  );
  renderFoundation();
  fireEvent.change(screen.getByLabelText("资产名称"), {
    target: { value: "中国银行" },
  });
  fireEvent.change(
    screen.getByRole("textbox", { name: "证券代码（自动价格，选填）" }),
    {
      target: { value: "601988.SS" },
    },
  );
  fireEvent.change(
    screen.getByRole("spinbutton", { name: "成本价（选填） CNY" }),
    {
      target: { value: "4.92" },
    },
  );
  fireEvent.change(
    screen.getByRole("spinbutton", { name: "持仓数 份 / 股 / 单位" }),
    {
      target: { value: "800" },
    },
  );
  expect(
    (
      screen.getByRole("spinbutton", {
        name: "当前市值（CNY） CNY",
      }) as HTMLInputElement
    ).value,
  ).toBe("");
  expect(screen.getByText("保存后获取价格并计算市值")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "加入组合" }));
  await waitFor(() =>
    expect(portfolioApi.saveHolding).toHaveBeenCalledWith(
      expect.objectContaining({
        symbol: "601988.SS",
        quantity: 800,
        currentPrice: null,
        costPrice: 4.92,
        costBasis: 3936,
      }),
    ),
  );
});

it("allows entering cash balances and clears investment details when switching to cash", async () => {
  vi.mocked(portfolioApi.saveHolding).mockResolvedValue(
    emptySnapshot as Snapshot,
  );
  renderFoundation();
  fillHolding();
  fireEvent.change(screen.getByLabelText("资产类别"), {
    target: { value: "现金" },
  });
  const amount = screen.getByRole("spinbutton", {
    name: "当前金额（CNY） CNY",
  }) as HTMLInputElement;
  expect(amount.readOnly).toBe(false);
  fireEvent.change(amount, { target: { value: "2000" } });
  fireEvent.click(screen.getByRole("button", { name: "加入组合" }));
  await waitFor(() =>
    expect(portfolioApi.saveHolding).toHaveBeenCalledWith(
      expect.objectContaining({
        assetClass: "现金",
        marketValue: 2000,
        quantity: null,
        currentPrice: null,
      }),
    ),
  );
});

it("saves calculated market value and retains context for the next holding", async () => {
  vi.mocked(portfolioApi.saveHolding).mockResolvedValue(
    emptySnapshot as Snapshot,
  );
  const { container } = renderFoundation();
  expect(container.querySelector("section h2")?.textContent).toBe(
    "管理资产组合",
  );
  expect(container.querySelector("details")?.open).toBe(false);
  fillHolding();
  fireEvent.change(screen.getByLabelText("资产类别"), {
    target: { value: "股票" },
  });
  fireEvent.change(screen.getByLabelText(/^估值日期/), {
    target: { value: "2025-03-10" },
  });
  fireEvent.click(screen.getByRole("button", { name: "加入组合" }));
  await waitFor(() =>
    expect(portfolioApi.saveHolding).toHaveBeenCalledWith(
      expect.objectContaining({
        name: "测试基金",
        marketValue: 1234.56,
        quantity: 100,
        currentPrice: 12.3456,
        costBasis: 0,
        targetPct: 0,
        assetClass: "股票",
        valuationDate: "2025-03-10",
      }),
    ),
  );
  await waitFor(() =>
    expect((screen.getByLabelText("资产名称") as HTMLInputElement).value).toBe(
      "",
    ),
  );
  expect((screen.getByLabelText("资产类别") as HTMLSelectElement).value).toBe(
    "股票",
  );
  expect((screen.getByLabelText(/^估值日期/) as HTMLInputElement).value).toBe(
    "2025-03-10",
  );
  expect(
    (
      screen.getByRole("spinbutton", {
        name: "当前市值（CNY） CNY",
      }) as HTMLInputElement
    ).value,
  ).toBe("");
});

it("saves foreign assets without a manual rate input", async () => {
  vi.mocked(portfolioApi.saveHolding).mockResolvedValue(
    emptySnapshot as Snapshot,
  );
  renderFoundation();
  fillHolding();
  fireEvent.change(screen.getByLabelText("持仓币种"), {
    target: { value: "USD" },
  });
  expect(screen.queryByRole("spinbutton", { name: /折算汇率/ })).toBeNull();
  expect(screen.queryByRole("button", { name: /查询 ECB/ })).toBeNull();
  const add = screen.getByRole("button", {
    name: "加入组合",
  }) as HTMLButtonElement;
  expect(add.disabled).toBe(false);
  fireEvent.click(add);
  await waitFor(() =>
    expect(portfolioApi.saveHolding).toHaveBeenCalledWith(
      expect.objectContaining({ currency: "USD", fxRateToBase: null }),
    ),
  );
});

it("preserves input after a failed holding save", async () => {
  vi.mocked(portfolioApi.saveHolding).mockRejectedValue(new Error("保存失败"));
  renderFoundation();
  fillHolding();
  fireEvent.click(screen.getByRole("button", { name: "加入组合" }));
  expect((await screen.findByRole("alert")).textContent).toContain("保存失败");
  expect((screen.getByLabelText("资产名称") as HTMLInputElement).value).toBe(
    " 测试基金 ",
  );
  expect(screen.getByDisplayValue("1234.56")).toBeTruthy();
});

it("opens optional details for editing and preserves existing values", async () => {
  const holding = {
    ...emptyHolding(),
    id: "holding-1",
    name: "已有基金",
    marketValue: 1000,
    costBasis: 800,
    targetPct: 20,
    symbol: "TEST",
  };
  vi.mocked(portfolioApi.updateHolding).mockResolvedValue(
    emptySnapshot as Snapshot,
  );
  const { container } = renderFoundation({
    ...(emptySnapshot as Snapshot),
    holdings: [holding],
  });
  fireEvent.click(screen.getByRole("button", { name: "编辑资产" }));
  expect(container.querySelector("details")?.open).toBe(true);
  expect(screen.getByDisplayValue("TEST")).toBeTruthy();
  fireEvent.change(
    screen.getByRole("spinbutton", { name: "当前市值（CNY） CNY" }),
    { target: { value: "1100" } },
  );
  fireEvent.click(screen.getByRole("button", { name: "保存修改" }));
  await waitFor(() =>
    expect(portfolioApi.updateHolding).toHaveBeenCalledWith(
      "holding-1",
      expect.objectContaining({
        marketValue: 1100,
        costBasis: 800,
        targetPct: 20,
        symbol: "TEST",
      }),
    ),
  );
});
it("keeps financial inputs visible and reports failed saves", async () => {
  vi.mocked(portfolioApi.saveProfile).mockRejectedValue(new Error("保存失败"));
  const onUpdate = vi.fn();
  render(
    <Foundation
      snapshot={emptySnapshot as Snapshot}
      onUpdate={onUpdate}
      flash={vi.fn()}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "收支与风险" }));
  const input = screen.getByRole("spinbutton", { name: "月收入 CNY" });
  fireEvent.change(input, { target: { value: "12345" } });
  fireEvent.click(screen.getByRole("button", { name: "保存并检查" }));
  expect((await screen.findByRole("alert")).textContent).toContain("保存失败");
  expect(screen.getByDisplayValue("12345")).toBeTruthy();
  expect(onUpdate).not.toHaveBeenCalled();
});
it("preserves an explicit zero while editing an existing holding", async () => {
  const holding = {
    ...emptyHolding(),
    id: "zero",
    name: "清零账户",
    marketValue: 0,
  };
  vi.mocked(portfolioApi.updateHolding).mockResolvedValue({
    ...emptySnapshot,
    holdings: [holding],
  } as Snapshot);
  renderFoundation({ ...emptySnapshot, holdings: [holding] } as Snapshot);
  fireEvent.click(screen.getByRole("button", { name: "编辑资产" }));
  const field = screen.getByRole("spinbutton", {
    name: "当前市值（CNY） CNY",
  }) as HTMLInputElement;
  expect(field.value).toBe("0");
  fireEvent.change(screen.getByLabelText("资产名称"), {
    target: { value: "清零后账户" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存修改" }));
  await waitFor(() =>
    expect(portfolioApi.updateHolding).toHaveBeenCalledWith(
      "zero",
      expect.objectContaining({ marketValue: 0, name: "清零后账户" }),
    ),
  );
});
