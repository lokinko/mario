// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { Dashboard } from "./Dashboard";
import type { Snapshot } from "../../types";
import emptySnapshot from "../../test/emptySnapshot.json";

vi.mock("../../api", () => ({
  getPortfolioCheckins: vi.fn().mockResolvedValue([]),
  getReviewReminders: vi.fn().mockResolvedValue(null),
  savePortfolioCheckin: vi.fn(),
}));
afterEach(cleanup);

it("prioritizes high risks and keeps additional findings reachable", async () => {
  const snapshot = {
    ...(emptySnapshot as Snapshot),
    findings: [
      {
        level: "low" as const,
        title: "一般提示",
        detail: "提示依据",
        action: "继续观察",
      },
      {
        level: "medium" as const,
        title: "中等风险",
        detail: "风险依据",
        action: "检查",
      },
      {
        level: "high" as const,
        title: "优先风险",
        detail: "风险依据",
        action: "处理",
      },
      {
        level: "low" as const,
        title: "更多提示",
        detail: "提示依据",
        action: "观察",
      },
    ],
  };
  const { container } = render(
    <Dashboard snapshot={snapshot} navigate={vi.fn()} flash={vi.fn()} />,
  );
  await waitFor(() =>
    expect(container.querySelector(".finding strong")?.textContent).toBe(
      "优先风险",
    ),
  );
  expect(screen.queryByText("更多提示")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "查看全部 4 项" }));
  expect(screen.getByText("更多提示")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "收起" }));
  expect(screen.queryByText("更多提示")).toBeNull();
});

it("directs the first goal action to the goals section", async () => {
  const navigate = vi.fn();
  render(
    <Dashboard
      snapshot={emptySnapshot as Snapshot}
      navigate={navigate}
      flash={vi.fn()}
    />,
  );
  fireEvent.click(
    await screen.findByRole("button", { name: "设定第一个目标" }),
  );
  expect(navigate).toHaveBeenCalledWith("foundation", "goals");
});

it("does not present unconverted allocations or risk estimates", async () => {
  const snapshot = {
    ...(emptySnapshot as Snapshot),
    valuationStatus: { ...emptySnapshot.valuationStatus, comparable: false },
  };
  const { container } = render(
    <Dashboard snapshot={snapshot} navigate={vi.fn()} flash={vi.fn()} />,
  );
  await waitFor(() =>
    expect(
      screen.getByText("等待汇率", { selector: ".risk-status" }),
    ).toBeTruthy(),
  );
  expect(container.querySelector(".donut")).toBeNull();
  expect(
    [...container.querySelectorAll(".risk-comparison-row strong")].map(
      (node) => node.textContent,
    ),
  ).toEqual(["—", "—"]);
});
