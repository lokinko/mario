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
import * as researchApi from "../../api/research";
import * as settingsApi from "../../api/settings";
import type {
  AnalysisPreview,
  AnalysisResult,
  StoredAnalysis,
} from "../../types";
import { analysisSettings } from "../../test/analysisSettings";

vi.mock("../../api/research", () => ({
  getAnalysis: vi.fn(),
  getAnalysisHistory: vi.fn().mockResolvedValue([]),
  previewAnalysis: vi.fn(),
  runAnalysis: vi.fn(),
}));
vi.mock("../../api/settings", () => ({
  getAnalysisSettings: vi.fn(),
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
  onOpenSettings: vi.fn(),
};

beforeEach(() => {
  vi.mocked(settingsApi.getAnalysisSettings).mockResolvedValue(
    analysisSettings,
  );
  vi.mocked(researchApi.previewAnalysis).mockResolvedValue(preview);
  vi.mocked(researchApi.runAnalysis).mockResolvedValue(result);
  vi.mocked(researchApi.getAnalysisHistory).mockResolvedValue([]);
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

it("fills a starter question and focuses the composer without sending an analysis", async () => {
  render(<Advisor {...props} />);
  const prompt = "帮我梳理当前持仓的主要风险";
  fireEvent.click(screen.getByRole("button", { name: prompt }));
  const input = screen.getByRole("textbox", {
    name: "输入问题",
  }) as HTMLTextAreaElement;
  expect(input.value).toBe(prompt);
  expect(document.activeElement).toBe(input);
  expect(researchApi.previewAnalysis).not.toHaveBeenCalled();
  expect(researchApi.runAnalysis).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "发送" }));
  await screen.findByText(result.answer);
  expect(screen.queryByLabelText("开始问答")).toBeNull();
});

it("shows streamed text before completion and replaces it with the saved report", async () => {
  let finish!: (result: AnalysisResult) => void;
  vi.mocked(researchApi.runAnalysis).mockImplementation(
    (_request, onUpdate) => {
      onUpdate?.({ type: "status", message: "正在核对证据…" });
      onUpdate?.({ type: "draft", answer: "逐步显示的中文回复" });
      return new Promise((resolve) => {
        finish = resolve;
      });
    },
  );
  render(<Advisor {...props} />);
  ask("流式问题");
  await screen.findByText("逐步显示的中文回复");
  expect(screen.queryByText(result.answer)).toBeNull();
  expect(
    screen.getByRole("button", { name: "停止生成" }).hasAttribute("disabled"),
  ).toBe(false);
  await act(async () => {
    finish(result);
  });
  await screen.findByText(result.answer);
  expect(screen.queryByText("逐步显示的中文回复")).toBeNull();
  expect((screen.getByRole("textbox") as HTMLTextAreaElement).value).toBe("");
});

it("preserves an interrupted draft and original question for retry", async () => {
  vi.mocked(researchApi.runAnalysis).mockImplementation(
    async (_request, onUpdate) => {
      onUpdate?.({ type: "draft", answer: "中断前的部分回复" });
      throw new Error("回复连接中断");
    },
  );
  render(<Advisor {...props} />);
  ask("保留问题");
  await screen.findByRole("alert");
  expect(screen.getByText("中断前的部分回复")).toBeTruthy();
  expect(screen.getByText("mario · 回复未完成")).toBeTruthy();
  expect((screen.getByRole("textbox") as HTMLTextAreaElement).value).toBe(
    "保留问题",
  );
});

it("cancels generation on unmount and discards later draft updates", async () => {
  let signal: AbortSignal | undefined;
  vi.mocked(researchApi.runAnalysis).mockImplementation(
    (_request, _onUpdate, nextSignal) => {
      signal = nextSignal;
      return new Promise(() => {});
    },
  );
  const view = render(<Advisor {...props} />);
  ask("流式问题");
  await waitFor(() => expect(signal).toBeDefined());
  view.unmount();
  expect(signal?.aborted).toBe(true);
});

it("stops the stream, retains the draft and ignores late updates or completion", async () => {
  let finish!: (result: AnalysisResult) => void;
  let update!: NonNullable<Parameters<typeof researchApi.runAnalysis>[1]>;
  let signal!: AbortSignal;
  vi.mocked(researchApi.runAnalysis).mockImplementation(
    (_request, onUpdate, nextSignal) => {
      update = onUpdate!;
      signal = nextSignal!;
      update({ type: "draft", answer: "停止前的部分回复" });
      return new Promise((resolve) => {
        finish = resolve;
      });
    },
  );
  render(<Advisor {...props} />);
  ask("待中止的问题");
  await screen.findByText("停止前的部分回复");
  fireEvent.click(screen.getByRole("button", { name: "停止生成" }));
  expect(signal.aborted).toBe(true);
  expect(screen.getByText("mario · 已停止生成")).toBeTruthy();
  expect(screen.queryByRole("alert")).toBeNull();
  const input = screen.getByRole("textbox") as HTMLTextAreaElement;
  expect(input.value).toBe("待中止的问题");
  expect(input.disabled).toBe(false);
  await act(async () => {
    update({ type: "draft", answer: "停止后到达的回复" });
    finish(result);
  });
  expect(screen.getByText("停止前的部分回复")).toBeTruthy();
  expect(screen.queryByText("停止后到达的回复")).toBeNull();
  expect(screen.queryByText(result.answer)).toBeNull();
});

it("stops while preparing data without starting a model request", async () => {
  let prepare!: (settings: typeof analysisSettings) => void;
  vi.mocked(settingsApi.getAnalysisSettings).mockReturnValue(
    new Promise((resolve) => {
      prepare = resolve;
    }),
  );
  render(<Advisor {...props} />);
  ask("准备中的问题");
  fireEvent.click(screen.getByRole("button", { name: "停止生成" }));
  await act(async () => {
    prepare(analysisSettings);
  });
  expect(researchApi.previewAnalysis).not.toHaveBeenCalled();
  expect(researchApi.runAnalysis).not.toHaveBeenCalled();
  expect((screen.getByRole("textbox") as HTMLTextAreaElement).disabled).toBe(
    false,
  );
});

it("does not let a stopped request unlock or overwrite a new generation", async () => {
  let firstFinish!: (result: AnalysisResult) => void;
  let secondFinish!: (result: AnalysisResult) => void;
  vi.mocked(researchApi.runAnalysis)
    .mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          firstFinish = resolve;
        }),
    )
    .mockImplementationOnce((_request, onUpdate) => {
      onUpdate?.({ type: "draft", answer: "新请求的回复" });
      return new Promise((resolve) => {
        secondFinish = resolve;
      });
    });
  render(<Advisor {...props} />);
  ask("旧问题");
  await waitFor(() => expect(researchApi.runAnalysis).toHaveBeenCalledTimes(1));
  fireEvent.click(screen.getByRole("button", { name: "停止生成" }));
  ask("新问题");
  await screen.findByText("新请求的回复");
  await act(async () => {
    firstFinish(result);
  });
  expect((screen.getByRole("textbox") as HTMLTextAreaElement).disabled).toBe(
    true,
  );
  expect(screen.getByRole("button", { name: "停止生成" })).toBeTruthy();
  expect(screen.queryByText(result.answer)).toBeNull();
  await act(async () => {
    secondFinish({ ...result, answer: "新请求完整回复" });
  });
  await screen.findByText("新请求完整回复");
});

it("sends once using saved defaults and the frozen revision without a confirmation step", async () => {
  const settings = {
    ...analysisSettings,
    webSearch: false,
    contextSelection: {
      ...analysisSettings.contextSelection,
      includeProfile: false,
      includeEvidence: false,
    },
  };
  vi.mocked(settingsApi.getAnalysisSettings).mockResolvedValue(settings);
  render(<Advisor {...props} />);
  ask("我的持仓有什么风险？");
  await screen.findByText(result.answer);
  expect(researchApi.previewAnalysis).toHaveBeenCalledWith({
    ...settings,
    previousTurn: undefined,
    question: "我的持仓有什么风险？",
  });
  expect(researchApi.runAnalysis).toHaveBeenCalledWith(
    {
      ...vi.mocked(researchApi.previewAnalysis).mock.calls[0][0],
      previewRevision: "revision-1",
    },
    expect.any(Function),
    expect.any(AbortSignal),
  );
  expect(researchApi.runAnalysis).toHaveBeenCalledTimes(1);
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
  vi.mocked(researchApi.previewAnalysis).mockReturnValue(
    new Promise((_, fail) => {
      reject = fail;
    }),
  );
  render(<Advisor {...props} />);
  ask("待回答的问题");
  await waitFor(() =>
    expect(researchApi.previewAnalysis).toHaveBeenCalledTimes(1),
  );
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
  expect(researchApi.runAnalysis).not.toHaveBeenCalled();
});

it("does not send if saved settings cannot be read", async () => {
  vi.mocked(settingsApi.getAnalysisSettings).mockRejectedValue(
    new Error("预设读取失败"),
  );
  render(<Advisor {...props} />);
  ask("问题");
  expect((await screen.findByRole("alert")).textContent).toContain(
    "预设读取失败",
  );
  expect(researchApi.previewAnalysis).not.toHaveBeenCalled();
  expect(researchApi.runAnalysis).not.toHaveBeenCalled();
});

it("opens the complete saved report from history without a decision handoff", async () => {
  vi.mocked(researchApi.getAnalysisHistory).mockResolvedValue([
    { id: "a", question: "历史问题", createdAt: "2026-10-12T00:00:00Z" },
  ]);
  vi.mocked(researchApi.getAnalysis).mockResolvedValue({
    id: "a",
    question: "历史问题",
    answer: "完整历史 report",
  } as StoredAnalysis);
  render(<Advisor {...props} />);
  fireEvent.click(await screen.findByRole("button", { name: /历史问题/ }));
  await screen.findByText("完整历史 report");
  expect(researchApi.getAnalysis).toHaveBeenCalledWith("a");
  expect(researchApi.runAnalysis).not.toHaveBeenCalled();
});

it("disables native search for Chat Completions even when enabled in saved defaults", async () => {
  render(
    <Advisor {...props} model={{ ...props.model, provider: "openai-chat" }} />,
  );
  ask("问题");
  await screen.findByText(result.answer);
  expect(researchApi.previewAnalysis).toHaveBeenCalledWith(
    expect.objectContaining({ webSearch: false }),
  );
  expect(researchApi.runAnalysis).toHaveBeenCalledWith(
    expect.objectContaining({ webSearch: false }),
    expect.any(Function),
    expect.any(AbortSignal),
  );
});

it("automatically carries previous context and rereads saved defaults, then clears context for a new conversation", async () => {
  render(<Advisor {...props} />);
  ask("第一次提问");
  await screen.findByText(result.answer);
  vi.mocked(settingsApi.getAnalysisSettings).mockResolvedValue({
    ...analysisSettings,
  });
  vi.mocked(researchApi.runAnalysis).mockResolvedValue({
    ...result,
    id: "answer-2",
    answer: "第二次 report",
  });
  ask("三年内需要用钱");
  await screen.findByText("第二次 report");
  expect(screen.getByText(result.answer)).toBeTruthy();
  const followUp = vi.mocked(researchApi.previewAnalysis).mock.lastCall?.[0];
  expect(followUp?.previousTurn?.answer).toBe(result.answer);
  expect(followUp?.question).toBe("三年内需要用钱");
  fireEvent.click(screen.getByRole("button", { name: "新对话" }));
  expect(screen.queryByText(result.answer)).toBeNull();
  ask("一个新问题");
  await screen.findByText("第二次 report");
  expect(
    vi.mocked(researchApi.previewAnalysis).mock.lastCall?.[0].question,
  ).toBe("一个新问题");
});

it("reopens a historical answer in the same report view and allows continuing that conversation", async () => {
  vi.mocked(researchApi.getAnalysisHistory).mockResolvedValue([
    { id: "saved", question: "旧问题", createdAt: "2026-10-10T10:00:00Z" },
  ]);
  vi.mocked(researchApi.getAnalysis).mockResolvedValue({
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
  expect(
    vi.mocked(researchApi.previewAnalysis).mock.lastCall?.[0].previousTurn
      ?.answer,
  ).toContain("历史 report");
});

it("accepts an explicit question only when active", async () => {
  const onQuestionAccepted = vi.fn();
  const pendingQuestion = {
    id: 1,
    question: "从资产概览带来的问题",
  };
  const view = render(
    <Advisor
      {...props}
      active={false}
      pendingQuestion={pendingQuestion}
      onQuestionAccepted={onQuestionAccepted}
    />,
  );
  expect(onQuestionAccepted).not.toHaveBeenCalled();
  view.rerender(
    <Advisor
      {...props}
      active
      pendingQuestion={pendingQuestion}
      onQuestionAccepted={onQuestionAccepted}
    />,
  );
  expect(
    await screen.findByDisplayValue(pendingQuestion.question),
  ).toBeTruthy();
  expect(onQuestionAccepted).toHaveBeenCalledWith(1);
  view.rerender(
    <Advisor {...props} active onQuestionAccepted={onQuestionAccepted} />,
  );
  fireEvent.click(screen.getByRole("button", { name: "发送" }));
  await screen.findByText(result.answer);
  expect(researchApi.previewAnalysis).toHaveBeenCalledWith(
    expect.objectContaining({
      question: pendingQuestion.question,
    }),
  );
});

it("previews the actual selected payload without sending an analysis", async () => {
  vi.mocked(researchApi.previewAnalysis).mockResolvedValue({
    ...preview,
    model: "test-model",
    generatedAt: "2026-10-25T12:40:00Z",
    groups: [
      {
        key: "portfolio",
        label: "我的持仓",
        included: true,
        recordCount: 2,
        sensitivity: "个人资料",
        description: "已选择的资产",
      },
    ],
    payload: { portfolio: [{ name: "核对样例", marketValue: "1234.56" }] },
    localOnly: ["API Key"],
  });
  render(<Advisor {...props} />);
  fireEvent.click(screen.getByRole("button", { name: "查看将发送的数据" }));
  await screen.findByRole("heading", { name: "实际资料正文" });
  expect(screen.getByText(/核对样例/)).toBeTruthy();
  expect(researchApi.runAnalysis).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "关闭详情" }));
  expect(screen.queryByRole("dialog")).toBeNull();
});
