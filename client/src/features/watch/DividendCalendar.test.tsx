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
import { DividendCalendar } from "./DividendCalendar";
import * as api from "../../api/watch";

vi.mock("../../api/watch", () => ({
  getDividendCalendar: vi.fn(),
  refreshDividendCalendar: vi.fn(),
}));
const entry: api.DividendCalendarEntry = {
  symbol: "600000.SS",
  name: "测试公司",
  personal: true,
  fetchedAt: "2026-11-07T08:00:00Z",
  plan: {
    id: "plan-one",
    symbol: "600000.SS",
    reportDate: "2026-06-30",
    announcedOn: "2026-11-06",
    stage: "scheduled",
    sourceProgress: "实施分配",
    profile: "10派1.25元（含税）",
    cashPerTen: "1.25",
    bonusPerTen: null,
    transferPerTen: null,
    recordDate: "2026-11-10",
    exDate: "2026-11-11",
    paymentDate: null,
    issues: [],
    sources: ["https://example.test/dividend-plan"],
  },
};
const board: api.DividendCalendarBoard = {
  start: "2026-11-07",
  end: "2026-12-06",
  asOf: "2026-11-07",
  scope: "all",
  entries: [entry],
  pending: [
    {
      ...entry,
      plan: {
        ...entry.plan,
        id: "pending",
        stage: "preannounced",
        cashPerTen: null,
        profile: "分红比例待确定",
        recordDate: null,
        exDate: null,
      },
    },
  ],
  coverage: [
    {
      symbol: entry.symbol,
      name: entry.name,
      personal: true,
      attemptedAt: entry.fetchedAt,
      fetchedAt: entry.fetchedAt,
      refreshFailed: false,
    },
  ],
};
beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(api.getDividendCalendar).mockResolvedValue(structuredClone(board));
  vi.mocked(api.refreshDividendCalendar).mockResolvedValue(
    structuredClone(board),
  );
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
it("shows a 30-day calendar, separates record/ex dates, and leaves payment dates unknown", async () => {
  render(<DividendCalendar onClose={vi.fn()} />);
  expect(
    await screen.findByRole("button", { name: "2026-11-10，1项安排" }),
  ).toBeTruthy();
  expect(
    screen.getByRole("button", { name: "2026-11-11，1项安排" }),
  ).toBeTruthy();
  expect(
    screen.getByRole("button", { name: "2026-12-06，0项安排" }),
  ).toBeTruthy();
  expect(screen.getByText("1.25 元")).toBeTruthy();
  expect(screen.getByText("来源未提供")).toBeTruthy();
  expect(screen.getByRole("link", { name: "分红计划来源 ↗" })).toHaveProperty(
    "href",
    "https://example.test/dividend-plan",
  );
  expect(api.refreshDividendCalendar).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "2026-11-12，0项安排" }));
  expect(screen.getByText("当天暂无已获取的确定安排。")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "查看全部日期" }));
  expect(screen.getByText("1.25 元")).toBeTruthy();
});
it("keeps undated proposals outside the grid and never turns unknown cash into zero", async () => {
  render(<DividendCalendar onClose={vi.fn()} />);
  fireEvent.click(await screen.findByRole("tab", { name: "待定计划 1" }));
  expect(screen.getByText("未披露")).toBeTruthy();
  expect(
    screen.getByText("预披露 · 报告期 2026-06-30 · 公告日期 2026-11-06"),
  ).toBeTruthy();
  expect(screen.queryByLabelText("未来30天分红日期")).toBeNull();
  expect(screen.queryByText("0 元")).toBeNull();
});
it("shows a failed company's original date and retries that company within the chosen scope", async () => {
  vi.mocked(api.getDividendCalendar).mockResolvedValue({
    ...board,
    scope: "favorites",
    coverage: [{ ...board.coverage[0], refreshFailed: true }],
  });
  render(<DividendCalendar onClose={vi.fn()} />);
  await screen.findByRole("tab", { name: "已定日期 1" });
  fireEvent.change(screen.getByLabelText("分红日历范围"), {
    target: { value: "favorites" },
  });
  await waitFor(() =>
    expect(api.getDividendCalendar).toHaveBeenLastCalledWith("favorites", ""),
  );
  fireEvent.click(screen.getByText("数据覆盖与更新记录（1 家待处理）"));
  expect(screen.getByText(/本次来源读取失败，保留原资料和日期/)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "重新抓取 测试公司" }));
  await waitFor(() =>
    expect(api.refreshDividendCalendar).toHaveBeenCalledWith("favorites", "", [
      "600000.SS",
    ]),
  );
});
it("does not ask to discard presentation choices and can close without sending a model request", async () => {
  const close = vi.fn();
  vi.spyOn(window, "confirm").mockReturnValue(false);
  render(<DividendCalendar onClose={close} />);
  await screen.findByRole("tab", { name: "已定日期 1" });
  fireEvent.change(screen.getByLabelText("分红日历范围"), {
    target: { value: "public" },
  });
  fireEvent.click(
    within(screen.getByRole("dialog")).getByRole("button", {
      name: "关闭详情",
    }),
  );
  expect(close).toHaveBeenCalled();
  expect(window.confirm).not.toHaveBeenCalled();
});
