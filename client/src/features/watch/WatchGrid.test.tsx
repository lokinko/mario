// @vitest-environment jsdom
import { fireEvent, render, screen, cleanup } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import * as api from "../../api/watch";
import { WatchGridEditor } from "./WatchGrid";
import { Drawer } from "../../components/Drawer";

vi.mock("../../api/watch", async (original) => ({
  ...(await original<typeof api>()),
  previewWatchGrid: vi.fn(),
}));
afterEach(cleanup);
beforeEach(() => {
  vi.mocked(api.previewWatchGrid)
    .mockReset()
    .mockResolvedValue({
      topPrice: "11.00",
      levels: [
        {
          price: "11.00",
          side: "sell",
          targetAmount: "5000.00",
          shares: 400,
          amount: "4400.00",
          cumulativeAmount: null,
          budgetPct: null,
        },
        {
          price: "8.33",
          side: "buy",
          targetAmount: "120000.00",
          shares: 14400,
          amount: "119952.00",
          cumulativeAmount: "119952.00",
          budgetPct: "119.9520",
        },
      ],
      scenarios: [
        {
          deepestPrice: "8.33",
          investedAmount: "119952.00",
          budgetPct: "119.9520",
          boughtShares: 14400,
          averageCost: "8.3300",
          baseProfit: "24048.00",
          baseReturnPct: "20.0480",
          soldShares: 400,
          remainingShares: 14000,
          saleProceeds: "4400.00",
          topProfit: "38448.00",
          topReturnPct: "32.0528",
        },
      ],
    });
});
const setup = (dividend: number | null = 0.5) => {
  const onSave = vi.fn().mockResolvedValue(false);
  render(
    <Drawer title="网格" onClose={() => {}}>
      <WatchGridEditor
        plan={undefined}
        dividend={dividend}
        close={10}
        busy={false}
        onEdited={() => {}}
        onCancel={() => {}}
        onSave={onSave}
      />
    </Drawer>,
  );
  return onSave;
};
const change = (label: string, value: string) =>
  fireEvent.change(screen.getByLabelText(label), { target: { value } });

it("defaults to the close, converts yields to prices and previews returns live", async () => {
  const save = setup();
  expect(screen.getByText("股息率 5%")).toBeTruthy();
  change("标的配置金额（元）", "100000");
  change("每档相差 %", "10");
  change("向下档数", "1");
  change("向上档数", "1");
  change("每档买入 占配置 %", "120");
  change("每档卖出 占配置 %", "5");
  fireEvent.click(screen.getByRole("button", { name: "生成网格" }));
  expect(
    (screen.getByLabelText("第 2 档股息率") as HTMLInputElement).value,
  ).toBe("5.56");
  change("第 2 档股息率", "6");
  expect((screen.getByLabelText("第 2 档价格") as HTMLInputElement).value).toBe(
    "8.33",
  );
  expect((await screen.findAllByText("14,400 股")).length).toBeGreaterThan(0);
  expect(api.previewWatchGrid).toHaveBeenLastCalledWith(
    {
      budget: "100000",
      basePrice: "10.00",
      sizeUnit: "budgetPct",
      levels: [
        { price: "11.00", size: "5" },
        { price: "8.33", size: "120" },
      ],
    },
    expect.any(AbortSignal),
  );
  expect(screen.getByRole("status").textContent).toContain("占配置 120%");
  expect(
    screen.getByRole("table", { name: "跌到各买入档后的测算" }).textContent,
  ).toContain("余 14,000 股");
  fireEvent.click(screen.getByRole("button", { name: "保存" }));
  expect(save).toHaveBeenCalledWith(
    expect.objectContaining({ basePrice: "10.00", sizeUnit: "budgetPct" }),
  );
});

it("switches size units by converting draft values against the budget", () => {
  setup();
  change("标的配置金额（元）", "20000");
  fireEvent.click(screen.getByRole("button", { name: /添加/ }));
  change("第 1 档价格", "9");
  change("第 1 档规模", "25");
  change("规模单位", "amount");
  expect((screen.getByLabelText("第 1 档规模") as HTMLInputElement).value).toBe(
    "5000",
  );
});

it("blocks duplicate or base-equal prices and plans without a buy level", () => {
  const save = setup(null);
  expect(screen.getByText("暂无有效分红，只能按价格设置档位")).toBeTruthy();
  change("标的配置金额（元）", "10000");
  fireEvent.click(screen.getByRole("button", { name: /添加/ }));
  change("第 1 档价格", "11");
  change("第 1 档规模", "10");
  fireEvent.click(screen.getByRole("button", { name: "保存" }));
  expect(screen.getByRole("alert").textContent).toContain("基准价以下");
  change("第 1 档价格", "10");
  fireEvent.click(screen.getByRole("button", { name: "保存" }));
  expect(screen.getByRole("alert").textContent).toContain("不等于基准价");
  expect(save).not.toHaveBeenCalled();
  expect(
    (screen.getByLabelText("第 1 档股息率") as HTMLInputElement).disabled,
  ).toBe(true);
});
