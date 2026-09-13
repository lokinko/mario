import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { startAutoSync } from "./autoSync";
import { HttpError } from "./transport";
import {
  beforeDataWrite,
  blockStaleWrites,
  endSync,
  isDataWrite,
  markDataWrite,
} from "./syncEvents";
import type { AutoSyncResult } from "../types";
let controllers: ReturnType<typeof startAutoSync>[] = [];
beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date("2026-09-12T00:00:00Z"));
});
afterEach(() => {
  controllers.forEach((c) => c.dispose());
  controllers = [];
  endSync();
  blockStaleWrites(false);
  vi.useRealTimers();
});
const synced: AutoSyncResult = {
  state: "synced",
  sync: {
    direction: "push",
    revision: 1,
    contentHash: "test",
    recordCount: 1,
    syncedAt: "now",
    message: "ok",
    localUpdated: false,
  },
};
function setup(overrides = {}) {
  const options = {
    sync: vi.fn().mockResolvedValue(synced),
    visible: () => true,
    online: () => true,
    onStatus: vi.fn(),
    onRemoteUpdate: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  };
  const controller = startAutoSync(options);
  controllers.push(controller);
  return { controller, ...options };
}
it("coalesces rapid writes, never overlaps and uses increasingly sparse idle checks", async () => {
  const { controller, sync } = setup();
  await vi.advanceTimersByTimeAsync(1000);
  expect(sync).toHaveBeenCalledTimes(1);
  for (let i = 0; i < 5; i++) {
    controller.changed();
    await vi.advanceTimersByTimeAsync(250);
  }
  expect(sync).toHaveBeenCalledTimes(1);
  await vi.advanceTimersByTimeAsync(1500);
  expect(sync).toHaveBeenCalledTimes(2);
  await vi.advanceTimersByTimeAsync(60000);
  expect(sync).toHaveBeenCalledTimes(3);
  await vi.advanceTimersByTimeAsync(120000);
  expect(sync).toHaveBeenCalledTimes(4);
  await vi.advanceTimersByTimeAsync(299000);
  expect(sync).toHaveBeenCalledTimes(4);
});
it("stops background checks and syncs when visible again", async () => {
  let visible = true;
  const { controller, sync } = setup({ visible: () => visible });
  await vi.advanceTimersByTimeAsync(1000);
  visible = false;
  controller.wake();
  controller.changed();
  await vi.advanceTimersByTimeAsync(600000);
  expect(sync).toHaveBeenCalledTimes(1);
  visible = true;
  controller.wake();
  await vi.advanceTimersByTimeAsync(1);
  expect(sync).toHaveBeenCalledTimes(2);
});
it("backs off transient failures and pauses on conflicts until explicitly retried", async () => {
  const { controller, sync, onStatus } = setup();
  sync.mockRejectedValue(new Error("offline"));
  await vi.advanceTimersByTimeAsync(1000);
  expect(sync).toHaveBeenCalledTimes(1);
  await vi.advanceTimersByTimeAsync(10000);
  expect(sync).toHaveBeenCalledTimes(2);
  await vi.advanceTimersByTimeAsync(19999);
  expect(sync).toHaveBeenCalledTimes(2);
  sync.mockRejectedValue(new HttpError("same record conflict", 409));
  await vi.advanceTimersByTimeAsync(1);
  expect(onStatus).toHaveBeenLastCalledWith({
    phase: "attention",
    message: "same record conflict",
  });
  await vi.advanceTimersByTimeAsync(600000);
  expect(sync).toHaveBeenCalledTimes(3);
  sync.mockResolvedValue(synced);
  controller.retry();
  await vi.advanceTimersByTimeAsync(1);
  expect(sync).toHaveBeenCalledTimes(4);
});
it("holds edits until remote updates have been checked and never submits a stale form", async () => {
  let resolve!: (result: AutoSyncResult) => void;
  const { sync } = setup({
    onRemoteUpdate: async () => {
      blockStaleWrites(true);
    },
  });
  sync.mockImplementation(
    () =>
      new Promise((r) => {
        resolve = r;
      }),
  );
  await vi.advanceTimersByTimeAsync(1000);
  let sent = false;
  const pending = beforeDataWrite().then(
    () => {
      sent = true;
    },
    (e) => e.message,
  );
  expect(sent).toBe(false);
  resolve({ ...synced, sync: { ...synced.sync!, localUpdated: true } });
  expect(await pending).toContain("查看最新资料");
  expect(sent).toBe(false);
});
it("does not poll signed-out accounts and wakes after login", async () => {
  const { controller, sync } = setup();
  sync.mockResolvedValue({ state: "signed_out", sync: null });
  await vi.advanceTimersByTimeAsync(1000);
  await vi.advanceTimersByTimeAsync(600000);
  expect(sync).toHaveBeenCalledTimes(1);
  sync.mockResolvedValue(synced);
  controller.retry();
  await vi.advanceTimersByTimeAsync(1);
  expect(sync).toHaveBeenCalledTimes(2);
});
it("triggers only for persisted investment data, not previews or device secrets", () => {
  for (const path of [
    "/profile",
    "/holdings/h/verified-valuation",
    "/analysis",
    "/portfolio-events/import/commit",
  ])
    expect(isDataWrite(path, "POST")).toBe(true);
  for (const path of [
    "/analysis/preview",
    "/model-config/codex",
    "/cloud/sync/auto",
    "/portfolio-events/import/preview",
    "/reminder-settings",
  ])
    expect(isDataWrite(path, "POST")).toBe(false);
  expect(isDataWrite("/holdings", "GET")).toBe(false);
});

it("keeps a single request in flight when more edits arrive", async () => {
  let resolve!: (result: AutoSyncResult) => void;
  const { controller, sync } = setup();
  sync.mockImplementationOnce(
    () =>
      new Promise((r) => {
        resolve = r;
      }),
  );
  await vi.advanceTimersByTimeAsync(1000);
  controller.changed();
  controller.changed();
  controller.wake();
  await vi.advanceTimersByTimeAsync(30000);
  expect(sync).toHaveBeenCalledTimes(1);
  resolve(synced);
  await vi.advanceTimersByTimeAsync(1501);
  expect(sync).toHaveBeenCalledTimes(2);
});
it("retains a pending UI refresh if reading the newly merged snapshot fails", async () => {
  const { sync, onRemoteUpdate } = setup();
  sync.mockResolvedValueOnce({
    ...synced,
    sync: { ...synced.sync!, localUpdated: true },
  });
  onRemoteUpdate.mockRejectedValueOnce(
    new Error("temporary local read failure"),
  );
  await vi.advanceTimersByTimeAsync(1000);
  await vi.advanceTimersByTimeAsync(10000);
  expect(onRemoteUpdate).toHaveBeenCalledTimes(2);
});

it("lets local writes finish before beginning a cloud request", async () => {
  const { sync } = setup();
  const release = markDataWrite();
  await vi.advanceTimersByTimeAsync(1000);
  expect(sync).not.toHaveBeenCalled();
  release();
  await vi.advanceTimersByTimeAsync(1500);
  expect(sync).toHaveBeenCalledTimes(1);
});
