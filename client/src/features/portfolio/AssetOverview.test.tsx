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
  getDividendIncome: vi.fn(),
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
function Overview({ snapshot = fixture as Snapshot }: { snapshot?: Snapshot }) {
  const state = useAssetHistory(snapshot);
  return (
    <AssetOverview
      snapshot={snapshot}
      history={state}
      display={{ maskAmounts: false, setMaskAmounts: vi.fn() }}
    />
  );
}
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(portfolioApi.getDailyAssets).mockResolvedValue(history);
  vi.mocked(portfolioApi.getDividendIncome).mockResolvedValue({
    baseCurrency: "CNY",
    since: "2025-11-16",
    value: "0",
    assetsPct: null,
    missing: [],
  });
});
afterEach(cleanup);
it("reads only the latest daily record and shows server-derived trailing dividends", async () => {
  vi.mocked(portfolioApi.getDailyAssets).mockResolvedValue({
    ...history,
    records: [{ ...day, previousDayChange: -5 }],
  });
  vi.mocked(portfolioApi.getDividendIncome).mockResolvedValue({
    baseCurrency: "CNY",
    since: "2025-11-16",
    value: "1234.5",
    assetsPct: "1.03",
    missing: ["腾讯控股"],
  });
  render(<Overview />);
  await screen.findByText(
    "占总资产 1.03% · 按买卖记录与 2025-11-16 后的除权日推算 · 腾讯控股未计入",
  );
  expect(portfolioApi.getDailyAssets).toHaveBeenCalledOnce();
  expect(portfolioApi.getDailyAssets).toHaveBeenCalledWith({ limit: 1 });
  expect(screen.getByText("近 12 个月分红（税前）")).toBeTruthy();
  expect(screen.getByText(/1,234\.5/)).toBeTruthy();
  expect(screen.queryByText("较上次实际更新")).toBeNull();
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
