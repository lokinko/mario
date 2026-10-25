// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { InvestmentMethodology } from "./InvestmentMethodology";
import * as api from "../../api/settings";

vi.mock("../../api/settings", () => ({
  getInvestmentMethodology: vi.fn(),
  saveInvestmentMethodology: vi.fn(),
}));
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
  vi.restoreAllMocks();
});
const stored = {
  prompt: "先核对现金流，再比较方案",
  defaultPrompt: "初始方法论",
  maxLength: 4000,
};

it("edits the full saved prompt, preserves a failed draft and saves a retry", async () => {
  vi.mocked(api.getInvestmentMethodology).mockResolvedValue(stored);
  vi.mocked(api.saveInvestmentMethodology)
    .mockRejectedValueOnce(new Error("保存冲突，请重新核对"))
    .mockImplementationOnce(async (prompt) => ({ ...stored, prompt }));
  const flash = vi.fn(),
    onSaved = vi.fn();
  render(
    <InvestmentMethodology
      onEdited={vi.fn()}
      flash={flash}
      onSaved={onSaved}
    />,
  );
  const editor = (await screen.findByLabelText(
    "投资方法论正文",
  )) as HTMLTextAreaElement;
  expect(editor.value).toBe(stored.prompt);
  fireEvent.change(editor, {
    target: { value: "优先解释费用，并比较保持不动" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存投资方法论" }));
  expect((await screen.findByRole("alert")).textContent).toContain("保存冲突");
  expect(editor.value).toBe("优先解释费用，并比较保持不动");
  expect(onSaved).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "保存投资方法论" }));
  await waitFor(() => expect(onSaved).toHaveBeenCalledTimes(1));
  expect(api.saveInvestmentMethodology).toHaveBeenLastCalledWith(editor.value);
  expect(flash).toHaveBeenCalledTimes(1);
  expect(
    screen
      .getByRole("button", { name: "保存投资方法论" })
      .hasAttribute("disabled"),
  ).toBe(true);
});

it("loads a template only into the draft and rejects blank or oversized text", async () => {
  vi.mocked(api.getInvestmentMethodology).mockResolvedValue(stored);
  render(
    <InvestmentMethodology
      onEdited={vi.fn()}
      flash={vi.fn()}
      onSaved={vi.fn()}
    />,
  );
  const editor = (await screen.findByLabelText(
    "投资方法论正文",
  )) as HTMLTextAreaElement;
  vi.spyOn(window, "confirm").mockReturnValue(true);
  fireEvent.click(screen.getByRole("button", { name: "使用初始范本" }));
  expect(editor.value).toBe(stored.defaultPrompt);
  expect(api.saveInvestmentMethodology).not.toHaveBeenCalled();
  for (const value of ["  \n", "中".repeat(4001)]) {
    fireEvent.change(editor, { target: { value } });
    expect(
      screen
        .getByRole("button", { name: "保存投资方法论" })
        .hasAttribute("disabled"),
    ).toBe(true);
    expect(screen.getByRole("alert")).toBeTruthy();
  }
});

it("retries a failed load without submitting a replacement prompt", async () => {
  vi.mocked(api.getInvestmentMethodology)
    .mockRejectedValueOnce(new Error("读取失败"))
    .mockResolvedValueOnce(stored);
  render(
    <InvestmentMethodology
      onEdited={vi.fn()}
      flash={vi.fn()}
      onSaved={vi.fn()}
    />,
  );
  await screen.findByRole("alert");
  expect(screen.queryByLabelText("投资方法论正文")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "重新加载" }));
  await screen.findByLabelText("投资方法论正文");
  expect(api.saveInvestmentMethodology).not.toHaveBeenCalled();
});
