// @vitest-environment jsdom
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { useDisplayPreferences } from "./useDisplayPreferences";
afterEach(() => {
  cleanup();
  localStorage.clear();
  document.documentElement.removeAttribute("data-mask-amounts");
  delete document.documentElement.dataset.pnl;
  delete document.documentElement.dataset.theme;
  vi.restoreAllMocks();
});
it("defaults to Chinese P/L colors and system theme, and persists display choices across mounts", () => {
  const first = renderHook(useDisplayPreferences);
  expect(first.result.current.pnl).toBe("cn");
  expect(first.result.current.theme).toBe("system");
  expect(document.documentElement.dataset.pnl).toBe("cn");
  expect(document.documentElement.dataset.theme).toBe("system");
  act(() => {
    first.result.current.setPnl("intl");
    first.result.current.setMaskAmounts(true);
    first.result.current.setTheme("dark");
  });
  expect(document.documentElement.hasAttribute("data-mask-amounts")).toBe(true);
  expect(document.documentElement.dataset.pnl).toBe("intl");
  expect(document.documentElement.dataset.theme).toBe("dark");
  first.unmount();
  const next = renderHook(useDisplayPreferences);
  expect(next.result.current.maskAmounts).toBe(true);
  expect(next.result.current.pnl).toBe("intl");
  expect(next.result.current.theme).toBe("dark");
  act(() => next.result.current.setMaskAmounts(false));
  expect(document.documentElement.hasAttribute("data-mask-amounts")).toBe(
    false,
  );
});
it("keeps presentation usable with malformed or unavailable browser storage", () => {
  localStorage.setItem("mario.display", "{invalid");
  const hook = renderHook(useDisplayPreferences);
  expect(hook.result.current.maskAmounts).toBe(false);
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
    throw new Error("storage blocked");
  });
  act(() => hook.result.current.setPnl("intl"));
  expect(document.documentElement.dataset.pnl).toBe("intl");
});
