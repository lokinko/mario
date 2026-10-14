// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { MemoryCenter } from "./MemoryCenter";
import * as api from "../../api/memory";
import type { MemoryCandidate } from "../../types";
vi.mock("../../api/memory", () => ({
  getMemories: vi.fn(),
  saveMemoryPreference: vi.fn(),
}));
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
  vi.restoreAllMocks();
});
const item: MemoryCandidate = {
  id: "m1",
  kind: "analysis",
  title: "仓位复盘",
  summary: "先核对现金流，再检查估值。",
  content: { answer: "完整历史依据：核对现金流与资本开支，保留反方证据。" },
  createdAt: "2026-10-01",
  occurredAt: "2026-10-01",
  status: "有效",
  reviewed: true,
  contradiction: false,
  tags: [],
  preference: "default",
  preferenceNote: "",
  preferenceUpdatedAt: null,
  selected: false,
};
it("retains a failed preference draft and applies a successful retry without losing the search", async () => {
  vi.mocked(api.getMemories).mockResolvedValue([item]);
  vi.mocked(api.saveMemoryPreference)
    .mockRejectedValueOnce(new Error("保存失败"))
    .mockResolvedValueOnce({
      ...item,
      preference: "pinned",
      preferenceNote: "保留依据",
    });
  render(<MemoryCenter flash={vi.fn()} />);
  fireEvent.change(screen.getByLabelText("搜索记忆"), {
    target: { value: "仓位" },
  });
  fireEvent.click(await screen.findByRole("button", { name: "仓位复盘" }));
  expect(
    screen.getByText("完整历史依据：核对现金流与资本开支，保留反方证据。"),
  ).toBeTruthy();
  fireEvent.change(screen.getByLabelText("长期策略"), {
    target: { value: "pinned" },
  });
  fireEvent.change(screen.getByLabelText("我的注释（不会改写原记录）"), {
    target: { value: "保留依据" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存策略" }));
  expect(await screen.findByRole("alert")).toHaveProperty(
    "textContent",
    "Error: 保存失败",
  );
  expect(screen.getByDisplayValue("保留依据")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "保存策略" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(api.saveMemoryPreference).toHaveBeenLastCalledWith("m1", {
    preference: "pinned",
    note: "保留依据",
  });
  expect(screen.getByLabelText("搜索记忆")).toHaveProperty("value", "仓位");
  fireEvent.click(screen.getByRole("button", { name: "仓位复盘" }));
  expect(screen.getByLabelText("长期策略")).toHaveProperty("value", "pinned");
});
it("preserves cancelled drafts and resets them only after confirmed discard", async () => {
  vi.mocked(api.getMemories).mockResolvedValue([item]);
  const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
  render(<MemoryCenter flash={vi.fn()} />);
  fireEvent.click(await screen.findByRole("button", { name: "仓位复盘" }));
  fireEvent.change(screen.getByLabelText("长期策略"), {
    target: { value: "hidden" },
  });
  fireEvent.click(screen.getByLabelText("关闭详情"));
  expect(screen.getByRole("dialog")).toBeTruthy();
  confirm.mockReturnValue(true);
  fireEvent.click(screen.getByLabelText("关闭详情"));
  fireEvent.click(screen.getByRole("button", { name: "仓位复盘" }));
  expect(screen.getByLabelText("长期策略")).toHaveProperty("value", "default");
  expect(api.saveMemoryPreference).not.toHaveBeenCalled();
});
