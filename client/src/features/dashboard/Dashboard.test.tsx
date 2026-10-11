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
  getDecisions: vi.fn().mockResolvedValue([]),
  getRuleEffectiveness: vi
    .fn()
    .mockResolvedValue({ applicableChecks: 0, adherencePct: null }),
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

it("adjusts the hypothetical loss and sorts holdings while preserving missing cost labels", async () => {
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
  const scenario = screen.getByRole("region", { name: "假设情景分析" });
  expect(within(scenario).getByText("¥4,000")).toBeTruthy();
  fireEvent.change(screen.getByRole("slider", { name: "投资市值下跌百分比" }), {
    target: { value: "50" },
  });
  expect(within(scenario).getByText("¥10,000")).toBeTruthy();
  expect(
    within(scenario).getByText("该假设下的资产降幅超过已填写的回撤容忍度。"),
  ).toBeTruthy();
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
  expect(await screen.findByText("已记录决策")).toBeTruthy();
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

it("prioritizes high risks and keeps additional findings reachable", async () => {
  const snapshot = {
    ...(emptySnapshot as Snapshot),
    findings: [
      {
        level: "low" as const,
        title: "一般提示",
        detail: "提示依据",
        action: "继续观察",
      },
      {
        level: "medium" as const,
        title: "中等风险",
        detail: "风险依据",
        action: "检查",
      },
      {
        level: "high" as const,
        title: "优先风险",
        detail: "风险依据",
        action: "处理",
      },
      {
        level: "low" as const,
        title: "更多提示",
        detail: "提示依据",
        action: "观察",
      },
    ],
  };
  const { container } = render(
    <Dashboard snapshot={snapshot} navigate={vi.fn()} flash={vi.fn()} />,
  );
  await waitFor(() =>
    expect(container.querySelector(".finding strong")?.textContent).toBe(
      "优先风险",
    ),
  );
  expect(screen.queryByText("更多提示")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "查看全部 4 项" }));
  expect(screen.getByText("更多提示")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "收起" }));
  expect(screen.queryByText("更多提示")).toBeNull();
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

it("does not present unconverted allocations or risk estimates", async () => {
  const snapshot = {
    ...(emptySnapshot as Snapshot),
    valuationStatus: { ...emptySnapshot.valuationStatus, comparable: false },
  };
  const { container } = render(
    <Dashboard snapshot={snapshot} navigate={vi.fn()} flash={vi.fn()} />,
  );
  await waitFor(() =>
    expect(
      screen.getByText("等待汇率", { selector: ".risk-status" }),
    ).toBeTruthy(),
  );
  expect(container.querySelector(".donut")).toBeNull();
  expect(
    [...container.querySelectorAll(".risk-comparison-row strong")].map(
      (node) => node.textContent,
    ),
  ).toEqual(["—", "—"]);
});
