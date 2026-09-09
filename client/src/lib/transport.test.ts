import { afterEach, describe, expect, it, vi } from "vitest";
import { requestJson } from "./transport";

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});
describe("local request lifecycle", () => {
  it("retries a transient read failure", async () => {
    const fetcher = vi
      .fn()
      .mockRejectedValueOnce(new TypeError("offline"))
      .mockResolvedValue(new Response('{"ok":true}'));
    vi.stubGlobal("fetch", fetcher);
    expect(await requestJson("http://localhost/test")).toEqual({ ok: true });
    expect(fetcher).toHaveBeenCalledTimes(2);
  });
  it("never retries writes or HTTP rejection", async () => {
    const fetcher = vi.fn().mockRejectedValue(new TypeError("offline"));
    vi.stubGlobal("fetch", fetcher);
    await expect(
      requestJson("http://localhost/test", { method: "POST" }),
    ).rejects.toThrow("无法连接");
    expect(fetcher).toHaveBeenCalledTimes(1);
    fetcher
      .mockReset()
      .mockResolvedValue(new Response('{"error":"conflict"}', { status: 409 }));
    await expect(requestJson("http://localhost/test")).rejects.toThrow(
      "conflict",
    );
    expect(fetcher).toHaveBeenCalledTimes(1);
  });
  it("aborts a hanging request at the deadline", async () => {
    vi.useFakeTimers();
    vi.stubGlobal(
      "fetch",
      vi.fn(
        (_url, init) =>
          new Promise((_resolve, reject) => {
            init.signal.addEventListener("abort", () =>
              reject(init.signal.reason),
            );
          }),
      ),
    );
    const pending = expect(
      requestJson("http://localhost/test", { timeoutMs: 100 }),
    ).rejects.toThrow("请求超时");
    await vi.advanceTimersByTimeAsync(100);
    await pending;
  });
  it("does not issue a request for an already cancelled operation", async () => {
    const fetcher = vi.fn();
    vi.stubGlobal("fetch", fetcher);
    const controller = new AbortController();
    controller.abort();
    await expect(
      requestJson("http://localhost/test", { signal: controller.signal }),
    ).rejects.toThrow();
    expect(fetcher).not.toHaveBeenCalled();
  });
});
