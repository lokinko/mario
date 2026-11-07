// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { WatchRulesEditor } from "./WatchRulesEditor";
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
  onClose: vi.fn(),
  onEdited: vi.fn(),
  onSaved: vi.fn(),
  onReload: vi.fn(),
});

it("edits both periods and decimal yield, submitting exact basis points with the frozen original rules", async () => {
  vi.mocked(api.saveWatchRules).mockResolvedValue(board());
  const p = props();
  const ui = render(<WatchRulesEditor {...p} />);
  fireEvent.change(screen.getByLabelText("最低税前 TTM 股息率（%）"), {
    target: { value: "3.33" },
  });
  fireEvent.change(screen.getByLabelText("日线参考轨"), {
    target: { value: "upper" },
  });
  fireEvent.change(screen.getByLabelText("日线比较方式"), {
    target: { value: "at_or_above" },
  });
  fireEvent.change(screen.getByLabelText("日线偏移（%）"), {
    target: { value: "-1.25" },
  });
  fireEvent.change(screen.getByLabelText("周线参考轨"), {
    target: { value: "lower" },
  });
  fireEvent.change(screen.getByLabelText("周线比较方式"), {
    target: { value: "near" },
  });
  fireEvent.change(screen.getByLabelText("周线偏移（%）"), {
    target: { value: "4.5" },
  });
  // A parent poll cannot silently move the editor's baseline or overwrite its draft.
  ui.rerender(
    <WatchRulesEditor {...p} rules={{ ...defaultRules, minYieldBps: 700 }} />,
  );
  fireEvent.click(screen.getByRole("button", { name: "保存观察规则" }));
  await waitFor(() => expect(p.onSaved).toHaveBeenCalled());
  expect(api.saveWatchRules).toHaveBeenCalledWith(
    {
      minYieldBps: 333,
      daily: { reference: "upper", mode: "at_or_above", offsetBps: -125 },
      weekly: { reference: "lower", mode: "near", offsetBps: 450 },
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
  render(<WatchRulesEditor {...props()} />);
  fireEvent.change(screen.getByLabelText("最低税前 TTM 股息率（%）"), {
    target: { value: "3" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存观察规则" }));
  await screen.findByText(/观察规则已被更新/);
  expect(
    (screen.getByLabelText("最低税前 TTM 股息率（%）") as HTMLInputElement)
      .value,
  ).toBe("3");
  fireEvent.click(screen.getByRole("button", { name: "重新读取规则" }));
  expect(api.getWatchBoard).not.toHaveBeenCalled();
  confirm.mockReturnValue(true);
  fireEvent.click(screen.getByRole("button", { name: "重新读取规则" }));
  await waitFor(() =>
    expect(
      (screen.getByLabelText("最低税前 TTM 股息率（%）") as HTMLInputElement)
        .value,
    ).toBe("4.5"),
  );
  fireEvent.click(screen.getByRole("button", { name: "恢复默认值" }));
  expect(
    (screen.getByLabelText("最低税前 TTM 股息率（%）") as HTMLInputElement)
      .value,
  ).toBe("5");
  expect(
    (screen.getByLabelText("日线偏移（%）") as HTMLInputElement).value,
  ).toBe("2");
});

it("rejects more than two decimal places without submitting a rounded percentage", async () => {
  render(<WatchRulesEditor {...props()} />);
  fireEvent.change(screen.getByLabelText("最低税前 TTM 股息率（%）"), {
    target: { value: "3.333" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存观察规则" }));
  expect(screen.getByText("请输入百分比，最多两位小数。")).toBeTruthy();
  expect(api.saveWatchRules).not.toHaveBeenCalled();
});
