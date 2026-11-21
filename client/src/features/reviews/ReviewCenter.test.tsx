// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ReviewCenter } from "./ReviewCenter";
import * as reviewsApi from "../../api/reviews";
import type { SystemReviewRecord } from "../../types/user";

vi.mock("../../api/reviews", () => ({
  getInvestmentRules: vi.fn(),
  getInvestmentRuleHistory: vi.fn(),
  getSystemReviews: vi.fn(),
  saveSystemReview: vi.fn(),
  saveInvestmentRule: vi.fn(),
  updateInvestmentRule: vi.fn(),
}));
beforeEach(() => {
  vi.mocked(reviewsApi.getInvestmentRules).mockResolvedValue([]);
  vi.mocked(reviewsApi.getSystemReviews).mockResolvedValue([]);
});
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

const legacy = {
  id: "legacy",
  createdAt: "2026-09-30",
  periodLabel: "2026 Q3",
  adherenceScore: 4,
  processSummary: "先检查依据",
  ruleViolations: "信息不足时过早判断",
  lessons: "留意反方证据",
  nextActions: "继续跟踪资料",
  nextReviewDate: "2026-12-31",
  snapshot: {},
} as SystemReviewRecord;

it("saves the complete text from a single input and displays it literally", async () => {
  const content =
    " 我的观点：保持观察。\n\n顾虑：资料不足。\n思考：<保留原文> **不解析标记**。 ";
  const stored = {
    ...legacy,
    content,
    adherenceScore: null,
    nextReviewDate: "",
  };
  vi.mocked(reviewsApi.saveSystemReview).mockResolvedValue(stored);
  const flash = vi.fn();
  render(<ReviewCenter tab="reviews" flash={flash} onAsk={vi.fn()} />);
  fireEvent.click(await screen.findByRole("button", { name: "新建复盘" }));
  const input = await screen.findByRole("textbox", {
    name: "我的观点、顾虑和思考",
  });
  const form = input.closest("form")!;
  expect(form.querySelectorAll("textarea")).toHaveLength(1);
  expect(form.querySelectorAll("input, select")).toHaveLength(0);
  expect(document.querySelector(".rule-workbench")).toBeNull();
  expect(screen.queryByLabelText("下次复盘日")).toBeNull();
  fireEvent.change(input, { target: { value: content } });
  vi.mocked(reviewsApi.getSystemReviews).mockResolvedValue([stored]);
  fireEvent.click(screen.getByRole("button", { name: "保存复盘" }));
  await waitFor(() =>
    expect(reviewsApi.saveSystemReview).toHaveBeenCalledWith({ content }),
  );
  await waitFor(() => expect(flash).toHaveBeenCalledWith("复盘记录已保存"));
  expect(screen.queryByRole("dialog")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "我的观点：保持观察。" }));
  const body = document.querySelector(".detail-prose");
  expect(body?.textContent).toBe(content);
  expect(body?.children).toHaveLength(0);
  expect(screen.queryByText("纪律 null/5")).toBeNull();
});

it("rejects whitespace and retains the draft when saving fails", async () => {
  vi.mocked(reviewsApi.saveSystemReview).mockRejectedValueOnce(
    new Error("复盘保存失败"),
  );
  render(<ReviewCenter tab="reviews" flash={vi.fn()} onAsk={vi.fn()} />);
  fireEvent.click(await screen.findByRole("button", { name: "新建复盘" }));
  const input = await screen.findByRole("textbox", {
    name: "我的观点、顾虑和思考",
  });
  const save = screen.getByRole("button", {
    name: "保存复盘",
  }) as HTMLButtonElement;
  fireEvent.change(input, { target: { value: " \n " } });
  expect(save.disabled).toBe(true);
  fireEvent.submit(save.closest("form")!);
  expect(reviewsApi.saveSystemReview).not.toHaveBeenCalled();
  const content = "保留我的顾虑与思考";
  fireEvent.change(input, { target: { value: content } });
  fireEvent.click(save);
  expect((await screen.findByRole("alert")).textContent).toContain(
    "复盘保存失败",
  );
  expect(
    (
      screen.getByRole("textbox", {
        name: "我的观点、顾虑和思考",
      }) as HTMLTextAreaElement
    ).value,
  ).toBe(content);
});

it("shows every legacy field together as plain text", async () => {
  vi.mocked(reviewsApi.getSystemReviews).mockResolvedValue([legacy]);
  render(<ReviewCenter tab="reviews" flash={vi.fn()} onAsk={vi.fn()} />);
  fireEvent.click(await screen.findByRole("button", { name: /2026 Q3/ }));
  const content = document.querySelector(".detail-prose")?.textContent;
  for (const text of [
    "2026 Q3",
    "先检查依据",
    "信息不足时过早判断",
    "留意反方证据",
    "继续跟踪资料",
    "纪律评分：4/5",
    "2026-12-31",
  ]) {
    expect(content).toContain(text);
  }
});

it("shows rules without the review toolbar when the rules tab is selected", async () => {
  render(<ReviewCenter tab="rules" flash={vi.fn()} onAsk={vi.fn()} />);
  expect(await screen.findByRole("button", { name: "新建规则" })).toBeTruthy();
  expect(screen.queryByRole("button", { name: "新建复盘" })).toBeNull();
  expect(screen.queryByText("历史复盘")).toBeNull();
});

it("blocks whitespace-only investment rules and trims submitted fields", async () => {
  vi.mocked(reviewsApi.saveInvestmentRule).mockResolvedValue({
    id: "rule-1",
    category: "风险",
    statement: "单票不超10%",
    trigger: "买入前",
    rationale: "控制集中风险",
    active: true,
    revision: 1,
    createdAt: "2026-11-20T00:00:00Z",
    updatedAt: "2026-11-20T00:00:00Z",
  });
  render(<ReviewCenter tab="rules" flash={vi.fn()} onAsk={vi.fn()} />);
  fireEvent.click(await screen.findByRole("button", { name: "新建规则" }));
  fireEvent.change(screen.getByLabelText("规则内容"), {
    target: { value: "   " },
  });
  fireEvent.change(screen.getByLabelText("触发条件"), {
    target: { value: "   " },
  });
  fireEvent.change(screen.getByLabelText("依据"), {
    target: { value: "   " },
  });
  const submit = screen.getByRole("button", {
    name: "建立规则",
  }) as HTMLButtonElement;
  expect(submit.disabled).toBe(true);
  fireEvent.change(screen.getByLabelText("规则内容"), {
    target: { value: " 单票不超10% " },
  });
  fireEvent.change(screen.getByLabelText("触发条件"), {
    target: { value: " 买入前 " },
  });
  fireEvent.change(screen.getByLabelText("依据"), {
    target: { value: " 控制集中风险 " },
  });
  expect(submit.disabled).toBe(false);
  fireEvent.click(submit);
  await waitFor(() =>
    expect(reviewsApi.saveInvestmentRule).toHaveBeenCalledWith(
      expect.objectContaining({
        statement: "单票不超10%",
        trigger: "买入前",
        rationale: "控制集中风险",
      }),
    ),
  );
});

it("shows per-rule history error with retry and recovers on retry", async () => {
  vi.mocked(reviewsApi.getInvestmentRules).mockResolvedValue([
    {
      id: "rule-err",
      category: "风险",
      statement: "回撤超过阈值减仓",
      trigger: "季度复盘",
      rationale: "防止大幅亏损",
      active: true,
      revision: 2,
      createdAt: "2026-11-01T00:00:00Z",
      updatedAt: "2026-11-10T00:00:00Z",
    },
  ]);
  vi.mocked(reviewsApi.getInvestmentRuleHistory)
    .mockRejectedValueOnce(new Error("网络暂时中断"))
    .mockResolvedValueOnce([
      {
        ruleId: "rule-err",
        revision: 1,
        category: "风险",
        statement: "初始规则内容",
        trigger: "建仓前",
        rationale: "控制风险",
        active: true,
        changedAt: "2026-11-01T00:00:00Z",
      },
    ]);

  render(<ReviewCenter tab="rules" flash={vi.fn()} onAsk={vi.fn()} />);
  fireEvent.click(
    await screen.findByRole("button", { name: "回撤超过阈值减仓更多操作" }),
  );
  fireEvent.click(await screen.findByRole("menuitem", { name: "历史" }));
  expect(await screen.findByText(/网络暂时中断/)).toBeTruthy();

  fireEvent.click(screen.getByRole("button", { name: "重试" }));
  expect(await screen.findByText(/初始规则内容/)).toBeTruthy();
});
