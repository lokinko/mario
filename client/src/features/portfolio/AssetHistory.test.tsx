// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { AssetOverview } from "./AssetOverview";
import { useAssetHistory } from "./useAssetHistory";
import { AssetHistory } from "./AssetHistory";
import * as portfolioApi from "../../api/portfolio";
import fixture from "../../test/emptySnapshot.json";
import type { DailyHistory, DailyRecord, Snapshot } from "../../types";
vi.mock("../../api/portfolio", () => ({
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
function History() {
  const state = useAssetHistory(fixture as Snapshot);
  return (
    <>
      <AssetOverview
        snapshot={fixture as Snapshot}
        history={state.history}
        display={{ maskAmounts: false, setMaskAmounts: vi.fn() }}
      />
      <AssetHistory state={state} />
    </>
  );
}
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(portfolioApi.getDailyAssets).mockResolvedValue(history);
});
afterEach(cleanup);
it("shows one shared history in the overview and an expanded keyboard accessible trend", async () => {
  render(<History />);
  await screen.findByText(/今日沿用上次余额/);
  expect(screen.getByText("资产趋势").closest("details")).toBeNull();
  expect(portfolioApi.getDailyAssets).toHaveBeenCalledTimes(1);
  const chart = screen.getByRole("group", { name: /每日总资产趋势/ });
  fireEvent.keyDown(chart, { key: "ArrowLeft" });
  expect(screen.getByText("2026-09-12 · CNY")).toBeTruthy();
  expect(
    screen
      .getByRole("group", { name: /每日总资产趋势/ })
      .getAttribute("aria-label"),
  ).toContain("沿用值");
  expect(screen.queryByText("资产历史与区间比较")).toBeNull();
  expect(screen.queryByText("统计口径")).toBeNull();
});
it("shows incomplete totals and loads older records with a cursor", async () => {
  vi.mocked(portfolioApi.getDailyAssets).mockResolvedValue({
    ...history,
    records: [
      { ...day, totalAssets: null, netAssets: null, missingFx: ["美元账户"] },
    ],
    nextBefore: "2026-09-13",
  });
  render(<History />);
  await waitFor(() =>
    expect(document.querySelector(".hero-amount strong")?.textContent).toBe(
      "—",
    ),
  );
  expect(screen.queryByText("待补汇率：美元账户")).toBeNull();
  expect(screen.queryByRole("group", { name: /每日总资产趋势/ })).toBeNull();
  fireEvent.click(screen.getByText("全部历史"));
  await waitFor(() =>
    expect(portfolioApi.getDailyAssets).toHaveBeenCalledWith({ limit: 400 }),
  );
  vi.mocked(portfolioApi.getDailyAssets).mockResolvedValue({
    ...history,
    records: [{ ...day, day: "2026-09-12" }],
    nextBefore: null,
  });
  fireEvent.click(screen.getByText("加载更早记录"));
  await waitFor(() =>
    expect(portfolioApi.getDailyAssets).toHaveBeenCalledWith({
      limit: 400,
      before: "2026-09-13",
    }),
  );
});
