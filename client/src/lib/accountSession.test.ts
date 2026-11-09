// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { AccountSession } from "./accountSession";
import { currentSession, setWebToken } from "./webSession";
import { httpRequest } from "./httpClient";

afterEach(() => {
  setWebToken();
  localStorage.clear();
  vi.unstubAllGlobals();
});

it("releases waiting writes on disposal and isolates late sync/write completions", async () => {
  const first = new AccountSession("first", "/api");
  const finishSync = first.beginSync();
  const finishWrite = first.markDataWrite();
  const waiting = first.beforeDataWrite();
  first.dispose();
  await expect(waiting).rejects.toThrow("账号已切换");
  const second = new AccountSession("second", "/api");
  second.acceptRevision("second-version");
  const releaseSecond = second.beginSync();
  finishSync();
  finishWrite();
  finishWrite();
  let completed = false;
  const next = second.beforeDataWrite().then(() => {
    completed = true;
  });
  await Promise.resolve();
  expect(completed).toBe(false);
  releaseSecond();
  await next;
  expect(second.revision).toBe("second-version");
  expect(second.hasDataWrites()).toBe(false);
});

it("rejects stale responses across A → B → A even when the token matches again", async () => {
  setWebToken("a");
  const first = currentSession();
  let complete!: (response: Response) => void;
  const fetcher = vi.fn().mockImplementation(
    () =>
      new Promise<Response>((resolve) => {
        complete = resolve;
      }),
  );
  vi.stubGlobal("fetch", fetcher);
  const request = httpRequest("/snapshot");
  setWebToken("b");
  setWebToken("a");
  currentSession().acceptRevision("new-a");
  complete(new Response("{}", { headers: { "x-data-revision": "old-a" } }));
  await expect(request).rejects.toThrow("账号已切换");
  expect(first.signal.aborted).toBe(true);
  expect(fetcher.mock.calls[0][1].signal.aborted).toBe(true);
  expect(currentSession()).not.toBe(first);
  expect(currentSession().revision).toBe("new-a");
});

it("changing account invalidates a write waiting behind a sync barrier", async () => {
  setWebToken("a");
  const finish = currentSession().beginSync();
  const fetcher = vi.fn();
  vi.stubGlobal("fetch", fetcher);
  const request = httpRequest("/profile", { method: "PUT", body: "{}" });
  setWebToken("b");
  finish();
  await expect(request).rejects.toThrow("账号已切换");
  expect(fetcher).not.toHaveBeenCalled();
  expect(currentSession().hasDataWrites()).toBe(false);
});

it("invalidates queued work across A → B → A without delaying the new session", async () => {
  setWebToken("a");
  const first = currentSession();
  let release!: () => void;
  const running = first.enqueue(
    () =>
      new Promise<void>((resolve) => {
        release = resolve;
      }),
  );
  await Promise.resolve();
  await Promise.resolve();
  const staleWork = vi.fn().mockResolvedValue(undefined);
  const queued = first.enqueue(staleWork);
  setWebToken("b");
  setWebToken("a");
  const freshWork = vi.fn().mockResolvedValue("fresh");
  await expect(currentSession().enqueue(freshWork)).resolves.toBe("fresh");
  release();
  await running;
  await expect(queued).rejects.toThrow("账号已切换");
  expect(staleWork).not.toHaveBeenCalled();
  expect(freshWork).toHaveBeenCalledOnce();
});
