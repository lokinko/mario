// @vitest-environment jsdom
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { getSnapshot } from "../api/portfolio";
import { getModelConfig } from "../api/settings";
import {
  beforeDataWrite,
  blockStaleWrites,
  SERVER_DATA_UPDATED,
} from "../lib/syncEvents";
import type { ModelConfig, Snapshot } from "../types";
import fixture from "../test/emptySnapshot.json";
import { useApplicationData } from "./useApplicationData";

vi.mock("../api/portfolio", () => ({ getSnapshot: vi.fn() }));
vi.mock("../api/settings", () => ({ getModelConfig: vi.fn() }));
vi.mock("../lib/autoSync", () => ({
  startAutoSync: () => ({
    changed: vi.fn(),
    retry: vi.fn(),
    wake: vi.fn(),
    dispose: vi.fn(),
  }),
}));
const original = fixture as Snapshot;
const configured: ModelConfig = {
  provider: "openai-chat",
  baseUrl: "https://model.example.test/v1",
  model: "test",
  hasApiKey: true,
};
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
beforeEach(() => {
  vi.resetAllMocks();
  blockStaleWrites(false);
  vi.mocked(getSnapshot).mockResolvedValue(original);
  vi.mocked(getModelConfig).mockResolvedValue(configured);
});
afterEach(() => {
  cleanup();
  blockStaleWrites(false);
});
async function start() {
  const hook = renderHook(() => useApplicationData("facts"));
  await waitFor(() => expect(hook.result.current.loading).toBe(false));
  return hook;
}

it("keeps a draft started while the remote snapshot is loading and blocks stale writes", async () => {
  const { result } = await start();
  const pending = deferred<Snapshot>();
  vi.mocked(getSnapshot).mockReturnValueOnce(pending.promise);
  let refresh!: Promise<boolean>;
  act(() => {
    refresh = result.current.loadRemoteData(true);
    result.current.markEdited();
  });
  await act(async () => {
    pending.resolve({ ...original, updatedAt: "remote" });
    await refresh;
  });
  expect(result.current.snapshot).toBe(original);
  expect(result.current.remotePending).toBe(true);
  expect(result.current.remoteViewRevision).toBe(0);
  await expect(beforeDataWrite()).rejects.toThrow("其他设备");
});
it("ignores an older refresh even if it completes after the latest refresh", async () => {
  const { result } = await start();
  const older = deferred<Snapshot>(),
    newer = { ...original, updatedAt: "newer" };
  vi.mocked(getSnapshot)
    .mockReturnValueOnce(older.promise)
    .mockResolvedValueOnce(newer);
  let first!: Promise<boolean>;
  act(() => {
    first = result.current.loadRemoteData();
  });
  await act(async () => {
    await result.current.loadRemoteData();
  });
  await act(async () => {
    older.resolve({ ...original, updatedAt: "older" });
    await first;
  });
  expect(result.current.snapshot).toBe(newer);
  expect(result.current.remoteViewRevision).toBe(1);
});
it.each(["snapshot", "model"] as const)(
  "keeps a saved first trade when an older refresh is waiting for %s",
  async (stage) => {
    const { result } = await start();
    const pendingSnapshot = deferred<Snapshot>();
    const pendingModel = deferred<ModelConfig>();
    if (stage === "snapshot")
      vi.mocked(getSnapshot).mockReturnValueOnce(pendingSnapshot.promise);
    else vi.mocked(getModelConfig).mockReturnValueOnce(pendingModel.promise);
    const saved = {
      ...original,
      updatedAt: "saved-first-trade",
      holdings: [{ id: "first-holding", quantity: 10 }],
      holdingTrades: [{ id: "first-trade", holdingId: "first-holding" }],
    } as Snapshot;
    const updated = vi.fn();
    window.addEventListener(SERVER_DATA_UPDATED, updated);
    try {
      let refresh!: Promise<boolean>;
      await act(async () => {
        refresh = result.current.loadRemoteData();
      });
      act(() => result.current.setSnapshot(saved));
      await act(async () => {
        pendingSnapshot.resolve(original);
        pendingModel.resolve(configured);
        expect(await refresh).toBe(false);
      });
      expect(result.current.snapshot).toBe(saved);
      expect(result.current.snapshot?.holdingTrades).toHaveLength(1);
      expect(result.current.snapshot?.holdings).toHaveLength(1);
      expect(result.current.remoteViewRevision).toBe(0);
      expect(updated).not.toHaveBeenCalled();
      await act(async () => {
        vi.mocked(getSnapshot).mockResolvedValueOnce(saved);
        expect(await result.current.loadRemoteData()).toBe(true);
      });
      expect(result.current.snapshot).toBe(saved);
      expect(result.current.remoteViewRevision).toBe(1);
    } finally {
      window.removeEventListener(SERVER_DATA_UPDATED, updated);
    }
  },
);
it("preserves usable model configuration and reports its independent refresh failure", async () => {
  const { result } = await start();
  const newer = { ...original, updatedAt: "newer" };
  vi.mocked(getSnapshot).mockResolvedValueOnce(newer);
  vi.mocked(getModelConfig).mockRejectedValueOnce(
    new Error("model unavailable"),
  );
  await act(async () => {
    await result.current.loadRemoteData();
  });
  expect(result.current.snapshot).toBe(newer);
  expect(result.current.model).toBe(configured);
  expect(result.current.modelError).toContain("model unavailable");
});
it("failed restore retains drafts and does not remove the stale-write guard", async () => {
  const { result } = await start();
  act(() => {
    result.current.markEdited();
  });
  await act(async () => {
    await result.current.loadRemoteData(true);
  });
  vi.mocked(getSnapshot).mockRejectedValueOnce(new Error("offline"));
  await act(async () => {
    await expect(result.current.refreshInvestmentData()).rejects.toThrow(
      "offline",
    );
  });
  expect(result.current.hasDraft()).toBe(true);
  expect(result.current.remotePending).toBe(true);
  expect(result.current.dataRevision).toBe(0);
  await expect(beforeDataWrite()).rejects.toThrow("其他设备");
});
it("explicit refresh accepts data before clearing the draft and permits writes again", async () => {
  const { result } = await start();
  act(() => {
    result.current.markEdited();
  });
  await act(async () => {
    await result.current.loadRemoteData(true);
  });
  const newer = { ...original, updatedAt: "confirmed" };
  vi.mocked(getSnapshot).mockResolvedValueOnce(newer);
  await act(async () => {
    await result.current.acceptRemoteData();
  });
  expect(result.current.snapshot).toBe(newer);
  expect(result.current.hasDraft()).toBe(false);
  expect(result.current.remotePending).toBe(false);
  await expect(beforeDataWrite()).resolves.toBeUndefined();
});
it("unmounted requests cannot publish a data-updated event", async () => {
  const { result, unmount } = await start();
  const pending = deferred<Snapshot>(),
    updated = vi.fn();
  window.addEventListener(SERVER_DATA_UPDATED, updated);
  try {
    vi.mocked(getSnapshot).mockReturnValueOnce(pending.promise);
    const refresh = result.current.loadRemoteData();
    unmount();
    pending.resolve(original);
    await expect(refresh).resolves.toBe(false);
    expect(updated).not.toHaveBeenCalled();
  } finally {
    window.removeEventListener(SERVER_DATA_UPDATED, updated);
  }
});
