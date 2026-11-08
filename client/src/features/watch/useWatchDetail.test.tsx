// @vitest-environment jsdom
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import type { WatchMarketRow } from "../../api/watch";
import { defaultRules } from "../../test/watchRules";
import { useWatchDetail } from "./useWatchDetail";

afterEach(cleanup);
function summary(symbol: string): WatchMarketRow {
  return {
    latest: {
      symbol,
      observedOn: "2026-11-06",
      checkedAt: "2026-11-06T19:00:00+08:00",
      rules: defaultRules,
      yieldMatchesRule: true,
      price: 100,
      dividendPerShare: "5",
      yieldPct: 5,
      daily: null,
      weekly: null,
      status: "incomplete",
      issues: [],
      sources: [],
      dividendEvents: [],
    },
    dividendPrices: [],
    checkedAt: null,
    refreshFailed: false,
  };
}

it("aborts the previous company and ignores its late result while reading the new one", async () => {
  const a = summary("600036.SS"),
    b = summary("600941.SS");
  let finishA!: (row: WatchMarketRow) => void;
  const load = vi
    .fn()
    .mockImplementationOnce(
      () =>
        new Promise<WatchMarketRow>((resolve) => {
          finishA = resolve;
        }),
    )
    .mockResolvedValueOnce({ ...b, latest: { ...b.latest!, price: 103.49 } });
  const { result, rerender, unmount } = renderHook(
    ({ row }) => useWatchDetail(row, row.latest!.symbol, load),
    { initialProps: { row: a } },
  );
  const firstSignal = load.mock.calls[0][1] as AbortSignal;
  expect(result.current.loading).toBe(true);
  rerender({ row: b });
  expect(firstSignal.aborted).toBe(true);
  await waitFor(() => expect(result.current.row.latest?.price).toBe(103.49));
  await act(async () =>
    finishA({ ...a, latest: { ...a.latest!, price: 777 } }),
  );
  expect(result.current.row.latest?.symbol).toBe("600941.SS");
  expect(result.current.row.latest?.price).toBe(103.49);
  unmount();
  expect((load.mock.calls[1][1] as AbortSignal).aborted).toBe(true);
});

it("retains list facts on detail failure, retries reads, and invalidates after a new observation", async () => {
  const row = summary("600036.SS");
  const load = vi
    .fn()
    .mockRejectedValueOnce(new Error("read unavailable"))
    .mockResolvedValueOnce(row)
    .mockResolvedValueOnce({ ...row, latest: { ...row.latest!, price: 101 } });
  const { result, rerender } = renderHook(
    ({ value }) => useWatchDetail(value, "600036.SS", load),
    { initialProps: { value: row } },
  );
  await waitFor(() =>
    expect(result.current.error).toContain("read unavailable"),
  );
  expect(result.current.row).toBe(row);
  act(() => result.current.retry());
  await waitFor(() => expect(result.current.error).toBe(""));
  expect(load).toHaveBeenCalledTimes(2);
  rerender({ value: { ...row, checkedAt: "2026-11-07T19:00:00+08:00" } });
  await waitFor(() => expect(result.current.row.latest?.price).toBe(101));
  expect(load).toHaveBeenCalledTimes(3);
});

it("does not request details when no observation has been saved", () => {
  const row = { ...summary("600036.SS"), latest: null },
    load = vi.fn();
  const { result } = renderHook(() => useWatchDetail(row, "600036.SS", load));
  expect(load).not.toHaveBeenCalled();
  expect(result.current.loading).toBe(false);
});
