// @vitest-environment jsdom
import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { Advisor } from "./Advisor";
import * as api from "../../api";
import type { StoredAnalysis } from "../../types";
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
