// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { AssetHistory } from "./AssetHistory";
import * as api from "../../api";
import fixture from "../../test/emptySnapshot.json";
import type { DailyHistory, DailyRecord, Snapshot } from "../../types";
vi.mock("../../api", () => ({
  getDailyAssets: vi.fn(),
}));
const day: DailyRecord = {
  day: "2026-09-13",
  baseCurrency: "CNY",
  totalAssets: 120,
  liabilities: 20,
  netAssets: 100,
  carried: true,
  assets: [],
  missingFx: [],
  previousDayChange: 0,
  lastUpdatedDay: "2026-09-12",
  sinceLastUpdateChange: 0,
};
const history: DailyHistory = {
  timezone: "Asia/Shanghai",
  today: day.day,
  records: [
    day,
    { ...day, day: "2026-09-12", carried: false, totalAssets: 100 },
  ],
  nextBefore: null,
};
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.getDailyAssets).mockResolvedValue(history);
});
afterEach(cleanup);
it("shows carry-forward labels with the trend expanded and the removed sections absent", async () => {
  render(<AssetHistory snapshot={fixture as Snapshot} />);
  await screen.findByText(/今日沿用上次余额/);
  expect(screen.getByText("资产趋势").closest("details")?.open).toBe(true);
  expect(
    screen
      .getByRole("group", { name: /每日总资产趋势/ })
      .getAttribute("aria-label"),
  ).toContain("沿用值");
  expect(screen.queryByText("资产历史与区间比较")).toBeNull();
  expect(screen.queryByText("统计口径")).toBeNull();
});
it("shows incomplete totals and loads older records with a cursor", async () => {
  vi.mocked(api.getDailyAssets).mockResolvedValue({
    ...history,
    records: [
      { ...day, totalAssets: null, netAssets: null, missingFx: ["美元账户"] },
    ],
    nextBefore: "2026-09-13",
  });
  render(<AssetHistory snapshot={fixture as Snapshot} />);
  await screen.findByText("待补汇率：美元账户");
  expect(screen.queryByRole("group", { name: /每日总资产趋势/ })).toBeNull();
  fireEvent.click(screen.getByText("全部历史"));
  await waitFor(() =>
    expect(api.getDailyAssets).toHaveBeenCalledWith({ limit: 400 }),
  );
  vi.mocked(api.getDailyAssets).mockResolvedValue({
    ...history,
    records: [{ ...day, day: "2026-09-12" }],
    nextBefore: null,
  });
  fireEvent.click(screen.getByText("加载更早记录"));
  await waitFor(() =>
    expect(api.getDailyAssets).toHaveBeenCalledWith({
      limit: 400,
      before: "2026-09-13",
    }),
  );
});
