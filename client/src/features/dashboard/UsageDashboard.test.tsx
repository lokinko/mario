// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  getUsageDashboard,
  type UsageDashboard as DashboardData,
} from "../../api/activity";
import { HttpError } from "../../lib/transport";
import { UsageDashboard } from "./UsageDashboard";

vi.mock("../../api/activity", () => ({ getUsageDashboard: vi.fn() }));
beforeEach(() => {
  vi.spyOn(window, "getComputedStyle").mockImplementation((element) =>
    originalStyle(element),
  );
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: vi.fn(() => ({
      matches: false,
      addListener() {},
      removeListener() {},
      addEventListener() {},
      removeEventListener() {},
    })),
  });
});
const originalStyle = window.getComputedStyle.bind(window);
afterEach(() => {
  cleanup();
  window.history.replaceState(null, "", "/");
  vi.restoreAllMocks();
  vi.resetAllMocks();
});
const fixture: DashboardData = {
  marketPrices: {
    generatedAt: 1794528000,
    years: 10,
    targetThrough: "2026-11-13",
    universe: null,
    universeAttemptedAt: null,
    universeError: "",
    summary: {
      date: "2026-11-13",
      expected: 0,
      successful: 0,
      failed: 0,
      pending: 0,
      lagging: 0,
      updatedAt: 1794528000,
    },
    rows: [],
    daily: [],
  },
  generatedAt: 1794528000,
  days: 30,
  totalUsers: 2,
  dailyActive: 1,
  weeklyActive: 1,
  monthlyActive: 1,
  daily: [{ date: "2026-11-13", activeUsers: 1, activeMinutes: 3 }],
  registrations: [
    { date: "2026-11-13", newUsers: 1, totalUsers: 2 },
    { date: "2026-11-12", newUsers: 0, totalUsers: 1 },
  ],
  registeredBeforeTracking: 1,
  users: [
    {
      userId: "1",
      email: "one@example.test",
      lastSeen: 1794528000,
      activeDays: 1,
      activeMinutes: 3,
    },
    {
      userId: "2",
      email: "two@example.test",
      lastSeen: null,
      activeDays: 0,
      activeMinutes: 0,
    },
  ],
  pages: [{ page: "watch", activeUsers: 1, activeMinutes: 3 }],
};

it("keeps account details off the overview and opens them from the header", async () => {
  vi.mocked(getUsageDashboard).mockImplementation(async (days) => ({
    ...fixture,
    days,
  }));
  render(<UsageDashboard />);
  expect(
    await screen.findByRole("img", {
      name: "2026-11-12 至 2026-11-13，新注册 1 个，累计账号 2 个，其中上线前已有 1 个",
    }),
  ).toBeTruthy();
  expect(screen.getByText("买点观察")).toBeTruthy();
  expect(screen.getByText("3 分钟 · 1 人")).toBeTruthy();
  expect(screen.queryByText("one@example.test")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: /查看明细/ }));
  expect(window.location.hash).toBe("#prices");
  expect(screen.getByLabelText("价格状态筛选")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: /返回总览/ }));
  expect(window.location.hash).toBe("");
  fireEvent.click(screen.getByRole("button", { name: /用户活跃明细/ }));
  expect(window.location.hash).toBe("#users");
  expect(screen.getByText("one@example.test")).toBeTruthy();
  fireEvent.change(screen.getByLabelText("搜索用户邮箱"), {
    target: { value: "two@" },
  });
  expect(screen.queryByText("one@example.test")).toBeNull();
  expect(screen.getByText("two@example.test")).toBeTruthy();
  fireEvent.click(screen.getByText("近 7 天"));
  await waitFor(() => expect(getUsageDashboard).toHaveBeenLastCalledWith(7));
});

it("shows permission denial without account tables", async () => {
  vi.mocked(getUsageDashboard).mockRejectedValue(
    new HttpError("仅超级账户可查看全站活跃度", 403),
  );
  render(<UsageDashboard />);
  expect(await screen.findByText("无权查看全站统计")).toBeTruthy();
  expect(screen.queryByText("用户活跃明细")).toBeNull();
});

it("ignores a stale response when a newer range has been selected", async () => {
  let resolve!: (value: DashboardData) => void;
  vi.mocked(getUsageDashboard)
    .mockImplementationOnce(
      () =>
        new Promise((done) => {
          resolve = done;
        }),
    )
    .mockResolvedValue({ ...fixture, days: 7 });
  window.history.replaceState(null, "", "/#users");
  render(<UsageDashboard />);
  fireEvent.click(screen.getByText("近 7 天"));
  expect(await screen.findByText("one@example.test")).toBeTruthy();
  resolve({
    ...fixture,
    users: [{ ...fixture.users[0], email: "stale@example.test" }],
  });
  await waitFor(() =>
    expect(screen.queryByText("stale@example.test")).toBeNull(),
  );
});
