// @vitest-environment jsdom
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { Advisor } from "./Advisor";
import * as api from "../../api";
import type { AnalysisPreview, StoredAnalysis } from "../../types";
vi.mock("../../api", () => ({
  getAnalysis: vi.fn(),
  getAnalysisHistory: vi.fn().mockResolvedValue([]),
  previewAnalysis: vi.fn(),
  runAnalysis: vi.fn(),
}));
vi.mock("./AnalysisReport", () => ({
  StoredAnalysisView: ({ item }: { item: StoredAnalysis }) => (
    <p>{item.question}</p>
  ),
  StructuredReportView: () => null,
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
  fireEvent.click(screen.getByRole("button", { name: "预览将发送的数据" }));
  fireEvent.change(
    screen.getByRole("textbox", { name: "这次希望解决什么问题？" }),
    { target: { value: "另一个问题" } },
  );
  // An obsolete result must not be rendered or used for sending, even before parsing its UI fields.
  await act(async () => {
    resolve({ contextRevision: "obsolete" } as AnalysisPreview);
  });
  expect(screen.queryByRole("button", { name: "确认并开始分析" })).toBeNull();
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
