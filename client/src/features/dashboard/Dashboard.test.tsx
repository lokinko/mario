// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { Dashboard } from "./Dashboard";
import type { Snapshot } from "../../types";
import emptySnapshot from "../../test/emptySnapshot.json";
import * as api from "../../api";

vi.mock("../../api", () => ({
  getPortfolioCheckins: vi.fn().mockResolvedValue([]),
  getReviewReminders: vi.fn().mockResolvedValue(null),
  savePortfolioCheckin: vi.fn(),
  getDailyAssets: vi.fn().mockResolvedValue({ records: [], nextBefore: null }),
  getPortfolioEvents: vi.fn().mockResolvedValue([]),
}));
afterEach(cleanup);

it("prepares an overall analysis question without running a model", () => {
  const navigate = vi.fn();
  render(
    <Dashboard
      snapshot={emptySnapshot as Snapshot}
      navigate={navigate}
      flash={vi.fn()}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "继续做整体分析" }));
  expect(navigate).toHaveBeenCalledWith("advisor");
  expect(window.sessionStorage.getItem("mario.advisorQuestion")).toContain(
    "整体资产与投资分析",
  );
  window.sessionStorage.clear();
});

it("sorts holdings while preserving missing cost labels", async () => {
  const snapshot: Snapshot = {
    ...(emptySnapshot as Snapshot),
    totalValue: 30000,
    holdings: [
      {
        id: "cash",
        name: "现金测试",
        symbol: "",
        assetClass: "现金",
        marketValue: 10000,
        costBasis: 0,
        targetPct: 30,
        currency: "CNY",
        fxRateToBase: null,
        valuationDate: "2026-10-11",
        fxRateSource: "",
        fxRateObservedOn: "",
      },
      {
        id: "stock",
        name: "股票测试",
        symbol: "",
        assetClass: "股票",
        marketValue: 20000,
        costBasis: 0,
        targetPct: 70,
        currency: "CNY",
        fxRateToBase: null,
        valuationDate: "2026-10-11",
        fxRateSource: "",
        fxRateObservedOn: "",
      },
    ],
  };
  render(<Dashboard snapshot={snapshot} navigate={vi.fn()} flash={vi.fn()} />);
  expect(screen.getByText("待补成本")).toBeTruthy();
  const rows = screen.getAllByRole("row");
  expect(rows[1].textContent).toContain("股票测试");
  fireEvent.click(screen.getByRole("button", { name: "持仓币种" }));
  expect(
    screen
      .getByRole("button", { name: "持仓币种" })
      .getAttribute("aria-pressed"),
  ).toBe("true");
  await waitFor(() => expect(api.getPortfolioEvents).toHaveBeenCalled());
});

it("shows a failed ledger as unavailable without hiding other analysis, and retries", async () => {
  vi.mocked(api.getPortfolioEvents).mockRejectedValueOnce(
    new Error("ledger offline"),
  );
  render(
    <Dashboard
      snapshot={emptySnapshot as Snapshot}
      navigate={vi.fn()}
      flash={vi.fn()}
    />,
  );
  const ledger = screen.getByRole("article", { name: "资金流水分析" });
  await within(ledger).findByRole("alert");
  expect(within(ledger).queryByText("¥0")).toBeNull();
  expect(screen.queryByText("已记录决策")).toBeNull();
  fireEvent.click(within(ledger).getByRole("button", { name: "重试读取" }));
  await within(ledger).findByText(
    "所选区间没有当前币种流水，零值只表示未记录。",
  );
});

it("blocks new check-ins when existing snapshots could not be loaded", async () => {
  vi.mocked(api.getPortfolioCheckins).mockRejectedValueOnce(
    new Error("check-ins offline"),
  );
  render(
    <Dashboard
      snapshot={emptySnapshot as Snapshot}
      navigate={vi.fn()}
      flash={vi.fn()}
    />,
  );
  await screen.findByText("组合快照读取失败，暂时不能冻结新记录。");
  expect(
    (screen.getByRole("button", { name: "建立组合基线" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "重试读取快照" }));
  await waitFor(() =>
    expect(
      screen.queryByText("组合快照读取失败，暂时不能冻结新记录。"),
    ).toBeNull(),
  );
});

it("directs the first goal action to the goals section", async () => {
  const navigate = vi.fn();
  render(
    <Dashboard
      snapshot={emptySnapshot as Snapshot}
      navigate={navigate}
      flash={vi.fn()}
    />,
  );
  fireEvent.click(
    await screen.findByRole("button", { name: "设定第一个目标" }),
  );
  expect(navigate).toHaveBeenCalledWith("foundation", "goals");
});

it("does not present unconverted allocations", () => {
  const snapshot = {
    ...(emptySnapshot as Snapshot),
    valuationStatus: { ...emptySnapshot.valuationStatus, comparable: false },
  };
  const { container } = render(
    <Dashboard snapshot={snapshot} navigate={vi.fn()} flash={vi.fn()} />,
  );
  expect(container.querySelector(".donut")).toBeNull();
  expect(screen.getByText("补齐汇率后查看目标进度")).toBeTruthy();
});

it("removes the four modules and their controls even when analysis data exists", async () => {
  const snapshot: Snapshot = {
    ...(emptySnapshot as Snapshot),
    goals: [
      {
        id: "goal",
        name: "目标",
        targetAmount: 10000,
        currentAmount: 0,
        monthlyContribution: 0,
        targetDate: "2028-01-01",
        priority: "刚性",
      },
    ],
    findings: [
      {
        level: "high",
        title: "旧风险提示",
        detail: "旧风险依据",
        action: "旧风险操作",
      },
    ],
    plan: {
      ...emptySnapshot.plan,
      riskStatus: "over",
      stressLossPct: 30,
      riskCapacityPct: 10,
      rebalancing: [
        {
          holdingId: "stock",
          name: "旧再平衡项目",
          currentPct: 90,
          targetPct: 50,
          deviationPct: 40,
          direction: "减少",
          amount: 10000,
        },
      ],
    },
  };
  render(<Dashboard snapshot={snapshot} navigate={vi.fn()} flash={vi.fn()} />);
  for (const name of [
    "风险提醒",
    "风险预算",
    "如果资产下跌，会影响多少？",
    "最近一期投资变化",
  ]) {
    expect(screen.queryByRole("heading", { name })).toBeNull();
  }
  expect(screen.queryByRole("slider")).toBeNull();
  expect(screen.queryByRole("region", { name: "投资变化归因" })).toBeNull();
  expect(screen.queryByText("旧风险提示")).toBeNull();
  expect(screen.queryByText("旧风险操作")).toBeNull();
  expect(screen.queryByText("旧再平衡项目")).toBeNull();
  expect(screen.getByRole("heading", { name: "分析数据完整度" })).toBeTruthy();
  expect(screen.getByRole("heading", { name: "目标进度" })).toBeTruthy();
  expect(screen.getByText("组合变化与快照")).toBeTruthy();
  expect(screen.queryByText("已记录决策")).toBeNull();
  expect(screen.getByRole("article", { name: "资金流水分析" })).toBeTruthy();
});
