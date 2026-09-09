// @vitest-environment jsdom
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { DecisionJournal, emptyDecision } from "./DecisionJournal";
import * as api from "../../api";
import type { DecisionReview } from "../../types";

vi.mock("../../api", () => ({
  getDecisions: vi.fn(),
  getInvestmentRules: vi.fn(),
  saveDecision: vi.fn(),
  saveDecisionReview: vi.fn(),
}));
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

it("counts only binary resolved outcomes in the descriptive probability error", async () => {
  const outcomes: DecisionReview["thesisStatus"][] = [
    "成立",
    "失效",
    "部分成立",
    "尚不明确",
  ];
  vi.mocked(api.getInvestmentRules).mockResolvedValue([]);
  vi.mocked(api.getDecisions).mockResolvedValue(
    outcomes.map((thesisStatus, index) => ({
      ...emptyDecision,
      id: String(index),
      createdAt: "2026-09-09",
      ruleChecks: [],
      confidencePct: 50,
      review: {
        thesisStatus,
        outcomeSummary: "记录结果",
        processRating: 3,
        lessons: "继续观察",
      },
    })),
  );
  render(
    <DecisionJournal
      flash={vi.fn()}
      seed={null}
      clearSeed={vi.fn()}
      onOpenAnalysis={vi.fn()}
    />,
  );
  expect(await screen.findByText("0.250")).toBeTruthy();
  expect(screen.getByText(/2 个明确成立\/失效样本/)).toBeTruthy();
  expect(screen.queryByText("简化校准分")).toBeNull();
});

it("preserves edits to an AI draft when rules arrive later", async () => {
  vi.mocked(api.getDecisions).mockResolvedValue([]);
  let resolve!: (rules: []) => void;
  vi.mocked(api.getInvestmentRules).mockReturnValue(
    new Promise((r) => {
      resolve = r;
    }),
  );
  render(
    <DecisionJournal
      flash={vi.fn()}
      seed={{ ...emptyDecision, thesis: "原始草稿" }}
      clearSeed={vi.fn()}
      onOpenAnalysis={vi.fn()}
    />,
  );
  const thesis = await screen.findByDisplayValue("原始草稿");
  fireEvent.change(thesis, { target: { value: "用户独立补充的判断" } });
  await act(async () => {
    resolve([]);
  });
  expect(screen.getByDisplayValue("用户独立补充的判断")).toBeTruthy();
  expect(screen.queryByDisplayValue("原始草稿")).toBeNull();
});

it("saves a complete manual decision without an account or model", async () => {
  vi.mocked(api.getDecisions).mockResolvedValue([]);
  vi.mocked(api.getInvestmentRules).mockResolvedValue([]);
  vi.mocked(api.saveDecision).mockResolvedValue(undefined);
  const draft = {
    ...emptyDecision,
    assetName: "观察组合",
    thesis: "可验证假设",
    counterThesis: "最强反证",
    invalidation: "条件不成立",
    reviewDate: "2026-12-01",
  };
  render(
    <DecisionJournal
      flash={vi.fn()}
      seed={draft}
      clearSeed={vi.fn()}
      onOpenAnalysis={vi.fn()}
    />,
  );
  await waitFor(() =>
    expect(
      (
        screen.getByRole("button", {
          name: "冻结决策快照",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(false),
  );
  fireEvent.click(screen.getByRole("button", { name: "冻结决策快照" }));
  await waitFor(() => expect(api.saveDecision).toHaveBeenCalledWith(draft));
});

it("does not treat failed rule loading as an empty rule set", async () => {
  vi.mocked(api.getDecisions).mockResolvedValue([]);
  vi.mocked(api.getInvestmentRules)
    .mockRejectedValueOnce(new Error("规则读取失败"))
    .mockResolvedValue([]);
  render(
    <DecisionJournal
      flash={vi.fn()}
      seed={{
        ...emptyDecision,
        assetName: "完整草稿",
        thesis: "假设",
        counterThesis: "反证",
        invalidation: "条件",
        reviewDate: "2026-12-01",
      }}
      clearSeed={vi.fn()}
      onOpenAnalysis={vi.fn()}
    />,
  );
  expect((await screen.findByRole("alert")).textContent).toContain(
    "规则读取失败",
  );
  const save = screen.getByRole("button", {
    name: "冻结决策快照",
  }) as HTMLButtonElement;
  expect(save.disabled).toBe(true);
  fireEvent.click(save);
  expect(api.saveDecision).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "重新加载决策与规则" }));
  await waitFor(() => expect(save.disabled).toBe(false));
  expect(screen.getByDisplayValue("完整草稿")).toBeTruthy();
});
