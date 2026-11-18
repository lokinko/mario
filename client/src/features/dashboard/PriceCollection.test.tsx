// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { PriceCollectionDashboard } from "../../api/activity";
import { PriceCollection, PriceSummary } from "./PriceCollection";
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
const csi300 = {
  scope: "csi300",
  market: "cn",
  instrumentType: "EQUITY",
  currency: "CNY",
  tracked: true,
} as const;
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
      ...csi300,
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
      ...csi300,
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
      ...csi300,
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
    {
      symbol: "QQQ",
      name: "",
      scope: "other",
      market: "us",
      instrumentType: "ETF",
      currency: "USD",
      tracked: true,
      status: "success",
      issueCodes: [],
      error: "",
      attemptedAt: 1794816000,
      succeededAt: 1794816000,
      firstOn: "2016-11-14",
      latestOn: "2026-11-13",
      bars: 2515,
      provider: "东方财富",
    },
    {
      symbol: "VOO",
      name: "",
      scope: "other",
      market: "us",
      instrumentType: "ETF",
      currency: "USD",
      tracked: false,
      status: "inactive",
      issueCodes: ["tracking_inactive"],
      error: "",
      attemptedAt: 1794211200,
      succeededAt: 1794211200,
      firstOn: "2016-11-07",
      latestOn: "2026-11-06",
      bars: 2510,
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
  expect(screen.getByTitle(/1100 条日线/)).toBeTruthy();
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
it("visualizes status counts with a legend that filters the table by status", () => {
  render(<PriceCollection data={fixture} loading={false} />);
  expect(
    screen
      .getByRole("img", { name: /^今日价格状态分布/ })
      .getAttribute("aria-label"),
  ).toBe("今日价格状态分布：今日成功 298，获取失败 1，等待更新 0，价格滞后 1");
  expect(
    screen.getByRole("img", { name: /^2026-11-16 价格状态分布/ }),
  ).toBeTruthy();
  const legend = within(screen.getByRole("group", { name: "价格状态图例" }));
  const failed = legend.getByRole("button", { name: /获取失败/ });
  expect(failed.textContent).toContain("1");
  fireEvent.click(failed);
  expect(failed.getAttribute("aria-pressed")).toBe("true");
  expect(screen.queryByText("招商银行")).toBeNull();
  expect(screen.queryByText("平安银行")).toBeNull();
  expect(screen.getByText("中国移动")).toBeTruthy();
  fireEvent.click(failed);
  expect(screen.getByText("招商银行")).toBeTruthy();
});

it("lists registered overseas ETFs and inactive symbols by scope and filters by scope", () => {
  render(<PriceCollection data={fixture} loading={false} />);
  const scope = within(screen.getByLabelText("价格维护范围"));
  const csi = scope.getByText("沪深300成分").closest("tr")!;
  expect(
    within(csi as HTMLElement)
      .getAllByRole("cell")
      .map((cell) => cell.textContent),
  ).toEqual(["沪深300成分", "3", "1", "1", "0", "1", "0"]);
  const etf = scope.getByText("美股ETF").closest("tr")!;
  expect(
    within(etf as HTMLElement)
      .getAllByRole("cell")
      .map((cell) => cell.textContent),
  ).toEqual(["美股ETF", "1", "1", "0", "0", "0", "1"]);
  expect(screen.getByText("QQQ")).toBeTruthy();
  expect(screen.getAllByText("美股 · ETF · USD").length).toBe(2);
  expect(
    screen.getByText("近 48 小时无持仓或观察登记，已停止刷新，保留已有价格"),
  ).toBeTruthy();
  fireEvent.mouseDown(screen.getByRole("combobox", { name: "维护范围筛选" }));
  fireEvent.click(document.querySelector('.ant-select-item[title="美股ETF"]')!);
  expect(screen.queryByText("招商银行")).toBeNull();
  expect(screen.getByText("QQQ")).toBeTruthy();
  expect(screen.getByText("VOO")).toBeTruthy();
});

it("summarizes status counts on the overview and leaves per-stock rows to the detail view", () => {
  const onOpenDetail = vi.fn();
  render(<PriceSummary data={fixture} onOpenDetail={onOpenDetail} />);
  expect(
    screen.getByRole("img", {
      name: "今日价格状态分布：今日成功 298，获取失败 1，等待更新 0，价格滞后 1",
    }),
  ).toBeTruthy();
  expect(screen.queryByText("招商银行")).toBeNull();
  expect(screen.getByText("美股ETF")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "查看明细" }));
  expect(onOpenDetail).toHaveBeenCalledOnce();
});
