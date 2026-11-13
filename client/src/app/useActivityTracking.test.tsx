// @vitest-environment jsdom
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { recordActivity } from "../api/activity";
import { useActivityTracking } from "./useActivityTracking";

vi.mock("../api/activity", () => ({
  recordActivity: vi.fn().mockResolvedValue({ ok: true }),
}));
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.clearAllMocks();
});
const tick = async (ms: number) => {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
};

it("samples visible foreground use, pauses after idle and resumes on interaction", async () => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date("2026-11-13T04:00:00Z"));
  vi.spyOn(document, "visibilityState", "get").mockReturnValue("visible");
  const { unmount } = renderHook(() => useActivityTracking("watch"));
  await tick(0);
  expect(recordActivity).toHaveBeenCalledTimes(1);
  await tick(30000);
  expect(recordActivity).toHaveBeenCalledTimes(1);
  await tick(270000);
  expect(recordActivity).toHaveBeenCalledTimes(5);
  await tick(300000);
  expect(recordActivity).toHaveBeenCalledTimes(5);
  await act(async () => {
    // Scroll events from nested panels do not bubble to the window.
    document.body.dispatchEvent(new Event("scroll", { bubbles: false }));
  });
  expect(recordActivity).toHaveBeenCalledTimes(6);
  const signal = vi.mocked(recordActivity).mock.calls.at(-1)![1];
  unmount();
  expect(signal.aborted).toBe(true);
  await tick(60000);
  expect(recordActivity).toHaveBeenCalledTimes(6);
});

it("does not sample hidden tabs, records page changes and tolerates failures", async () => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date("2026-11-13T04:00:00Z"));
  const visibility = vi
    .spyOn(document, "visibilityState", "get")
    .mockReturnValue("hidden");
  const { rerender } = renderHook(
    ({ page }: { page: "watch" | "facts" }) => useActivityTracking(page),
    { initialProps: { page: "watch" } },
  );
  await tick(60000);
  expect(recordActivity).not.toHaveBeenCalled();
  visibility.mockReturnValue("visible");
  vi.mocked(recordActivity).mockRejectedValueOnce(new Error("offline"));
  await act(async () => {
    document.dispatchEvent(new Event("visibilitychange"));
  });
  await tick(30000);
  expect(recordActivity).toHaveBeenCalledTimes(2);
  rerender({ page: "facts" });
  await tick(0);
  expect(recordActivity).toHaveBeenLastCalledWith(
    "facts",
    expect.any(AbortSignal),
  );
});
