// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import type { WatchRow } from "../../api/watch";
import type { Snapshot } from "../../types";
import fixture from "../../test/emptySnapshot.json";
import { defaultRules } from "../../test/watchRules";
import { WatchDetails } from "./WatchDetails";

const failedRow = (): WatchRow => ({
  entry: {
    symbol: "600036.SS",
    name: "测试公司",
    thesis: "关注分红的持续性",
    createdAt: "2026-11-04T12:00:00Z",
    pinned: false,
  },
  latest: {
    rules: defaultRules,
    yieldMatchesRule: true,
    symbol: "600036.SS",
    observedOn: "2026-11-04",
    checkedAt: "2026-11-04T12:00:00Z",
    price: 100,
    dividendPerShare: "5",
    yieldPct: 5,
    daily: null,
    weekly: null,
    status: "attention",
    issues: [],
    sources: [],
    dividendEvents: [],
  },
  dividendPrices: [],
  alert: false,
  checkedAt: "2026-11-05T12:00:00Z",
  refreshFailed: true,
});
const props = () => ({
  row: failedRow(),
  snapshot: fixture as unknown as Snapshot,
  rules: defaultRules,
  busy: false,
  error: "",
  onClose: vi.fn(),
  onGridEdited: vi.fn(),
  onGridClosed: vi.fn(),
  onGridSave: vi.fn().mockResolvedValue(true),
  onAsk: vi.fn(),
  onEdit: vi.fn(),
  onRemove: vi.fn(),
  onRetry: vi.fn(),
});

afterEach(cleanup);

it("explains a failed check beside retained evidence and retries without leaving details", () => {
  const p = props();
  render(<WatchDetails {...p} />);

  expect(
    screen.getByText("本次来源读取失败，保留 2026-11-04 的观察结果。"),
  ).toBeTruthy();
  expect(screen.getByText("收盘价 · 2026-11-04")).toBeTruthy();
  expect(screen.getByRole("dialog").textContent).toContain("进入关注区间");
  fireEvent.click(screen.getByRole("button", { name: "重新检查数据" }));
  expect(p.onRetry).toHaveBeenCalledOnce();
  expect(p.onClose).not.toHaveBeenCalled();
});

it("allows retrying a first failed check without claiming an older result exists", () => {
  const p = props();
  p.row.latest = null;
  render(<WatchDetails {...p} />);

  expect(
    screen.getByText("本次来源读取失败，尚无可用的观察结果。"),
  ).toBeTruthy();
  expect(screen.queryByText(/保留 .* 的观察结果/)).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "重新检查数据" }));
  expect(p.onRetry).toHaveBeenCalledOnce();
});

it("disables retry while a check is running", () => {
  const p = props();
  render(<WatchDetails {...p} busy />);

  const retry = screen.getByRole("button", { name: "重新检查数据" });
  expect((retry as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(retry);
  expect(p.onRetry).not.toHaveBeenCalled();
});

it("removes the failure notice and retry after a successful check", () => {
  const p = props();
  const view = render(<WatchDetails {...p} />);
  expect(screen.getByText(/本次来源读取失败/)).toBeTruthy();

  const row = failedRow();
  row.refreshFailed = false;
  row.latest!.observedOn = "2026-11-05";
  view.rerender(<WatchDetails {...p} row={row} />);

  expect(screen.queryByText(/本次来源读取失败/)).toBeNull();
  expect(screen.queryByRole("button", { name: "重新检查数据" })).toBeNull();
  expect(screen.getByText("收盘价 · 2026-11-05")).toBeTruthy();
});
