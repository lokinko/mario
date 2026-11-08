// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import type { Observation, WatchTrendPoint } from "../../api/watch";
import { defaultRules } from "../../test/watchRules";
import { WatchEvidence } from "./WatchEvidence";
import { WatchTrends } from "./WatchTrends";

function point(index: number): WatchTrendPoint {
  return {
    day: new Date(Date.UTC(2024, 11, 6 + index * 7)).toISOString().slice(0, 10),
    close: 42 + index / 10,
    weeklyLower: 36,
    weeklyMiddle: 40,
    weeklyUpper: 44,
    monthlyMiddle: 39,
    ma20: 40,
    ma60: 39.5,
    goldenCross: index === 50,
    provisional: false,
  };
}
function observation(trends?: WatchTrendPoint[]): Observation {
  return {
    symbol: "600036.SS",
    observedOn: "2026-11-04",
    checkedAt: "2026-11-04T12:00:00Z",
    price: 45,
    dividendPerShare: "2.5",
    yieldPct: 5.55,
    rules: defaultRules,
    yieldMatchesRule: true,
    daily: null,
    weekly: null,
    status: "incomplete",
    issues: [],
    sources: [],
    dividendEvents: [],
    trends,
  };
}
afterEach(cleanup);

it("renders both requested charts through shared company evidence with real dates and values", () => {
  const latest = observation([point(0), point(1)]);
  render(
    <WatchEvidence
      row={{
        latest,
        dividendPrices: [],
        checkedAt: null,
        refreshFailed: false,
      }}
      rules={defaultRules}
    />,
  );
  expect(
    screen.getByRole("img", { name: /趋势图 · 月线中轨与周线 BOLL/ }),
  ).toBeTruthy();
  expect(
    screen.getByRole("img", { name: /金叉图 · 20 周与 60 周均线/ }),
  ).toBeTruthy();
  expect(screen.getByText("月线中轨")).toBeTruthy();
  expect(screen.getByText("20 周均线")).toBeTruthy();
  expect(screen.getByText("60 周均线")).toBeTruthy();
  expect(screen.getByText(/周中轨上方 · 相对偏强/)).toBeTruthy();
  expect(screen.getByText(/截至 2026-11-04/)).toBeTruthy();
});

it("supports keyboard date inspection, range switching and marks actual cross events", () => {
  const trends = Array.from({ length: 100 }, (_, i) => point(i));
  trends[99].goldenCross = true;
  trends[99].provisional = true;
  const view = render(<WatchTrends latest={observation(trends)} />);
  expect(screen.getByLabelText(`${trends[50].day} 金叉`)).toBeTruthy();
  expect(screen.getByLabelText(`${trends[99].day} 暂态金叉`)).toBeTruthy();
  const date = screen.getByRole("slider", {
    name: "金叉图 · 20 周与 60 周均线查看日期",
  });
  fireEvent.change(date, { target: { value: "50" } });
  expect(date.getAttribute("aria-valuetext")).toBe(trends[50].day);
  expect(screen.getByText("金叉")).toBeTruthy();
  fireEvent.change(screen.getByRole("combobox", { name: "趋势图范围" }), {
    target: { value: "26" },
  });
  expect(date.getAttribute("max")).toBe("25");
  expect(date.getAttribute("aria-valuetext")).toBe(trends[99].day);
  expect(screen.queryByLabelText(`${trends[50].day} 金叉`)).toBeNull();
  expect(
    screen.getByRole("img", { name: /金叉图/ }).getAttribute("aria-label"),
  ).toContain(trends[74].day);
  const updated = trends.map((p) => ({ ...p, goldenCross: false, ma20: 38 }));
  view.rerender(<WatchTrends latest={observation(updated)} />);
  expect(screen.queryByLabelText(`${trends[99].day} 暂态金叉`)).toBeNull();
  expect(screen.getByText(/20 周均线在 60 周下方/)).toBeTruthy();
});

it("breaks lines at gaps while retaining available series and a single date", () => {
  const trends = [
    point(0),
    { ...point(1), monthlyMiddle: null, ma60: null },
    point(2),
  ];
  trends[0].ma60 = null;
  const { container } = render(<WatchTrends latest={observation(trends)} />);
  const monthly = container.querySelector('path[data-series="monthlyMiddle"]')!;
  expect(monthly.getAttribute("d")?.match(/M/g)).toHaveLength(2);
  expect(monthly.getAttribute("d")).not.toContain("L");
  const slider = screen.getByRole("slider", {
    name: "趋势图 · 月线中轨与周线 BOLL查看日期",
  });
  fireEvent.change(slider, { target: { value: "1" } });
  const section = screen.getByRole("region", {
    name: "趋势图 · 月线中轨与周线 BOLL",
  });
  expect(within(section).getByText("—")).toBeTruthy();
  expect(within(section).getByText("40.00", { selector: "dd" })).toBeTruthy();
  cleanup();
  render(<WatchTrends latest={observation([point(0)])} />);
  expect((screen.getAllByRole("slider")[0] as HTMLInputElement).disabled).toBe(
    true,
  );
  expect(screen.getByRole("img", { name: /趋势图 · 月线/ })).toBeTruthy();
});

it("shows older observations and first checks without inventing history", () => {
  const view = render(<WatchTrends latest={null} />);
  expect(screen.getByText(/尚无趋势资料/)).toBeTruthy();
  expect(screen.queryByRole("img")).toBeNull();
  view.rerender(<WatchTrends latest={observation()} />);
  expect(screen.getByText(/尚无趋势资料/)).toBeTruthy();
  expect(screen.queryByRole("slider")).toBeNull();
});
