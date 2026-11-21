// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it } from "vitest";
import { DividendPreview } from "./DividendPreview";

beforeEach(() => {
  window.history.replaceState(null, "", "/?preview=dividend");
});

afterEach(() => {
  cleanup();
});

it("matches A-share codes regardless of exchange suffix or letter case and populates datalist", () => {
  render(<DividendPreview />);
  const options = document.querySelectorAll("#dividend-preview-stocks option");
  expect(options.length).toBeGreaterThan(5);

  const input = screen.getByLabelText("公司名称或 A 股代码");
  fireEvent.change(input, { target: { value: "  601088.ss  " } });
  fireEvent.submit(input.closest("form")!);
  expect(screen.getByRole("heading", { name: "中国神华" })).toBeTruthy();

  fireEvent.change(input, { target: { value: "000333.SZ" } });
  fireEvent.submit(input.closest("form")!);
  expect(screen.getByRole("heading", { name: "美的集团" })).toBeTruthy();
});

it("resolves unique partial names and shows empty state on ambiguous or unknown queries", () => {
  render(<DividendPreview />);
  const input = screen.getByLabelText("公司名称或 A 股代码");

  // Unique partial name resolves cleanly.
  fireEvent.change(input, { target: { value: "长江电" } });
  fireEvent.submit(input.closest("form")!);
  expect(screen.getByRole("heading", { name: "长江电力" })).toBeTruthy();

  // Ambiguous short query ("银行" matches multiple banks) must not silently pick an arbitrary company.
  fireEvent.change(input, { target: { value: "银行" } });
  fireEvent.submit(input.closest("form")!);
  expect(screen.getByText("样本中没有找到这家公司")).toBeTruthy();
  expect(screen.queryByRole("button", { name: /分享卡片/ })).toBeNull();

  // Unknown symbol renders empty state cleanly without crashing.
  fireEvent.change(input, { target: { value: "999999.BJ" } });
  fireEvent.submit(input.closest("form")!);
  expect(screen.getByText("样本中没有找到这家公司")).toBeTruthy();
  expect(screen.queryByRole("button", { name: /分享卡片/ })).toBeNull();
});
