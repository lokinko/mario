// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { DailyReviewPage } from "./DailyReviewPage";
import * as dailyReviewApi from "../../api/dailyReview";
import * as reviewsApi from "../../api/reviews";
import type { DailyReviewFeedResponse } from "../../types/ai";
import type { Snapshot } from "../../types/user";

vi.mock("../../api/dailyReview", () => ({
  getDailyReviewFeed: vi.fn(),
  saveDailyReviewSettings: vi.fn(),
  runDailyReviewNow: vi.fn(),
  markDailyReviewRead: vi.fn(),
}));

vi.mock("../../api/reviews", () => ({
  getSystemReviews: vi.fn(),
  saveSystemReview: vi.fn(),
  saveInvestmentRule: vi.fn(),
}));

const sampleSnapshot = {
  holdings: [
    {
      id: "h1",
      symbol: "600519.SS",
      name: "贵州茅台",
      assetClass: "股票",
      marketValue: 150000,
      costBasis: 140000,
      targetPct: 20,
      currency: "CNY",
    },
  ],
} as Snapshot;

const sampleFeed: DailyReviewFeedResponse = {
  settings: {
    enabled: true,
    scheduleTime: "07:30",
    scope: "holdingsAndWatch",
    webSearch: true,
  },
  timezone: "Asia/Shanghai",
  latestReviewDay: "2026-11-20",
  unreadCount: 2,
  runs: [
    {
      id: "run-1",
      reviewDay: "2026-11-20",
      trigger: "scheduled",
      status: "completed",
      aiStatus: "completed",
      aiModel: "gpt-5.4",
      degradedReason: null,
      itemCount: 2,
      announcementCount: 1,
      createdAt: "2026-11-21T00:00:00Z",
    },
  ],
  items: [
    {
      id: "filing:2026-11-20:cninfo:1",
      runId: "run-1",
      reviewDay: "2026-11-20",
      kind: "officialAnnouncement",
      origin: "officialFiling",
      importance: "high",
      title: "贵州茅台：关于2026年中期利润分配方案的公告",
      summary: "持仓标的 贵州茅台（600519.SS）通过巨潮资讯网发布分红派息公告。",
      body: "持仓标的 贵州茅台（600519.SS）通过巨潮资讯网发布分红派息公告。\n公告全文见官方披露链接。",
      symbol: "600519.SS",
      assetName: "贵州茅台",
      metrics: [],
      sources: [
        {
          label: "关于2026年中期利润分配方案的公告",
          url: "https://static.cninfo.com.cn/finalpage/1.PDF",
          publisher: "巨潮资讯网",
          publishedOn: "2026-11-20",
        },
      ],
      tags: ["持仓标的", "分红派息"],
      askPrompt: "请解读贵州茅台最新中期分红公告。",
      readAt: null,
      createdAt: "2026-11-20T10:30:00Z",
    },
    {
      id: "ai:2026-11-20:0",
      runId: "run-1",
      reviewDay: "2026-11-20",
      kind: "aiInsight",
      origin: "aiReview",
      importance: "medium",
      title: "昨日组合复盘：白酒仓位保持稳健",
      summary: "持仓结构符合红利与安全边际纪律。",
      body: "昨日整体组合回撤有限，重点关注中期分红兑现节奏。",
      symbol: "600519.SS",
      assetName: "贵州茅台",
      metrics: [
        {
          label: "单日变动",
          value: "+1.25%",
          tone: "positive",
          sensitive: false,
        },
      ],
      sources: [],
      tags: ["AI复盘"],
      askPrompt: "请结合规则评估当前仓位。",
      readAt: null,
      createdAt: "2026-11-21T00:00:00Z",
    },
  ],
  hasMore: false,
  nextBefore: null,
};

beforeEach(() => {
  vi.mocked(dailyReviewApi.getDailyReviewFeed).mockResolvedValue(sampleFeed);
  vi.mocked(reviewsApi.getSystemReviews).mockResolvedValue([]);
});

afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

it("renders the microblog feed with official CNINFO links, supports filtering, mark-read, and ask AI", async () => {
  const onAsk = vi.fn();
  const flash = vi.fn();
  vi.mocked(dailyReviewApi.markDailyReviewRead).mockResolvedValue({
    ok: true,
    unreadCount: 1,
  });

  render(
    <DailyReviewPage
      snapshot={sampleSnapshot}
      onAsk={onAsk}
      onOpenSettings={vi.fn()}
      flash={flash}
    />,
  );

  expect(
    await screen.findByText("贵州茅台：关于2026年中期利润分配方案的公告"),
  ).toBeTruthy();
  expect(screen.getByText("每日 07:30 自动复盘前一日")).toBeTruthy();
  const link = screen.getByRole("link", {
    name: /巨潮资讯网.*关于2026年中期利润分配方案的公告/,
  }) as HTMLAnchorElement;
  expect(link.href).toBe("https://static.cninfo.com.cn/finalpage/1.PDF");

  // Filter to official announcements only
  fireEvent.click(screen.getByRole("button", { name: "官方公告" }));
  expect(screen.queryByText("昨日组合复盘：白酒仓位保持稳健")).toBeNull();
  expect(
    screen.getByText("贵州茅台：关于2026年中期利润分配方案的公告"),
  ).toBeTruthy();

  // Mark item read and ask AI
  fireEvent.click(screen.getByRole("button", { name: "已读" }));
  await waitFor(() =>
    expect(dailyReviewApi.markDailyReviewRead).toHaveBeenCalledWith({
      id: "filing:2026-11-20:cninfo:1",
    }),
  );

  fireEvent.click(screen.getByRole("button", { name: "问 AI 深聊" }));
  expect(onAsk).toHaveBeenCalledWith("请解读贵州茅台最新中期分红公告。");
});

it("publishes manual review notes into the timeline and saves schedule settings", async () => {
  const flash = vi.fn();
  vi.mocked(reviewsApi.saveSystemReview).mockResolvedValue({
    id: "rev-new",
    content: "今天重点跟踪茅台分红兑现",
    periodLabel: "",
    adherenceScore: null,
    processSummary: "",
    ruleViolations: "",
    lessons: "",
    nextActions: "",
    nextReviewDate: "",
    snapshot: {
      portfolioValue: 150000,
      baseCurrency: "CNY",
      portfolioComparable: true,
      emergencyMonths: 12,
      concentrationPct: 20,
      riskStatus: "within",
      highRiskFindings: 0,
      activeRules: 1,
    },
    createdAt: "2026-11-21T01:00:00Z",
  });
  vi.mocked(dailyReviewApi.saveDailyReviewSettings).mockResolvedValue({
    enabled: true,
    scheduleTime: "08:00",
    scope: "holdingsOnly",
    webSearch: true,
  });

  render(
    <DailyReviewPage
      snapshot={sampleSnapshot}
      onAsk={vi.fn()}
      onOpenSettings={vi.fn()}
      flash={flash}
    />,
  );

  const composer = await screen.findByLabelText("写一条今日复盘想法");
  fireEvent.change(composer, {
    target: { value: "今天重点跟踪茅台分红兑现" },
  });
  fireEvent.click(screen.getByRole("button", { name: "发布随手记" }));
  await waitFor(() =>
    expect(reviewsApi.saveSystemReview).toHaveBeenCalledWith({
      content: "今天重点跟踪茅台分红兑现",
    }),
  );
  expect(await screen.findByText("我的复盘随手记")).toBeTruthy();

  fireEvent.click(screen.getByRole("button", { name: "复盘设置" }));
  fireEvent.change(screen.getByDisplayValue("07:30"), {
    target: { value: "08:00" },
  });
  fireEvent.change(screen.getByDisplayValue("当前持仓 + 个人自选"), {
    target: { value: "holdingsOnly" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存设置" }));
  await waitFor(() =>
    expect(dailyReviewApi.saveDailyReviewSettings).toHaveBeenCalledWith({
      enabled: true,
      scheduleTime: "08:00",
      scope: "holdingsOnly",
      webSearch: true,
    }),
  );
});
