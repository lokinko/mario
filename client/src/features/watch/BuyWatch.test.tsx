// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { BuyWatch } from "./BuyWatch";
import * as api from "../../api/watch";
import type { Snapshot } from "../../types";
import fixture from "../../test/emptySnapshot.json";

vi.mock("../../api/watch", () => ({
  getWatchBoard: vi.fn(),
  saveWatch: vi.fn(),
  deleteWatch: vi.fn(),
  readWatchAlerts: vi.fn(),
  refreshWatch: vi.fn(),
}));
const empty: api.WatchBoard = {
  rows: [],
  weekly: [],
  weekStart: "2026-11-02",
  weekEnd: "2026-11-08",
  minYieldPct: 5,
  bandDistancePct: 2,
};
const row: api.WatchRow = {
  entry: {
    symbol: "600036.SS",
    name: "测试公司",
    thesis: "关注分红的持续性",
    createdAt: "2026-11-04T12:00:00Z",
  },
  latest: {
    symbol: "600036.SS",
    observedOn: "2026-11-04",
    checkedAt: "2026-11-04T12:00:00Z",
    price: 100,
    dividendPerShare: "5",
    yieldPct: 5,
    daily: {
      lower: 100,
      middle: 102,
      upper: 104,
      distancePct: 0,
      nearLower: true,
    },
    weekly: {
      lower: 100,
      middle: 103,
      upper: 106,
      distancePct: 0,
      nearLower: true,
    },
    status: "attention",
    issues: [],
    sources: ["https://example.test/source"],
    dividendEvents: [],
  },
  alert: true,
  checkedAt: "2026-11-04T12:00:00Z",
  refreshFailed: false,
};
const props = () => ({
  snapshot: fixture as unknown as Snapshot,
  onAsk: vi.fn(),
  flash: vi.fn(),
  onEdited: vi.fn(),
  onSaved: vi.fn(),
});
beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(api.getWatchBoard).mockResolvedValue(structuredClone(empty));
});
afterEach(cleanup);

it("adds an unheld company without creating a financial position", async () => {
  const p = props();
  render(<BuyWatch {...p} />);
  fireEvent.click(
    await screen.findByRole("button", { name: "加入第一家公司" }),
  );
  fireEvent.change(screen.getByLabelText("证券代码"), {
    target: { value: "600036.SS" },
  });
  fireEvent.change(screen.getByLabelText("公司名称"), {
    target: { value: "测试公司" },
  });
  fireEvent.change(screen.getByLabelText("我认可它的理由"), {
    target: { value: "关注分红" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存观察公司" }));
  await waitFor(() =>
    expect(api.saveWatch).toHaveBeenCalledWith({
      symbol: "600036.SS",
      name: "测试公司",
      thesis: "关注分红",
    }),
  );
  expect(p.onEdited).toHaveBeenCalled();
  await waitFor(() => expect(p.onSaved).toHaveBeenCalled());
});

it("keeps the entry editable after save failure", async () => {
  vi.mocked(api.saveWatch).mockRejectedValue(new Error("写入失败"));
  render(<BuyWatch {...props()} />);
  fireEvent.click(
    await screen.findByRole("button", { name: "加入第一家公司" }),
  );
  fireEvent.change(screen.getByLabelText("证券代码"), {
    target: { value: "600036.SS" },
  });
  fireEvent.change(screen.getByLabelText("公司名称"), {
    target: { value: "测试公司" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存观察公司" }));
  await screen.findByText(/写入失败/);
  expect((screen.getByLabelText("公司名称") as HTMLInputElement).value).toBe(
    "测试公司",
  );
  expect(screen.getByRole("dialog")).toBeTruthy();
});

it("opens grounded discussion, acknowledges alerts and preserves missing-data status", async () => {
  vi.mocked(api.getWatchBoard).mockResolvedValue({
    ...empty,
    rows: [structuredClone(row)],
  });
  const p = props();
  render(<BuyWatch {...p} />);
  await screen.findByText("进入关注区间");
  fireEvent.click(screen.getByRole("button", { name: "结合我的交易讨论" }));
  expect(p.onAsk).toHaveBeenCalledWith(expect.stringContaining("600036.SS"));
  expect(p.onAsk).toHaveBeenCalledWith(expect.stringContaining("最近真实交易"));
  fireEvent.click(screen.getByRole("button", { name: "标为已读" }));
  await waitFor(() =>
    expect(api.readWatchAlerts).toHaveBeenCalledWith("600036.SS"),
  );
  const missing = structuredClone(row);
  missing.latest!.status = "incomplete";
  missing.latest!.weekly = null;
  missing.latest!.issues = ["weekly_history_incomplete"];
  vi.mocked(api.refreshWatch).mockResolvedValue({ ...empty, rows: [missing] });
  await waitFor(() =>
    expect(
      screen
        .getByRole("button", { name: "检查最新收盘" })
        .hasAttribute("disabled"),
    ).toBe(false),
  );
  fireEvent.click(screen.getByRole("button", { name: "检查最新收盘" }));
  await screen.findByText("待补齐数据");
  expect(screen.queryByText("进入关注区间")).toBeNull();
  expect(screen.getByRole("button", { name: "重新检查数据" })).toBeTruthy();
});

it("shows the dividend coverage and exact events behind the closing yield", async () => {
  const stored = structuredClone(row);
  stored.latest!.dividendBasis = {
    asOf: "2026-11-04",
    fetchedAt: "2026-11-04T10:00:00Z",
  };
  stored.latest!.dividendEvents = [
    {
      reportDate: "2025-12-31",
      exDate: "2026-07-01",
      cashPerTen: "12.345",
      bonusPerTen: "0",
      transferPerTen: "2",
    },
  ];
  vi.mocked(api.getWatchBoard).mockResolvedValue({ ...empty, rows: [stored] });
  render(<BuyWatch {...props()} />);
  fireEvent.click(
    await screen.findByRole("button", { name: "查看依据与交易" }),
  );
  expect(screen.getByText(/分红资料覆盖至 2026-11-04/)).toBeTruthy();
  expect(screen.getByText(/除息日 2026-07-01/).textContent).toContain("12.345");
  expect(screen.getByText(/除息日 2026-07-01/).textContent).toContain(
    "报告期 2025-12-31",
  );
});
