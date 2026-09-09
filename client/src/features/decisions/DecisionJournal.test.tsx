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
  await waitFor(() => expect(api.getInvestmentRules).toHaveBeenCalled());
  fireEvent.click(screen.getByRole("button", { name: "冻结决策快照" }));
  await waitFor(() => expect(api.saveDecision).toHaveBeenCalledWith(draft));
});
