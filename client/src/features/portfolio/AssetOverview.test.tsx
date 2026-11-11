// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { AssetOverview } from "./AssetOverview";
import { useAssetHistory } from "./useAssetHistory";
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
function Overview() {
  const state = useAssetHistory(fixture as Snapshot);
  return (
    <AssetOverview
      snapshot={fixture as Snapshot}
      history={state}
      display={{ maskAmounts: false, setMaskAmounts: vi.fn() }}
    />
  );
}
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(portfolioApi.getDailyAssets).mockResolvedValue(history);
});
afterEach(cleanup);
it("reads only the latest daily record and shows the change since the last real update", async () => {
  vi.mocked(portfolioApi.getDailyAssets).mockResolvedValue({
    ...history,
    records: [{ ...day, previousDayChange: -5, sinceLastUpdateChange: 20 }],
  });
  render(<Overview />);
  await screen.findByText("2026-09-12 起");
  expect(portfolioApi.getDailyAssets).toHaveBeenCalledOnce();
  expect(portfolioApi.getDailyAssets).toHaveBeenCalledWith({ limit: 1 });
  expect(screen.getByText("较上次实际更新")).toBeTruthy();
  expect(screen.queryByText("较昨日")).toBeNull();
  expect(screen.queryByText(/沿用原值|日走势/)).toBeNull();
});
it("shows an unavailable total", async () => {
  vi.mocked(portfolioApi.getDailyAssets).mockResolvedValue({
    ...history,
    records: [
      { ...day, totalAssets: null, netAssets: null, missingFx: ["美元账户"] },
    ],
  });
  render(<Overview />);
  await waitFor(() =>
    expect(document.querySelector(".hero-amount strong")?.textContent).toBe(
      "—",
    ),
  );
});
