// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { DividendCalendar } from "./DividendCalendar";
import * as api from "../../api/watch";

vi.mock("../../api/watch", () => ({
  getDividendCalendar: vi.fn(),
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
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
it("shows a 30-day calendar, separates record/ex dates, and leaves payment dates unknown", async () => {
  render(<DividendCalendar />);
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
  expect(screen.queryByRole("button", { name: /更新|重新抓取/ })).toBeNull();
  expect(screen.queryByRole("tab")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "2026-11-12，0项安排" }));
  expect(screen.getByText("当天暂无已获取的确定安排。")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "查看全部日期" }));
  expect(screen.getByText("1.25 元")).toBeTruthy();
});
it("moves the 30-day window and returns to today", async () => {
  render(<DividendCalendar />);
  expect(
    await screen.findByRole("heading", { name: "2026年11月7日 – 12月6日" }),
  ).toBeTruthy();
  expect(
    (screen.getByRole("button", { name: "今天" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "后30天" }));
  await waitFor(() =>
    expect(api.getDividendCalendar).toHaveBeenLastCalledWith(
      "all",
      "2026-12-07",
    ),
  );
  fireEvent.click(screen.getByRole("button", { name: "今天" }));
  await waitFor(() =>
    expect(api.getDividendCalendar).toHaveBeenLastCalledWith("all", ""),
  );
});
it("does not show the previous scope's entries while loading the new scope", async () => {
  vi.mocked(api.getDividendCalendar).mockResolvedValueOnce(board);
  let finish!: (next: api.DividendCalendarBoard) => void;
  vi.mocked(api.getDividendCalendar).mockImplementationOnce(
    () => new Promise((resolve) => (finish = resolve)),
  );
  render(<DividendCalendar />);
  await screen.findByText("1.25 元");
  fireEvent.click(screen.getByRole("radio", { name: "仅我的自选" }));
  expect(screen.queryByText("1.25 元")).toBeNull();
  expect(screen.getByRole("status").textContent).toContain("正在读取分红日历");
  finish({ ...board, scope: "favorites", entries: [] });
  await screen.findByRole("button", { name: "2026-11-10，0项安排" });
  expect(screen.queryByText("1.25 元")).toBeNull();
});

it("marks invalid plan values for review without offering a manual refetch", async () => {
  vi.mocked(api.getDividendCalendar).mockResolvedValue({
    ...board,
    entries: [
      { ...entry, plan: { ...entry.plan, issues: ["invalid_amount"] } },
    ],
  });
  render(<DividendCalendar />);
  expect(await screen.findByText("金额待核对")).toBeTruthy();
  expect(screen.queryByRole("button", { name: /重新抓取/ })).toBeNull();
});
it("keeps presentation choices out of the unsaved-draft guard", async () => {
  render(<DividendCalendar />);
  await screen.findByText("1.25 元");
  expect(
    screen
      .getByRole("heading", { name: "分红日历" })
      .closest("[data-presentation]"),
  ).toBeTruthy();
  fireEvent.click(screen.getByRole("radio", { name: "仅我的自选" }));
  await waitFor(() =>
    expect(api.getDividendCalendar).toHaveBeenLastCalledWith("favorites", ""),
  );
});

it("updates window heading on paged ranges and links cards to watch and AI", async () => {
  const onAsk = vi.fn();
  const onInspectWatch = vi.fn();
  vi.mocked(api.getDividendCalendar)
    .mockResolvedValueOnce(structuredClone(board))
    .mockResolvedValueOnce({
      ...board,
      start: "2026-10-08",
      end: "2026-11-06",
      entries: [],
    });
  render(<DividendCalendar onAsk={onAsk} onInspectWatch={onInspectWatch} />);
  await screen.findByText("1.25 元");
  fireEvent.click(screen.getByRole("button", { name: "查看观察" }));
  expect(onInspectWatch).toHaveBeenCalledWith("600000.SS", true);
  fireEvent.click(screen.getByRole("button", { name: "问 AI 分红" }));
  expect(onAsk).toHaveBeenCalledWith(expect.stringContaining("600000.SS"));
  fireEvent.click(screen.getByRole("button", { name: "前30天" }));
  expect(
    await screen.findByText(
      "当前已获取资料中，该30天内暂无已公布日期的分红安排。",
    ),
  ).toBeTruthy();
  expect(screen.queryByText("未来30天已公布的安排")).toBeNull();
});
