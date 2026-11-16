// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { PriceCollectionDashboard } from "../../api/activity";
import { PriceCollection } from "./PriceCollection";
const originalStyle = window.getComputedStyle.bind(window);
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
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
const day = {
  date: "2026-11-16",
  expected: 300,
  successful: 298,
  failed: 1,
  pending: 0,
  lagging: 1,
  updatedAt: 1794816000,
};
const fixture: PriceCollectionDashboard = {
  generatedAt: 1794816000,
  years: 10,
  targetThrough: "2026-11-16",
  universe: {
    asOf: "2026-11-11",
    fetchedAt: 1794816000,
    sourceUrl: "https://example.test/official.xls",
    members: [{ symbol: "600036.SS", name: "招商银行" }],
  },
  universeAttemptedAt: 1794816000,
  universeError: "",
  summary: day,
  daily: [day],
  rows: [
    {
      symbol: "600036.SS",
      name: "招商银行",
      status: "success",
      issueCodes: [],
      error: "",
      attemptedAt: 1794816000,
      succeededAt: 1794816000,
      firstOn: "2016-11-16",
      latestOn: "2026-11-16",
      bars: 2430,
      provider: "东方财富",
    },
    {
      symbol: "600941.SS",
      name: "中国移动",
      status: "failed",
      issueCodes: ["source_failed", "short_history"],
      error: "东方财富连接失败；Yahoo HTTP 429",
      attemptedAt: 1794816000,
      succeededAt: 1794729600,
      firstOn: "2022-01-05",
      latestOn: "2026-11-13",
      bars: 1100,
      provider: "Yahoo Finance",
    },
    {
      symbol: "000001.SZ",
      name: "平安银行",
      status: "lagging",
      issueCodes: ["latest_close_lagging"],
      error: "",
      attemptedAt: 1794816000,
      succeededAt: 1794816000,
      firstOn: "2016-11-16",
      latestOn: "2026-11-13",
      bars: 2429,
      provider: "东方财富",
    },
  ],
};
it("shows actual coverage and source errors, and filters failures without hiding retained prices", () => {
  render(<PriceCollection data={fixture} loading={false} />);
  expect(screen.getByText("招商银行")).toBeTruthy();
  expect(screen.getByText("东方财富连接失败；Yahoo HTTP 429")).toBeTruthy();
  expect(
    screen.getByText("历史不足十年，请核对上市时间与来源覆盖"),
  ).toBeTruthy();
  expect(screen.getByText("每日采集结果")).toBeTruthy();
  fireEvent.click(screen.getByText("异常与待获取"));
  expect(screen.queryByText("招商银行")).toBeNull();
  expect(screen.getByText("中国移动")).toBeTruthy();
  expect(screen.getByText(/1100 条日线/)).toBeTruthy();
  fireEvent.change(screen.getByLabelText("搜索价格证券"), {
    target: { value: "000001" },
  });
  expect(screen.queryByText("中国移动")).toBeNull();
  expect(screen.getByText("平安银行")).toBeTruthy();
});
it("reports missing scope explicitly rather than claiming that all prices succeeded", () => {
  render(
    <PriceCollection
      data={{
        ...fixture,
        universe: null,
        universeError: "官方成分表超时",
        rows: [],
      }}
      loading={false}
    />,
  );
  expect(screen.getByText(/尚未取得有效的300只成分表/)).toBeTruthy();
  expect(screen.getByRole("status").textContent).toContain("官方成分表超时");
});
