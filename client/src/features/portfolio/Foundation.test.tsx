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
import * as api from "../../api";
import type { Snapshot } from "../../types";
import emptySnapshot from "../../test/emptySnapshot.json";
import { emptyHolding } from "./forms";
vi.mock("../../api", async (original) => ({
  ...(await original<typeof import("../../api")>()),
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
    screen.getByRole("spinbutton", { name: "当前市值（CNY） CNY" }),
    { target: { value: "1234.56" } },
  );
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

it("saves with only name and market value and retains context for the next holding", async () => {
  vi.mocked(api.saveHolding).mockResolvedValue(emptySnapshot as Snapshot);
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
    expect(api.saveHolding).toHaveBeenCalledWith(
      expect.objectContaining({
        name: "测试基金",
        marketValue: 1234.56,
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

it("requires a foreign currency rate and clears it when the date changes", async () => {
  vi.mocked(api.saveHolding).mockResolvedValue(emptySnapshot as Snapshot);
  renderFoundation();
  fillHolding();
  fireEvent.change(screen.getByLabelText("持仓币种"), {
    target: { value: "USD" },
  });
  const add = screen.getByRole("button", {
    name: "加入组合",
  }) as HTMLButtonElement;
  expect(add.disabled).toBe(true);
  fireEvent.change(
    screen.getByRole("spinbutton", { name: "折算汇率（1 USD = ? CNY） CNY" }),
    { target: { value: "7.2" } },
  );
  expect(add.disabled).toBe(false);
  fireEvent.click(add);
  await waitFor(() =>
    expect((screen.getByLabelText("资产名称") as HTMLInputElement).value).toBe(
      "",
    ),
  );
  expect((screen.getByLabelText("持仓币种") as HTMLSelectElement).value).toBe(
    "USD",
  );
  expect(screen.getByDisplayValue("7.2")).toBeTruthy();
  fireEvent.change(screen.getByLabelText(/^估值日期/), {
    target: { value: "2025-03-11" },
  });
  expect(
    (
      screen.getByRole("spinbutton", {
        name: "折算汇率（1 USD = ? CNY） CNY",
      }) as HTMLInputElement
    ).value,
  ).toBe("");
});

it("preserves input after a failed holding save", async () => {
  vi.mocked(api.saveHolding).mockRejectedValue(new Error("保存失败"));
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
  vi.mocked(api.updateHolding).mockResolvedValue(emptySnapshot as Snapshot);
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
    expect(api.updateHolding).toHaveBeenCalledWith(
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
  vi.mocked(api.saveProfile).mockRejectedValue(new Error("保存失败"));
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
