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

it("records a manual review without an account or model", async () => {
  vi.mocked(api.getInvestmentRules).mockResolvedValue([]);
  vi.mocked(api.getDecisions).mockResolvedValue([
    {
      ...emptyDecision,
      id: "manual",
      assetName: "人工判断",
      ruleChecks: [],
      createdAt: "2026-09-09",
      reviewDate: "2026-09-09",
    },
  ]);
  vi.mocked(api.saveDecisionReview).mockResolvedValue(undefined);
  render(
    <DecisionJournal
      flash={vi.fn()}
      seed={null}
      clearSeed={vi.fn()}
      onOpenAnalysis={vi.fn()}
    />,
  );
  fireEvent.click(await screen.findByRole("button", { name: "开始复盘" }));
  fireEvent.change(screen.getByRole("textbox", { name: "复盘内容" }), {
    target: { value: "记录可验证事实\n补足反方证据" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存复盘" }));
  await waitFor(() =>
    expect(api.saveDecisionReview).toHaveBeenCalledWith(
      "manual",
      expect.objectContaining({
        content: "记录可验证事实\n补足反方证据",
      }),
    ),
  );
});

it("saves only the user's complete text and clears the draft after success", async () => {
  vi.mocked(api.getDecisions).mockResolvedValue([]);
  vi.mocked(api.saveDecision).mockResolvedValue(undefined);
  const flash = vi.fn();
  const clearSeed = vi.fn();
  render(
    <DecisionJournal
      flash={flash}
      seed={null}
      clearSeed={clearSeed}
      onOpenAnalysis={vi.fn()}
    />,
  );
  const input = screen.getByRole("textbox", { name: "我的观点、顾虑和思考" });
  expect(screen.getAllByRole("textbox")).toHaveLength(1);
  expect(screen.queryByRole("spinbutton")).toBeNull();
  const content =
    "我的观点：先观察。\n\n顾虑：资料还不完整。\n思考：等下一期报告，保留自己的判断。";
  fireEvent.change(input, { target: { value: content } });
  fireEvent.click(screen.getByRole("button", { name: "保存决策" }));
  await waitFor(() =>
    expect(api.saveDecision).toHaveBeenCalledWith({ content }),
  );
  await waitFor(() => expect((input as HTMLTextAreaElement).value).toBe(""));
  expect(flash).toHaveBeenCalledWith("决策记录已保存");
  expect(clearSeed).toHaveBeenCalledOnce();
  expect(api.getInvestmentRules).not.toHaveBeenCalled();
});

it("rejects blank content even on form submission", async () => {
  vi.mocked(api.getDecisions).mockResolvedValue([]);
  render(
    <DecisionJournal
      flash={vi.fn()}
      seed={null}
      clearSeed={vi.fn()}
      onOpenAnalysis={vi.fn()}
    />,
  );
  fireEvent.change(
    screen.getByRole("textbox", { name: "我的观点、顾虑和思考" }),
    { target: { value: " \n  " } },
  );
  const save = screen.getByRole("button", {
    name: "保存决策",
  }) as HTMLButtonElement;
  expect(save.disabled).toBe(true);
  fireEvent.submit(save.closest("form")!);
  expect(api.saveDecision).not.toHaveBeenCalled();
});

it("preserves text after a save failure and permits retry", async () => {
  vi.mocked(api.getDecisions).mockResolvedValue([]);
  vi.mocked(api.saveDecision)
    .mockRejectedValueOnce(new Error("保存失败"))
    .mockResolvedValue(undefined);
  const clearSeed = vi.fn();
  render(
    <DecisionJournal
      flash={vi.fn()}
      seed={null}
      clearSeed={clearSeed}
      onOpenAnalysis={vi.fn()}
    />,
  );
  const input = screen.getByRole("textbox", { name: "我的观点、顾虑和思考" });
  fireEvent.change(input, { target: { value: "我需要保留的观点与顾虑" } });
  fireEvent.click(screen.getByRole("button", { name: "保存决策" }));
  expect((await screen.findByRole("alert")).textContent).toContain("保存失败");
  expect((input as HTMLTextAreaElement).value).toBe("我需要保留的观点与顾虑");
  expect(clearSeed).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "保存决策" }));
  await waitFor(() => expect(clearSeed).toHaveBeenCalledOnce());
});

it("preserves edits to an AI draft when history arrives later and saves its provenance", async () => {
  let resolve!: (records: []) => void;
  vi.mocked(api.getDecisions)
    .mockReturnValueOnce(
      new Promise((r) => {
        resolve = r;
      }),
    )
    .mockResolvedValue([]);
  vi.mocked(api.saveDecision).mockResolvedValue(undefined);
  render(
    <DecisionJournal
      flash={vi.fn()}
      seed={{
        ...emptyDecision,
        sourceAnalysisId: "analysis-1",
        sourceActionIndex: 0,
        thesis: "原始草稿",
        counterThesis: "资料局限",
        invalidation: "新的反证",
      }}
      clearSeed={vi.fn()}
      onOpenAnalysis={vi.fn()}
    />,
  );
  const input = screen.getByRole("textbox", {
    name: "我的观点、顾虑和思考",
  }) as HTMLTextAreaElement;
  expect(input.value).toContain("原始草稿");
  expect(input.value).toContain("资料局限");
  expect(input.value).toContain("新的反证");
  fireEvent.change(input, { target: { value: "用户独立补充的判断" } });
  await act(async () => {
    resolve([]);
  });
  expect(input.value).toBe("用户独立补充的判断");
  fireEvent.click(screen.getByRole("button", { name: "保存决策" }));
  await waitFor(() =>
    expect(api.saveDecision).toHaveBeenCalledWith({
      content: "用户独立补充的判断",
      sourceAnalysisId: "analysis-1",
      sourceActionIndex: 0,
    }),
  );
});

it("shows complete text and all legacy viewpoints without inventing numeric values for text records", async () => {
  const content = "我暂时不加仓。\n\n顾虑是用钱时间尚不确定。";
  vi.mocked(api.getDecisions).mockResolvedValue([
    {
      ...emptyDecision,
      id: "text",
      ruleChecks: [],
      content,
      assetName: "我暂时不加仓。",
      thesis: content,
      createdAt: "2026-10-11",
    },
    {
      ...emptyDecision,
      id: "legacy",
      ruleChecks: [],
      assetName: "旧基金",
      thesis: "旧观点",
      counterThesis: "旧顾虑",
      invalidation: "旧证伪条件",
      createdAt: "2026-09-09",
    },
  ]);
  render(
    <DecisionJournal
      flash={vi.fn()}
      seed={null}
      clearSeed={vi.fn()}
      onOpenAnalysis={vi.fn()}
    />,
  );
  await screen.findByText("我暂时不加仓。");
  expect(document.querySelector(".decision-content")?.textContent).toBe(
    content,
  );
  const legacy = document.querySelectorAll(".decision-content")[1]?.textContent;
  expect(legacy).toContain("旧观点");
  expect(legacy).toContain("旧顾虑");
  expect(legacy).toContain("旧证伪条件");
  expect(screen.queryByText("概率误差")).toBeNull();
});

it("allows writing a decision when loading history fails", async () => {
  vi.mocked(api.getDecisions)
    .mockRejectedValueOnce(new Error("读取失败"))
    .mockResolvedValue([]);
  vi.mocked(api.saveDecision).mockResolvedValue(undefined);
  render(
    <DecisionJournal
      flash={vi.fn()}
      seed={null}
      clearSeed={vi.fn()}
      onOpenAnalysis={vi.fn()}
    />,
  );
  await screen.findByRole("alert");
  fireEvent.change(
    screen.getByRole("textbox", { name: "我的观点、顾虑和思考" }),
    { target: { value: "新的独立思考" } },
  );
  fireEvent.click(screen.getByRole("button", { name: "保存决策" }));
  await waitFor(() =>
    expect(api.saveDecision).toHaveBeenCalledWith({ content: "新的独立思考" }),
  );
});

it("loads all legacy review fields into one editable paragraph and retains them on failure", async () => {
  vi.mocked(api.getDecisions).mockResolvedValue([
    {
      ...emptyDecision,
      id: "legacy-review",
      assetName: "旧决策",
      ruleChecks: [],
      createdAt: "2026-09-09",
      review: {
        outcomeSummary: "价格回落",
        lessons: "不要忽略顾虑",
        thesisStatus: "部分成立",
        actualReturnPct: 0,
        processRating: 4,
      },
    },
  ]);
  vi.mocked(api.saveDecisionReview).mockRejectedValue(
    new Error("复盘保存失败"),
  );
  render(
    <DecisionJournal
      flash={vi.fn()}
      seed={null}
      clearSeed={vi.fn()}
      onOpenAnalysis={vi.fn()}
    />,
  );
  fireEvent.click(await screen.findByRole("button", { name: "更新复盘" }));
  const input = screen.getByRole("textbox", {
    name: "复盘内容",
  }) as HTMLTextAreaElement;
  for (const text of [
    "价格回落",
    "不要忽略顾虑",
    "部分成立",
    "实际收益：0%",
    "过程评分：4/5",
  ]) {
    expect(input.value).toContain(text);
  }
  fireEvent.change(input, { target: { value: " \n " } });
  const save = screen.getByRole("button", {
    name: "保存复盘",
  }) as HTMLButtonElement;
  expect(save.disabled).toBe(true);
  fireEvent.submit(save.closest("form")!);
  expect(api.saveDecisionReview).not.toHaveBeenCalled();
  const content = "保留我的完整思考\n\n仍有顾虑";
  fireEvent.change(input, { target: { value: content } });
  fireEvent.click(save);
  expect((await screen.findByRole("alert")).textContent).toContain(
    "复盘保存失败",
  );
  expect(input.value).toBe(content);
});
