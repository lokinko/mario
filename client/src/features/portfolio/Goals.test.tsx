// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { Goals } from "./Goals";
import * as api from "../../api/portfolio";
import type { Snapshot } from "../../types";
import fixture from "../../test/emptySnapshot.json";
vi.mock("../../api/portfolio", () => ({
  saveGoal: vi.fn(),
  updateGoal: vi.fn(),
  deleteGoal: vi.fn(),
}));
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
  vi.restoreAllMocks();
});
const snapshot = fixture as Snapshot;
const goal = {
  id: "goal-1",
  name: "养老储备",
  targetAmount: 100000,
  currentAmount: 25000,
  monthlyContribution: 1000,
  targetDate: "2035-01-01",
  priority: "重要" as const,
};
function mount(goals = [goal]) {
  const onUpdate = vi.fn();
  render(
    <Goals
      snapshot={{ ...snapshot, goals }}
      onUpdate={onUpdate}
      flash={vi.fn()}
    />,
  );
  return onUpdate;
}
it("shows funding progress and opens a focused editor without duplicate asset forms", () => {
  mount();
  expect((screen.getByRole("progressbar") as HTMLProgressElement).value).toBe(
    25,
  );
  expect(screen.queryByRole("textbox")).toBeNull();
  expect(screen.queryByText(/模拟达成/)).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "编辑目标 养老储备" }));
  expect(document.activeElement).toBe(screen.getByLabelText("目标名称"));
  fireEvent.click(screen.getByRole("button", { name: "取消" }));
  expect(screen.queryByRole("textbox")).toBeNull();
});
it("blocks invalid submissions and creates a trimmed goal with valid funding", async () => {
  const onUpdate = mount([]);
  vi.mocked(api.saveGoal).mockResolvedValue(snapshot);
  fireEvent.click(screen.getByRole("button", { name: "添加目标" }));
  fireEvent.change(screen.getByLabelText("目标名称"), {
    target: { value: " 买房 " },
  });
  fireEvent.submit(screen.getByLabelText("目标名称").closest("form")!);
  expect(api.saveGoal).not.toHaveBeenCalled();
  fireEvent.change(screen.getByLabelText("目标日期"), {
    target: { value: "2030-01-01" },
  });
  fireEvent.change(screen.getByRole("spinbutton", { name: "目标金额 CNY" }), {
    target: { value: "500000" },
  });
  fireEvent.change(
    screen.getByRole("spinbutton", { name: "计划每月投入 CNY" }),
    { target: { value: "-1" } },
  );
  fireEvent.submit(screen.getByLabelText("目标名称").closest("form")!);
  expect(api.saveGoal).not.toHaveBeenCalled();
  fireEvent.change(
    screen.getByRole("spinbutton", { name: "计划每月投入 CNY" }),
    { target: { value: "2000" } },
  );
  fireEvent.click(screen.getByRole("button", { name: "保存目标" }));
  await waitFor(() =>
    expect(api.saveGoal).toHaveBeenCalledWith(
      expect.objectContaining({
        name: "买房",
        targetAmount: 500000,
        monthlyContribution: 2000,
      }),
    ),
  );
  await waitFor(() => expect(onUpdate).toHaveBeenCalledWith(snapshot));
});
it("preserves an edited goal on failure and retries the original record", async () => {
  const onUpdate = mount();
  vi.mocked(api.updateGoal)
    .mockRejectedValueOnce(new Error("保存失败"))
    .mockResolvedValue(snapshot);
  fireEvent.click(screen.getByRole("button", { name: "编辑目标 养老储备" }));
  fireEvent.change(screen.getByRole("spinbutton", { name: "已经投入 CNY" }), {
    target: { value: "40000" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存目标" }));
  await screen.findByRole("alert");
  expect(screen.getByDisplayValue("40000")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "保存目标" }));
  await waitFor(() => expect(onUpdate).toHaveBeenCalledWith(snapshot));
  expect(api.updateGoal).toHaveBeenLastCalledWith(
    goal.id,
    expect.objectContaining({ currentAmount: 40000 }),
  );
  expect(api.saveGoal).not.toHaveBeenCalled();
});
it("deletes only after confirmation and refreshes the displayed goals", async () => {
  const onUpdate = mount();
  const confirm = vi
    .spyOn(window, "confirm")
    .mockReturnValueOnce(false)
    .mockReturnValueOnce(true);
  vi.mocked(api.deleteGoal).mockResolvedValue(snapshot);
  fireEvent.click(screen.getByRole("button", { name: "删除目标 养老储备" }));
  expect(api.deleteGoal).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "删除目标 养老储备" }));
  await waitFor(() => expect(onUpdate).toHaveBeenCalledWith(snapshot));
  expect(confirm).toHaveBeenCalledTimes(2);
  expect(api.deleteGoal).toHaveBeenCalledWith(goal.id);
});
