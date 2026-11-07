// @vitest-environment jsdom
import { fireEvent, render, screen, cleanup } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { WatchGridEditor } from "./WatchGrid";
import { Drawer } from "../../components/Drawer";

afterEach(cleanup);
const setup = () => {
  const onSave = vi.fn().mockResolvedValue(false);
  render(
    <Drawer title="网格" onClose={() => {}}>
      <WatchGridEditor
        plan={undefined}
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
it("persists generated settings plus individually edited share quantities", () => {
  const save = setup();
  change("标的配置金额（元）", "20000");
  change("网格间距（百分点）", "0.25");
  change("首档买入股数", "100");
  change("首档卖出股数", "50");
  change("每档买入股数递增 %", "20");
  fireEvent.click(screen.getByRole("button", { name: "生成网格" }));
  change("4.25% 下跌买入股数", "123");
  fireEvent.click(screen.getByRole("button", { name: "保存并测算" }));
  expect(save).toHaveBeenCalledWith(
    expect.objectContaining({
      budget: "20000",
      generator: expect.objectContaining({ stepBps: 25, buyGrowthBps: 2000 }),
      levels: expect.arrayContaining([
        { yieldBps: 425, buyShares: 123, sellShares: 50 },
      ]),
    }),
  );
});
it("preserves edits until replacement is confirmed and reports invalid ranges", () => {
  setup();
  change("5% 下跌买入股数", "77");
  change("首档买入股数", "100");
  fireEvent.click(screen.getByRole("button", { name: "生成网格" }));
  expect(
    (screen.getByLabelText("5% 下跌买入股数") as HTMLInputElement).value,
  ).toBe("77");
  fireEvent.click(screen.getByRole("button", { name: "确认替换现有档位" }));
  expect(
    (screen.getByLabelText("5% 下跌买入股数") as HTMLInputElement).value,
  ).toBe("100");
  change("网格间距（百分点）", "0.3");
  fireEvent.click(screen.getByRole("button", { name: "生成网格" }));
  expect(screen.getByRole("alert").textContent).toContain("间距须整除范围");
});
it("blocks duplicate levels and empty buying instructions", () => {
  const save = setup();
  change("标的配置金额（元）", "10000");
  fireEvent.click(screen.getByRole("button", { name: "保存并测算" }));
  expect(save).not.toHaveBeenCalled();
  change("5% 下跌买入股数", "100");
  change("第 1 档股息率", "5");
  fireEvent.click(screen.getByRole("button", { name: "保存并测算" }));
  expect(save).not.toHaveBeenCalled();
  expect(screen.getByRole("alert").textContent).toContain("档位不能重复");
});
