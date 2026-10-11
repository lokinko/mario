// @vitest-environment jsdom
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { Advisor } from "./Advisor";
import * as api from "../../api";
import type {
  AnalysisPreview,
  AnalysisResult,
  StoredAnalysis,
} from "../../types";
import { analysisSettings } from "../../test/analysisSettings";

vi.mock("../../api", () => ({
  getAnalysis: vi.fn(),
  getAnalysisHistory: vi.fn().mockResolvedValue([]),
  getAnalysisSettings: vi.fn(),
  previewAnalysis: vi.fn(),
  runAnalysis: vi.fn(),
}));

const preview = { contextRevision: "revision-1" } as AnalysisPreview;
const result = {
  id: "answer-1",
  answer: "先明确你的资金使用时间。",
  workflowTrace: { evidenceCatalog: [] },
} as unknown as AnalysisResult;
const props = {
  model: {
    provider: "openai-responses" as const,
    baseUrl: "",
    model: "test-model",
    hasApiKey: true,
  },
  navigate: vi.fn(),
  clearRequestedAnalysis: vi.fn(),
  onCreateDecisionDraft: vi.fn(),
  requestedAnalysisId: null,
};

beforeEach(() => {
  vi.mocked(api.getAnalysisSettings).mockResolvedValue(analysisSettings);
  vi.mocked(api.previewAnalysis).mockResolvedValue(preview);
  vi.mocked(api.runAnalysis).mockResolvedValue(result);
  vi.mocked(api.getAnalysisHistory).mockResolvedValue([]);
});
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  window.sessionStorage.clear();
});
const ask = (question: string) => {
  fireEvent.change(screen.getByRole("textbox", { name: "输入问题" }), {
    target: { value: question },
  });
  fireEvent.click(screen.getByRole("button", { name: "发送" }));
};

it("sends once using saved defaults and the frozen revision without a confirmation step", async () => {
  const settings = {
    ...analysisSettings,
    useMemory: false,
    webSearch: false,
    contextSelection: {
      ...analysisSettings.contextSelection,
      includeProfile: false,
      includeEvidence: false,
    },
  };
  vi.mocked(api.getAnalysisSettings).mockResolvedValue(settings);
  render(<Advisor {...props} />);
  ask("我的持仓有什么风险？");
  await screen.findByText(result.answer);
  expect(api.previewAnalysis).toHaveBeenCalledWith({
    ...settings,
    userMessage: "我的持仓有什么风险？",
    question: "我的持仓有什么风险？",
    dailyAssetRange: undefined,
    excludedMemoryIds: [],
  });
  expect(api.runAnalysis).toHaveBeenCalledWith({
    ...vi.mocked(api.previewAnalysis).mock.calls[0][0],
    previewRevision: "revision-1",
  });
  expect(api.runAnalysis).toHaveBeenCalledTimes(1);
  for (const label of [
    "确认发送",
    "隐私与资料范围",
    "分析过程与使用的资料",
    "查看完整分析与方案比较",
  ])
    expect(screen.queryByText(label)).toBeNull();
  expect(screen.queryByRole("checkbox")).toBeNull();
});

it("prevents duplicate sends and preserves the draft if preparing data fails", async () => {
  let reject!: (error: Error) => void;
  vi.mocked(api.previewAnalysis).mockReturnValue(
    new Promise((_, fail) => {
      reject = fail;
    }),
  );
  render(<Advisor {...props} />);
  ask("待回答的问题");
  await waitFor(() => expect(api.previewAnalysis).toHaveBeenCalledTimes(1));
  const input = screen.getByRole("textbox") as HTMLTextAreaElement;
  expect(input.disabled).toBe(true);
  fireEvent.keyDown(input, { key: "Enter", ctrlKey: true });
  await act(async () => {
    reject(new Error("资料发生变化，请重试"));
  });
  expect((await screen.findByRole("alert")).textContent).toContain(
    "资料发生变化",
  );
  expect(input.value).toBe("待回答的问题");
  expect(input.disabled).toBe(false);
  expect(api.runAnalysis).not.toHaveBeenCalled();
});

it("does not send if saved settings cannot be read", async () => {
  vi.mocked(api.getAnalysisSettings).mockRejectedValue(
    new Error("预设读取失败"),
  );
  render(<Advisor {...props} />);
  ask("问题");
  expect((await screen.findByRole("alert")).textContent).toContain(
    "预设读取失败",
  );
  expect(api.previewAnalysis).not.toHaveBeenCalled();
  expect(api.runAnalysis).not.toHaveBeenCalled();
});

it("keeps the latest selected historical report when requests finish out of order", async () => {
  const pending: Record<string, (value: StoredAnalysis) => void> = {};
  vi.mocked(api.getAnalysis).mockImplementation(
    (id) =>
      new Promise((resolve) => {
        pending[id] = resolve;
      }),
  );
  const view = render(<Advisor {...props} requestedAnalysisId="a" />);
  view.rerender(<Advisor {...props} requestedAnalysisId="b" />);
  await act(async () => {
    pending.b({
      id: "b",
      question: "最新问题",
      answer: "最新 report",
    } as StoredAnalysis);
  });
  await act(async () => {
    pending.a({
      id: "a",
      question: "过期问题",
      answer: "过期 report",
    } as StoredAnalysis);
  });
  expect(screen.getByText("最新 report")).toBeTruthy();
  expect(screen.queryByText("过期 report")).toBeNull();
  expect(props.clearRequestedAnalysis).toHaveBeenCalledTimes(1);
});

it("disables native search for Chat Completions even when enabled in saved defaults", async () => {
  render(
    <Advisor {...props} model={{ ...props.model, provider: "openai-chat" }} />,
  );
  ask("问题");
  await screen.findByText(result.answer);
  expect(api.previewAnalysis).toHaveBeenCalledWith(
    expect.objectContaining({ webSearch: false }),
  );
  expect(api.runAnalysis).toHaveBeenCalledWith(
    expect.objectContaining({ webSearch: false }),
  );
});

it("automatically carries previous context and rereads saved defaults, then clears context for a new conversation", async () => {
  render(<Advisor {...props} />);
  ask("第一次提问");
  await screen.findByText(result.answer);
  vi.mocked(api.getAnalysisSettings).mockResolvedValue({
    ...analysisSettings,
    useMemory: false,
  });
  vi.mocked(api.runAnalysis).mockResolvedValue({
    ...result,
    id: "answer-2",
    answer: "第二次 report",
  });
  ask("三年内需要用钱");
  await screen.findByText("第二次 report");
  expect(screen.getByText(result.answer)).toBeTruthy();
  const followUp = vi.mocked(api.previewAnalysis).mock.lastCall?.[0];
  expect(followUp?.question).toContain(result.answer);
  expect(followUp?.question).toContain("三年内需要用钱");
  expect(followUp?.userMessage).toBe("三年内需要用钱");
  expect(followUp?.useMemory).toBe(false);
  fireEvent.click(screen.getByRole("button", { name: "新对话" }));
  expect(screen.queryByText(result.answer)).toBeNull();
  ask("一个新问题");
  await screen.findByText("第二次 report");
  expect(vi.mocked(api.previewAnalysis).mock.lastCall?.[0].question).toBe(
    "一个新问题",
  );
});

it("reopens a historical answer in the same report view and allows continuing that conversation", async () => {
  vi.mocked(api.getAnalysisHistory).mockResolvedValue([
    { id: "saved", question: "旧问题", createdAt: "2026-10-10T10:00:00Z" },
  ]);
  vi.mocked(api.getAnalysis).mockResolvedValue({
    id: "saved",
    question: "旧问题",
    answer: "历史 report",
    createdAt: "2026-10-10T10:00:00Z",
  });
  render(<Advisor {...props} />);
  fireEvent.click(await screen.findByRole("button", { name: /旧问题/ }));
  await screen.findByText("历史 report");
  ask("继续旧问题");
  await screen.findByText(result.answer);
  expect(vi.mocked(api.previewAnalysis).mock.lastCall?.[0].question).toContain(
    "历史 report",
  );
});

it("accepts a queued question and its asset range when returning from supporting pages", async () => {
  const view = render(<Advisor {...props} active={false} />);
  window.sessionStorage.setItem(
    "mario.advisorQuestion",
    "从资产概览带来的问题",
  );
  window.sessionStorage.setItem(
    "mario.dailyAssetRange",
    JSON.stringify({ from: "2026-10-01", to: "2026-10-10" }),
  );
  view.rerender(<Advisor {...props} active />);
  expect(await screen.findByDisplayValue("从资产概览带来的问题")).toBeTruthy();
  expect(window.sessionStorage.getItem("mario.advisorQuestion")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "发送" }));
  await screen.findByText(result.answer);
  expect(api.previewAnalysis).toHaveBeenCalledWith(
    expect.objectContaining({
      dailyAssetRange: { from: "2026-10-01", to: "2026-10-10" },
    }),
  );
});
