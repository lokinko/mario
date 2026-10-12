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
import * as api from "../../api";
import type { SystemReviewRecord } from "../../types";

vi.mock("../../api", () => ({
  getInvestmentRules: vi.fn(),
  getInvestmentRuleHistory: vi.fn(),
  getSystemReviews: vi.fn(),
  saveSystemReview: vi.fn(),
  saveInvestmentRule: vi.fn(),
  updateInvestmentRule: vi.fn(),
}));
beforeEach(() => {
  vi.mocked(api.getInvestmentRules).mockResolvedValue([]);
  vi.mocked(api.getSystemReviews).mockResolvedValue([]);
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
  vi.mocked(api.saveSystemReview).mockResolvedValue(stored);
  const flash = vi.fn();
  render(<ReviewCenter flash={flash} navigate={vi.fn()} />);
  const input = await screen.findByRole("textbox", {
    name: "我的观点、顾虑和思考",
  });
  const form = input.closest("form")!;
  expect(form.querySelectorAll("textarea")).toHaveLength(1);
  expect(form.querySelectorAll("input, select")).toHaveLength(0);
  expect(
    document.querySelector<HTMLDetailsElement>(".rule-workbench")?.open,
  ).toBe(false);
  expect(screen.queryByLabelText("下次复盘日")).toBeNull();
  fireEvent.change(input, { target: { value: content } });
  vi.mocked(api.getSystemReviews).mockResolvedValue([stored]);
  fireEvent.click(screen.getByRole("button", { name: "保存复盘" }));
  await waitFor(() =>
    expect(api.saveSystemReview).toHaveBeenCalledWith({ content }),
  );
  await waitFor(() => expect(flash).toHaveBeenCalledWith("复盘记录已保存"));
  expect(
    (
      screen.getByRole("textbox", {
        name: "我的观点、顾虑和思考",
      }) as HTMLTextAreaElement
    ).value,
  ).toBe("");
  const body = document.querySelector(".review-content");
  expect(body?.textContent).toBe(content);
  expect(body?.children).toHaveLength(0);
  expect(screen.queryByText("纪律 null/5")).toBeNull();
});

it("rejects whitespace and retains the draft when saving fails", async () => {
  vi.mocked(api.saveSystemReview).mockRejectedValueOnce(
    new Error("复盘保存失败"),
  );
  render(<ReviewCenter flash={vi.fn()} navigate={vi.fn()} />);
  const input = await screen.findByRole("textbox", {
    name: "我的观点、顾虑和思考",
  });
  const save = screen.getByRole("button", {
    name: "保存复盘",
  }) as HTMLButtonElement;
  fireEvent.change(input, { target: { value: " \n " } });
  expect(save.disabled).toBe(true);
  fireEvent.submit(save.closest("form")!);
  expect(api.saveSystemReview).not.toHaveBeenCalled();
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
  vi.mocked(api.getSystemReviews).mockResolvedValue([legacy]);
  render(<ReviewCenter flash={vi.fn()} navigate={vi.fn()} />);
  await screen.findByRole("textbox", { name: "我的观点、顾虑和思考" });
  const content = document.querySelector(".review-content")?.textContent;
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
