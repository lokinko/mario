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
import type {
  DailyComparison,
  DailyHistory,
  DailyRecord,
  Snapshot,
} from "../../types";
vi.mock("../../api", () => ({
  getDailyAssets: vi.fn(),
  compareDailyAssets: vi.fn(),
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
  window.sessionStorage.clear();
  vi.mocked(api.getDailyAssets).mockResolvedValue(history);
});
afterEach(cleanup);
it("shows carry-forward labels and passes the selected dates to the advisor without calling AI", async () => {
  const navigate = vi.fn();
  render(<AssetHistory snapshot={fixture as Snapshot} navigate={navigate} />);
  await screen.findByText(/今日沿用上次余额/);
  expect(screen.getByRole("img").getAttribute("aria-label")).toContain(
    "沿用值",
  );
  const comparison: DailyComparison = {
    from: history.records[1],
    to: day,
    amountChange: 20,
    assets: [],
    allocationChanges: [],
  };
  vi.mocked(api.compareDailyAssets).mockResolvedValue(comparison);
  fireEvent.click(screen.getByText("比较变化"));
  fireEvent.click(await screen.findByText("分析这段变化"));
  expect(navigate).toHaveBeenCalledWith("advisor");
  expect(
    JSON.parse(window.sessionStorage.getItem("mario.dailyAssetRange")!),
  ).toEqual({ from: "2026-09-12", to: "2026-09-13" });
  fireEvent.change(screen.getByLabelText("开始日期"), {
    target: { value: "2026-09-11" },
  });
  expect(screen.queryByText("分析这段变化")).toBeNull();
});
it("shows incomplete totals and loads older records with a cursor", async () => {
  vi.mocked(api.getDailyAssets).mockResolvedValue({
    ...history,
    records: [
      { ...day, totalAssets: null, netAssets: null, missingFx: ["美元账户"] },
    ],
    nextBefore: "2026-09-13",
  });
  render(<AssetHistory snapshot={fixture as Snapshot} navigate={vi.fn()} />);
  await screen.findByText("待补汇率：美元账户");
  expect(screen.queryByRole("img")).toBeNull();
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
