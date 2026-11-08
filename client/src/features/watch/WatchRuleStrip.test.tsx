// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { WatchRuleStrip } from "./WatchRuleStrip";
import { defaultRules } from "../../test/watchRules";
import * as api from "../../api/watch";

vi.mock("../../api/watch", () => ({
  saveWatchRules: vi.fn(),
  getWatchBoard: vi.fn(),
}));
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  vi.restoreAllMocks();
});
const board = (rules = defaultRules): api.WatchBoard => ({
  rules,
  defaultRules,
  rows: [],
  weekly: [],
  weekStart: "2026-11-02",
  weekEnd: "2026-11-08",
});
const props = () => ({
  rules: structuredClone(defaultRules),
  defaults: defaultRules,
  disabled: false,
  onEdited: vi.fn(),
  onClosed: vi.fn(),
  onSaved: vi.fn(),
  onReload: vi.fn(),
});
const chip = (name: RegExp) => screen.getByRole("button", { name });

it("edits one condition from its chip and submits it with the other conditions unchanged", async () => {
  vi.mocked(api.saveWatchRules).mockResolvedValue(board());
  const p = props();
  const ui = render(<WatchRuleStrip {...p} />);
  fireEvent.click(chip(/② 日线/));
  fireEvent.change(await screen.findByLabelText("日线参考轨"), {
    target: { value: "upper" },
  });
  fireEvent.change(screen.getByLabelText("日线比较方式"), {
    target: { value: "at_or_above" },
  });
  fireEvent.change(screen.getByLabelText("日线偏移（%）"), {
    target: { value: "-1.25" },
  });
  expect(screen.queryByLabelText("周线参考轨")).toBeNull();
  expect(p.onEdited).toHaveBeenCalled();
  // A parent poll cannot silently move the editor's baseline or overwrite its draft.
  ui.rerender(
    <WatchRuleStrip {...p} rules={{ ...defaultRules, minYieldBps: 700 }} />,
  );
  fireEvent.click(screen.getByRole("button", { name: "保存规则" }));
  await waitFor(() => expect(p.onSaved).toHaveBeenCalled());
  expect(api.saveWatchRules).toHaveBeenCalledWith(
    {
      ...defaultRules,
      daily: { reference: "upper", mode: "at_or_above", offsetBps: -125 },
    },
    defaultRules,
  );
});

it("preserves a conflicting draft and reloads only after explicit discard", async () => {
  vi.mocked(api.saveWatchRules).mockRejectedValue(
    new Error("观察规则已被更新"),
  );
  vi.mocked(api.getWatchBoard).mockResolvedValue(
    board({ ...defaultRules, minYieldBps: 450 }),
  );
  const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
  render(<WatchRuleStrip {...props()} />);
  fireEvent.click(chip(/① TTM 税前股息/));
  const input = () =>
    screen.getByLabelText("最低税前 TTM 股息率（%）") as HTMLInputElement;
  fireEvent.change(await screen.findByLabelText("最低税前 TTM 股息率（%）"), {
    target: { value: "3" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存规则" }));
  await screen.findByText(/观察规则已被更新/);
  expect(input().value).toBe("3");
  fireEvent.click(screen.getByRole("button", { name: "重新读取规则" }));
  expect(api.getWatchBoard).not.toHaveBeenCalled();
  confirm.mockReturnValue(true);
  fireEvent.click(screen.getByRole("button", { name: "重新读取规则" }));
  await waitFor(() => expect(input().value).toBe("4.5"));
  fireEvent.click(screen.getByRole("button", { name: "恢复默认" }));
  expect(input().value).toBe("5");
});

it("rejects more than two decimal places without submitting a rounded percentage", async () => {
  render(<WatchRuleStrip {...props()} />);
  fireEvent.click(chip(/① TTM 税前股息/));
  fireEvent.change(await screen.findByLabelText("最低税前 TTM 股息率（%）"), {
    target: { value: "3.333" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存规则" }));
  expect(screen.getByText("请输入百分比，最多两位小数。")).toBeTruthy();
  expect(api.saveWatchRules).not.toHaveBeenCalled();
});

it("asks before discarding an edited condition and keeps other chips locked while editing", async () => {
  const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
  const p = props();
  render(<WatchRuleStrip {...p} />);
  fireEvent.click(chip(/③ 周线/));
  fireEvent.change(await screen.findByLabelText("周线偏移（%）"), {
    target: { value: "1" },
  });
  expect(chip(/① TTM 税前股息/).hasAttribute("disabled")).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: /取\s?消/ }));
  expect(confirm).toHaveBeenCalled();
  expect(screen.getByLabelText("周线偏移（%）")).toBeTruthy();
  confirm.mockReturnValue(true);
  fireEvent.click(screen.getByRole("button", { name: /取\s?消/ }));
  expect(p.onClosed).toHaveBeenCalled();
  await waitFor(() =>
    expect(chip(/① TTM 税前股息/).hasAttribute("disabled")).toBe(false),
  );
});
