// @vitest-environment jsdom
import { currentSession } from "../lib/webSession";
import { accountSession, resetAccountSession } from "../lib/accountSession";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { runAnalysis } from "./research";

import { analysisSettings } from "../test/analysisSettings";

const session = vi.hoisted(() => ({ token: "account-a" }));
vi.mock("../lib/webSession", () => ({
  webToken: () => session.token,
  currentSession: () => accountSession(session.token, "http://localhost/api"),
}));
vi.mock("../lib/service", () => ({
  serviceUrl: () => "http://localhost/api",
}));
beforeEach(() => {
  resetAccountSession();
  currentSession().acceptRevision("revision-a");
});
afterEach(() => {
  resetAccountSession();
  vi.unstubAllGlobals();
  session.token = "account-a";
});

it("guards every stream update against account changes and releases the write guard", async () => {
  let stream!: ReadableStreamDefaultController<Uint8Array>;
  const fetcher = vi.fn().mockResolvedValue(
    new Response(
      new ReadableStream({
        start(controller) {
          stream = controller;
        },
      }),
      { headers: { "content-type": "text/event-stream" } },
    ),
  );
  vi.stubGlobal("fetch", fetcher);
  const updates = vi.fn();
  const pending = expect(
    runAnalysis({ ...analysisSettings, question: "问题" }, updates),
  ).rejects.toThrow("账号已切换");
  await vi.waitFor(() => expect(fetcher).toHaveBeenCalled());
  expect(currentSession().hasDataWrites()).toBe(true);
  const headers = fetcher.mock.calls[0][1].headers;
  expect(headers.Authorization).toBe("Bearer account-a");
  expect(headers["If-Match"]).toBe("revision-a");
  session.token = "account-b";
  stream.enqueue(
    new TextEncoder().encode(
      'data: {"type":"draft","answer":"旧账号回复"}\n\n',
    ),
  );
  await pending;
  expect(updates).not.toHaveBeenCalled();
  expect(currentSession().hasDataWrites()).toBe(false);
});

it("only completes and publishes the saved event after draft updates", async () => {
  const result = { id: "saved", answer: "完整回复" };
  vi.stubGlobal(
    "fetch",
    vi
      .fn()
      .mockResolvedValue(
        new Response(
          `data: ${JSON.stringify({ type: "draft", answer: "部分回复" })}\n\ndata: ${JSON.stringify({ type: "complete", result })}\n\n`,
          { headers: { "content-type": "text/event-stream" } },
        ),
      ),
  );
  const updates = vi.fn();
  expect(
    await runAnalysis({ ...analysisSettings, question: "问题" }, updates),
  ).toEqual(result);
  expect(updates).toHaveBeenCalledWith({ type: "draft", answer: "部分回复" });
  expect(currentSession().hasDataWrites()).toBe(false);
});
