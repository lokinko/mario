import { afterEach, describe, expect, it, vi } from "vitest";
import { requestJson, requestEvents } from "./transport";

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe("streaming response lifecycle", () => {
  it("cancels a stalled body immediately when the user stops and never retries", async () => {
    const cancel = vi.fn();
    const fetcher = vi.fn().mockResolvedValue(
      new Response(new ReadableStream({ start() {}, cancel }), {
        headers: { "content-type": "text/event-stream" },
      }),
    );
    vi.stubGlobal("fetch", fetcher);
    const controller = new AbortController();
    const pending = expect(
      requestEvents(
        "/stream",
        { method: "POST", signal: controller.signal },
        () => undefined,
      ),
    ).rejects.toThrow();
    await vi.waitFor(() => expect(fetcher).toHaveBeenCalledTimes(1));
    controller.abort();
    await pending;
    expect(cancel).toHaveBeenCalledTimes(1);
    expect(fetcher).toHaveBeenCalledTimes(1);
  });
  it("decodes byte-split Chinese, multiline events and heartbeats before completion", async () => {
    let stream!: ReadableStreamDefaultController<Uint8Array>;
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        new Response(
          new ReadableStream({
            start(controller) {
              stream = controller;
            },
          }),
          { headers: { "content-type": "text/event-stream" } },
        ),
      ),
    );
    const events: unknown[] = [];
    const pending = requestEvents("/stream", {}, (event) => {
      events.push(event);
      return (event as { type: string }).type === "complete"
        ? "done"
        : undefined;
    });
    const bytes = new TextEncoder().encode(
      ': ping\r\n\r\ndata: {"type":"draft",\r\ndata: "answer":"中文回复"}\r\n\r\n',
    );
    for (const byte of bytes) stream.enqueue(new Uint8Array([byte]));
    await vi.waitFor(() =>
      expect(events).toEqual([{ type: "draft", answer: "中文回复" }]),
    );
    stream.enqueue(new TextEncoder().encode('data: {"type":"complete"}\n\n'));
    expect(await pending).toBe("done");
  });
  it("rejects truncated streams and never retries the POST", async () => {
    const fetcher = vi.fn().mockResolvedValue(
      new Response('data: {"type":"draft"}\n\n', {
        headers: { "content-type": "text/event-stream" },
      }),
    );
    vi.stubGlobal("fetch", fetcher);
    await expect(
      requestEvents("/stream", { method: "POST" }, () => undefined),
    ).rejects.toThrow("连接中断");
    expect(fetcher).toHaveBeenCalledTimes(1);
  });
  it("times out after headers when the body stalls", async () => {
    vi.useFakeTimers();
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        new Response(new ReadableStream({ start() {} }), {
          headers: { "content-type": "text/event-stream" },
        }),
      ),
    );
    const pending = expect(
      requestEvents("/stream", { timeoutMs: 100 }, () => undefined),
    ).rejects.toThrow("回复超时");
    await vi.advanceTimersByTimeAsync(100);
    await pending;
  });
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
