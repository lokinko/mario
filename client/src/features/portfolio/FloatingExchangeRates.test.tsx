// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { FloatingExchangeRates } from "./FloatingExchangeRates";

vi.mock("./ExchangeRates", () => ({
  ExchangeRates: () => (
    <section aria-label="汇率设置">
      <input aria-label="汇率草稿" />
    </section>
  ),
}));
afterEach(cleanup);

it("opens a separate floating panel and restores trigger focus on Escape", () => {
  render(<FloatingExchangeRates baseCurrency="CNY" onUpdate={vi.fn()} />);
  const trigger = screen.getByRole("button", { name: "展开汇率浮窗" });
  expect(screen.queryByRole("region", { name: "汇率设置" })).toBeNull();
  fireEvent.click(trigger);
  expect(trigger.getAttribute("aria-expanded")).toBe("true");
  expect(
    document.getElementById(trigger.getAttribute("aria-controls")!),
  ).toBeTruthy();
  expect(document.activeElement).toBe(
    screen.getByRole("button", { name: "收起汇率" }),
  );
  fireEvent.keyDown(window, { key: "Escape" });
  expect(screen.queryByRole("region", { name: "汇率设置" })).toBeNull();
  expect(document.activeElement).toBe(trigger);
  fireEvent.click(trigger);
  fireEvent.change(screen.getByRole("textbox", { name: "汇率草稿" }), {
    target: { value: "7.1234" },
  });
  fireEvent.click(screen.getByRole("button", { name: "收起汇率" }));
  expect(trigger.getAttribute("aria-expanded")).toBe("false");
  expect(document.activeElement).toBe(trigger);
  fireEvent.click(trigger);
  expect(screen.getByDisplayValue("7.1234")).toBeTruthy();
});
