// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import fixture from "../../test/emptySnapshot.json";
import type { Snapshot } from "../../types";
import { ExchangeRateAccess } from "./ExchangeRateAccess";

vi.mock("./ExchangeRates", () => ({
  ExchangeRates: () => (
    <section aria-label="汇率设置">
      <input aria-label="汇率草稿" />
    </section>
  ),
}));
afterEach(cleanup);

it("opens exchange rates in a modal drawer and restores focus", () => {
  render(
    <ExchangeRateAccess snapshot={fixture as Snapshot} onUpdate={vi.fn()} />,
  );
  const trigger = screen.getByRole("button", { name: "查看汇率" });
  trigger.focus();
  fireEvent.click(trigger);
  expect(screen.getByRole("dialog", { name: "汇率" })).toBeTruthy();
  fireEvent.keyDown(window, { key: "Escape" });
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(document.activeElement).toBe(trigger);
});
