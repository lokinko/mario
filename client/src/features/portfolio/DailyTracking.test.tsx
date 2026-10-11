// @vitest-environment jsdom
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { DailyTracking } from "./DailyTracking";
import * as api from "../../api";
vi.mock("../../api", () => ({ ensureDailyAssets: vi.fn() }));
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.clearAllMocks();
});
it("checks day boundaries in the saved timezone without saving on every foreground tick", async () => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date("2026-01-31T15:59:00Z"));
  vi.mocked(api.ensureDailyAssets).mockResolvedValue({
    timezone: "Asia/Shanghai",
    today: "2026-01-31",
    records: [],
    nextBefore: null,
  });
  const onNewDay = vi.fn();
  await act(async () => {
    render(<DailyTracking onNewDay={onNewDay} />);
  });
  expect(api.ensureDailyAssets).toHaveBeenCalledTimes(1);
  expect(onNewDay).not.toHaveBeenCalled();
  await act(async () => {
    window.dispatchEvent(new Event("focus"));
    vi.advanceTimersByTime(30000);
  });
  expect(api.ensureDailyAssets).toHaveBeenCalledTimes(1);
  vi.mocked(api.ensureDailyAssets).mockResolvedValue({
    timezone: "Asia/Shanghai",
    today: "2026-02-01",
    records: [],
    nextBefore: null,
  });
  await act(async () => {
    vi.advanceTimersByTime(30000);
  });
  expect(api.ensureDailyAssets).toHaveBeenCalledTimes(2);
  expect(onNewDay).toHaveBeenCalledTimes(1);
  expect(api.ensureDailyAssets).toHaveBeenLastCalledWith("Asia/Shanghai");
});
