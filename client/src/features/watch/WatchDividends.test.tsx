// @vitest-environment jsdom
import { defaultRules } from "../../test/watchRules";
import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import type { Observation } from "../../api/watch";
import { WatchDividends } from "./WatchDividends";

afterEach(cleanup);
const current = {
  reportDate: "2025-12-31",
  exDate: "2026-07-10",
  cashPerTen: "10.03",
  bonusPerTen: "0",
  transferPerTen: "0",
};
const older = { ...current, reportDate: "2023-12-31", exDate: "2024-07-11" };
const latest: Observation = {
  rules: defaultRules,
  yieldMatchesRule: null,
  symbol: "600036.SS",
  observedOn: "2026-11-06",
  checkedAt: "2026-11-07T06:00:00Z",
  price: 40,
  dividendPerShare: "2.016",
  yieldPct: 5.04,
  daily: null,
  weekly: null,
  status: "incomplete",
  issues: [],
  sources: [],
  dividendEvents: [current],
  dividendBasis: { asOf: "2026-11-07", fetchedAt: "2026-11-07T06:00:00Z" },
  dividendHistory: {
    start: "2023-11-06",
    end: "2026-11-06",
    events: [older, current],
  },
};

it("shows every history event in reverse order and marks only TTM events", () => {
  render(<WatchDividends latest={latest} />);
  const table = screen.getByRole("table", { name: "近三年已实施分红事件" });
  const rows = within(table).getAllByRole("row");
  expect(rows).toHaveLength(3);
  expect(rows[1].textContent).toContain("2026-07-10");
  expect(rows[1].textContent).toContain("计入");
  expect(rows[2].textContent).toContain("2024-07-11");
  expect(rows[2].textContent).not.toContain("计入");
  expect(screen.getByText(/金额为税前、当次每十股口径/)).toBeTruthy();
});

it("keeps old observations readable without claiming three-year coverage", () => {
  render(<WatchDividends latest={{ ...latest, dividendHistory: undefined }} />);
  expect(screen.queryByText("近三年已实施分红")).toBeNull();
  expect(
    screen.getByRole("table", { name: "TTM 窗口已实施分红事件" }),
  ).toBeTruthy();
});

it("does not turn missing or stale history into a known empty three-year window", () => {
  const empty = {
    ...latest,
    dividendHistory: { ...latest.dividendHistory!, events: [] },
  };
  const view = render(<WatchDividends latest={empty} />);
  expect(screen.getByText("近三年没有已实施的分红方案。")).toBeTruthy();
  view.rerender(
    <WatchDividends
      latest={{
        ...empty,
        dividendBasis: { ...latest.dividendBasis!, asOf: "2026-11-04" },
      }}
    />,
  );
  expect(screen.queryByText("近三年没有已实施的分红方案。")).toBeNull();
  expect(screen.getByText("暂无可展示的分红事件。")).toBeTruthy();
});
