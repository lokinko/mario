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
import { Advisor } from "./Advisor";
import * as api from "../../api";
import type {
  AnalysisPreview,
  AnalysisResult,
  StoredAnalysis,
} from "../../types";
vi.mock("../../api", () => ({
  getAnalysis: vi.fn(),
  getAnalysisHistory: vi.fn().mockResolvedValue([]),
  previewAnalysis: vi.fn(),
  runAnalysis: vi.fn(),
}));
vi.mock("./AnalysisReport", () => ({
  WebSearchStatus: () => null,
  StoredAnalysisView: ({ item }: { item: StoredAnalysis }) => (
    <p>{item.question}</p>
  ),
  StructuredReportView: () => null,
  AdviceCards: () => null,
  EvidenceOverview: () => null,
  StoredWorkflowTrace: () => null,
}));
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

it("ignores a preview that returns after the question has changed", async () => {
  let resolve!: (preview: AnalysisPreview) => void;
  vi.mocked(api.previewAnalysis).mockReturnValue(
    new Promise((r) => {
      resolve = r;
    }),
  );
  render(
    <Advisor
      model={{
        provider: "openai-compatible",
        baseUrl: "",
        model: "",
        hasApiKey: false,
      }}
      navigate={vi.fn()}
      clearRequestedAnalysis={vi.fn()}
      onCreateDecisionDraft={vi.fn()}
      requestedAnalysisId={null}
    />,
  );
  fireEvent.click(
    screen.getByRole("button", { name: "我现在适合开始投资吗？" }),
  );
  fireEvent.click(screen.getByRole("button", { name: "提问" }));
  await waitFor(() => expect(api.previewAnalysis).toHaveBeenCalled());
  fireEvent.change(
    screen.getByRole("textbox", { name: "这次希望解决什么问题？" }),
    { target: { value: "另一个问题" } },
  );
  // An obsolete result must not be rendered or used for sending, even before parsing its UI fields.
  await act(async () => {
    resolve({ contextRevision: "obsolete" } as AnalysisPreview);
  });
  expect(screen.queryByRole("button", { name: "确认发送" })).toBeNull();
  expect(screen.getByDisplayValue("另一个问题")).toBeTruthy();
  expect(api.runAnalysis).not.toHaveBeenCalled();
});
it("keeps the latest selected analysis when requests finish out of order", async () => {
  const pending: Record<string, (value: StoredAnalysis) => void> = {};
  vi.mocked(api.getAnalysis).mockImplementation(
    (id) =>
      new Promise((resolve) => {
        pending[id] = resolve;
      }),
  );
  const props = {
    model: {
      provider: "openai-compatible" as const,
      baseUrl: "",
      model: "",
      hasApiKey: false,
    },
    navigate: vi.fn(),
    clearRequestedAnalysis: vi.fn(),
    onCreateDecisionDraft: vi.fn(),
  };
  const view = render(<Advisor {...props} requestedAnalysisId="a" />);
  view.rerender(<Advisor {...props} requestedAnalysisId="b" />);
  await act(async () => {
    pending.b({ question: "最新分析" } as StoredAnalysis);
  });
  await act(async () => {
    pending.a({ question: "过期分析" } as StoredAnalysis);
  });
  expect(screen.getByText("最新分析")).toBeTruthy();
  expect(screen.queryByText("过期分析")).toBeNull();
  expect(props.clearRequestedAnalysis).toHaveBeenCalledTimes(1);
});

const preview = {
  contextRevision: "revision-1",
  payloadBytes: 100,
  model: "test-model",
  provider: "test",
  workflow: "quick",
  groups: [],
  localOnly: [],
  payload: {},
  evidenceCandidates: [],
  memoryCandidates: [],
  systemPolicy: "",
  memoryPolicy: "",
} as unknown as AnalysisPreview;
const result = {
  id: "answer-1",
  answer: "先明确你的资金使用时间。",
  stages: [],
  disclaimer: "仅供参考",
  transparency: { model: "test-model", modelCalls: 1, totalLatencyMs: 100 },
  workflowTrace: {
    memoryItems: [],
    alternatives: [],
    calls: [],
    evidenceCatalog: [],
  },
} as unknown as AnalysisResult;
const props = {
  model: {
    provider: "openai-compatible" as const,
    baseUrl: "",
    model: "test-model",
    hasApiKey: true,
  },
  navigate: vi.fn(),
  clearRequestedAnalysis: vi.fn(),
  onCreateDecisionDraft: vi.fn(),
  requestedAnalysisId: null,
};
it("requires confirmation, keeps the answer while composing, and sends follow-up context only when selected", async () => {
  vi.mocked(api.previewAnalysis).mockResolvedValue(preview);
  vi.mocked(api.runAnalysis).mockResolvedValue(result);
  render(<Advisor {...props} />);
  expect(
    (screen.getByRole("button", { name: "提问" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
  fireEvent.click(
    screen.getByRole("button", { name: "我现在适合开始投资吗？" }),
  );
  fireEvent.click(screen.getByRole("button", { name: "提问" }));
  const confirm = await screen.findByRole("button", { name: "确认发送" });
  expect(api.runAnalysis).not.toHaveBeenCalled();
  fireEvent.click(confirm);
  await screen.findByText(result.answer);
  const input = screen.getByRole("textbox", { name: "继续聊聊" });
  fireEvent.change(input, { target: { value: "三年内需要用钱" } });
  expect(screen.getByText(result.answer)).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "提问" }));
  await screen.findByRole("button", { name: "确认发送" });
  expect(vi.mocked(api.previewAnalysis).mock.lastCall?.[0].question).toContain(
    result.answer,
  );
  expect(vi.mocked(api.previewAnalysis).mock.lastCall?.[0].question).toContain(
    "三年内需要用钱",
  );
  expect(vi.mocked(api.previewAnalysis).mock.lastCall?.[0].userMessage).toBe(
    "三年内需要用钱",
  );
  fireEvent.click(screen.getByRole("button", { name: "换个话题" }));
  expect(screen.queryByRole("button", { name: "确认发送" })).toBeNull();
  fireEvent.change(input, { target: { value: "一个新问题" } });
  fireEvent.click(screen.getByRole("button", { name: "提问" }));
  await screen.findByRole("button", { name: "确认发送" });
  expect(vi.mocked(api.previewAnalysis).mock.lastCall?.[0].question).toBe(
    "一个新问题",
  );
});
it("accepts a queued question when returning from supporting pages", async () => {
  const view = render(<Advisor {...props} active={false} />);
  window.sessionStorage.setItem(
    "mario.advisorQuestion",
    "从资产概览带来的问题",
  );
  view.rerender(<Advisor {...props} active />);
  expect(await screen.findByDisplayValue("从资产概览带来的问题")).toBeTruthy();
  expect(window.sessionStorage.getItem("mario.advisorQuestion")).toBeNull();
});

it("authorizes native search in preview without running the model before confirmation", async () => {
  vi.mocked(api.previewAnalysis).mockResolvedValue(preview);
  render(<Advisor {...props} />);
  fireEvent.click(
    screen.getByRole("button", { name: "我现在适合开始投资吗？" }),
  );
  fireEvent.click(screen.getByRole("button", { name: "提问" }));
  await screen.findByRole("button", { name: "确认发送" });
  expect(api.previewAnalysis).toHaveBeenCalledWith(
    expect.objectContaining({ webSearch: true }),
  );
  expect(screen.getByText(/确认后将启用供应商网页搜索/)).toBeTruthy();
  expect(api.runAnalysis).not.toHaveBeenCalled();
});
