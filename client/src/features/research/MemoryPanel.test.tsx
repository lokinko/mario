// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { MemoryPanel } from "./MemoryPanel";
import * as memoryApi from "../../api/memory";
import type { MemoryNote, MemoryOverview } from "../../types";

vi.mock("../../api/memory", () => ({
  getMemory: vi.fn(),
  dreamMemory: vi.fn(),
  setMemoryStatus: vi.fn(),
  deleteMemory: vi.fn(),
}));

const note = (overrides: Partial<MemoryNote>): MemoryNote => ({
  id: "m1",
  kind: "preference",
  origin: "userStated",
  statement: "先给结论再列依据",
  rationale: "用户多次要求",
  sourceAnalysisIds: ["a1"],
  quote: "请先说结论",
  active: true,
  createdAt: "2026-11-01T00:00:00Z",
  updatedAt: "2026-11-02T00:00:00Z",
  ...overrides,
});
const overview = (items: MemoryNote[]): MemoryOverview => ({
  items,
  dream: {
    lastRunAt: "2026-11-02T00:00:00Z",
    lastResult: {
      analyses: 3,
      added: 1,
      updated: 0,
      archived: 0,
      rejected: ["引用不在来源对话中"],
    },
  },
  pendingAnalyses: 2,
  dreamDue: false,
  enabled: true,
});

beforeEach(() => {
  vi.mocked(memoryApi.getMemory).mockResolvedValue(
    overview([
      note({}),
      note({
        id: "m2",
        kind: "viewpoint",
        origin: "modelInferred",
        statement: "股息率回落前不追高",
        quote: "",
      }),
    ]),
  );
  vi.spyOn(window, "confirm").mockReturnValue(true);
});
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

it("separates stated preferences from model-inferred views and shows rejected consolidations", async () => {
  const onOpenAnalysis = vi.fn();
  render(
    <MemoryPanel
      dreaming={false}
      dreamError=""
      onDream={vi.fn()}
      onOpenAnalysis={onOpenAnalysis}
    />,
  );
  const preference = await screen.findByRole("region", { name: "偏好" });
  expect(preference.textContent).toContain("你的原话");
  expect(preference.textContent).toContain("请先说结论");
  expect(screen.getByRole("region", { name: "观点" }).textContent).toContain(
    "模型归纳",
  );
  expect(
    screen.getByText("被拒绝的整理建议").parentElement!.textContent,
  ).toContain("引用不在来源对话中");
  fireEvent.click(screen.getAllByRole("button", { name: "对话 1" })[0]);
  expect(onOpenAnalysis).toHaveBeenCalledWith("a1");
});

it("archives and deletes notes, then reloads the list", async () => {
  vi.mocked(memoryApi.setMemoryStatus).mockResolvedValue(
    note({ active: false }),
  );
  vi.mocked(memoryApi.deleteMemory).mockResolvedValue();
  render(
    <MemoryPanel
      dreaming={false}
      dreamError=""
      onDream={vi.fn()}
      onOpenAnalysis={vi.fn()}
    />,
  );
  fireEvent.click((await screen.findAllByRole("button", { name: "归档" }))[0]);
  await waitFor(() =>
    expect(memoryApi.setMemoryStatus).toHaveBeenCalledWith("m1", false),
  );
  fireEvent.click(screen.getAllByRole("button", { name: "删除" })[1]);
  await waitFor(() =>
    expect(memoryApi.deleteMemory).toHaveBeenCalledWith("m2"),
  );
  expect(memoryApi.getMemory).toHaveBeenCalledTimes(3);
});

it("starts consolidation on request and disables it while running", async () => {
  const onDream = vi.fn();
  const { rerender } = render(
    <MemoryPanel
      dreaming={false}
      dreamError=""
      onDream={onDream}
      onOpenAnalysis={vi.fn()}
    />,
  );
  fireEvent.click(await screen.findByRole("button", { name: "立即整理" }));
  expect(onDream).toHaveBeenCalledTimes(1);
  rerender(
    <MemoryPanel
      dreaming
      dreamError="整理失败"
      onDream={onDream}
      onOpenAnalysis={vi.fn()}
    />,
  );
  expect(
    (screen.getByRole("button", { name: "整理中…" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
  expect(screen.getByText("整理失败")).toBeTruthy();
});
