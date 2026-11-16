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
});
afterEach(cleanup);
it("reads only the latest daily record and shows trailing dividends instead of update changes", async () => {
  vi.mocked(portfolioApi.getDailyAssets).mockResolvedValue({
    ...history,
    records: [{ ...day, previousDayChange: -5 }],
  });
  const snapshot = structuredClone(fixture) as Snapshot;
  snapshot.accounting = {
    ...snapshot.accounting!,
    dividends: {
      since: "2025-11-16",
      received: { value: "1234.5", missing: 1 },
      heldYieldPct: "3.21",
    },
  };
  render(<Overview snapshot={snapshot} />);
  await screen.findByText(
    "持仓成本回报 3.21% · 2025-11-16 之后到账 · 1 项缺汇率未计入",
  );
  expect(portfolioApi.getDailyAssets).toHaveBeenCalledOnce();
  expect(portfolioApi.getDailyAssets).toHaveBeenCalledWith({ limit: 1 });
  expect(screen.getByText("近 12 个月实收分红")).toBeTruthy();
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
