// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { financialRequest } from "./financialRequest";
import { httpRequest } from "../../lib/httpClient";
import { acceptRevision, dataRevision } from "../../lib/service";
import { setWebToken } from "../../lib/webSession";
import {
  blockStaleWrites,
  hasDataWrites,
  DATA_SAVED,
} from "../../lib/syncEvents";

const input = {
  method: "PUT",
  body: JSON.stringify({ amount: "100.01", requestId: "original" }),
};
const reply = (body: unknown, status = 200, revision = "9") =>
  new Response(JSON.stringify(body), {
    status,
    headers: {
      "x-data-revision": revision,
      "Content-Type": "application/json",
    },
  });
beforeEach(() => {
  blockStaleWrites(false);
  setWebToken("first");
  acceptRevision("1");
});
afterEach(() => {
  blockStaleWrites(false);
  sessionStorage.clear();
  localStorage.clear();
  vi.unstubAllGlobals();
});

it("holds the write guard through receipt lookup and publishes one committed save", async () => {
  let resolve!: (response: Response) => void;
  let started!: () => void;
  const resolving = new Promise<void>((done) => {
    started = done;
  });
  const fetcher = vi
    .fn()
    .mockRejectedValueOnce(new TypeError("lost response"))
    .mockImplementationOnce(() => {
      started();
      return new Promise<Response>((done) => {
        resolve = done;
      });
    });
  vi.stubGlobal("fetch", fetcher);
  const saved = vi.fn();
  window.addEventListener(DATA_SAVED, saved);
  try {
    const pending = financialRequest("/holdings/cash/amount", input);
    await resolving;
    expect(hasDataWrites()).toBe(true);
    expect(saved).not.toHaveBeenCalled();
    resolve(reply({ committed: true, snapshot: { holdings: [] } }));
    await expect(pending).resolves.toEqual({ holdings: [] });
    expect(hasDataWrites()).toBe(false);
    expect(saved).toHaveBeenCalledTimes(1);
    expect(dataRevision()).toBe("9");
    expect(fetcher).toHaveBeenCalledTimes(2);
  } finally {
    window.removeEventListener(DATA_SAVED, saved);
  }
});

it("generic HTTP requests never initiate domain receipt lookups", async () => {
  const fetcher = vi.fn().mockRejectedValue(new TypeError("offline"));
  vi.stubGlobal("fetch", fetcher);
  await expect(httpRequest("/holdings/cash/amount", input)).rejects.toThrow(
    "无法连接服务",
  );
  expect(fetcher).toHaveBeenCalledTimes(1);
});

it.each([400, 401, 409, 422])(
  "does not recover a definitive HTTP %s rejection",
  async (status) => {
    const fetcher = vi
      .fn()
      .mockResolvedValue(reply({ error: "rejected" }, status));
    vi.stubGlobal("fetch", fetcher);
    await expect(
      financialRequest("/holdings/cash/amount", input),
    ).rejects.toThrow("rejected");
    expect(fetcher).toHaveBeenCalledTimes(1);
    expect(hasDataWrites()).toBe(false);
  },
);

it("rejects a blocked stale form before any mutation or receipt request", async () => {
  blockStaleWrites(true);
  const fetcher = vi.fn();
  vi.stubGlobal("fetch", fetcher);
  await expect(
    financialRequest("/holdings/cash/amount", input),
  ).rejects.toThrow("查看最新资料");
  expect(fetcher).not.toHaveBeenCalled();
  expect(hasDataWrites()).toBe(false);
});

it("does not start receipt recovery after the account changes", async () => {
  const fetcher = vi.fn().mockImplementation(async () => {
    setWebToken("second");
    acceptRevision("second-version");
    throw new TypeError("old response lost");
  });
  vi.stubGlobal("fetch", fetcher);
  await expect(
    financialRequest("/holdings/cash/amount", input),
  ).rejects.toThrow("账号已切换");
  expect(fetcher).toHaveBeenCalledTimes(1);
  expect(dataRevision()).toBe("second-version");
});

it("ignores a receipt and revision arriving after an account switch", async () => {
  const fetcher = vi
    .fn()
    .mockRejectedValueOnce(new TypeError("lost"))
    .mockImplementationOnce(async () => {
      setWebToken("second");
      acceptRevision("second-version");
      return reply({ committed: true, snapshot: { private: "first" } });
    });
  vi.stubGlobal("fetch", fetcher);
  await expect(
    financialRequest("/holdings/cash/amount", input),
  ).rejects.toThrow("账号已切换");
  expect(dataRevision()).toBe("second-version");
});

it("propagates session expiry during receipt recovery to the shared auth handler", async () => {
  const fetcher = vi
    .fn()
    .mockRejectedValueOnce(new TypeError("lost"))
    .mockResolvedValueOnce(reply({ error: "expired" }, 401));
  vi.stubGlobal("fetch", fetcher);
  const expired = vi.fn();
  window.addEventListener("mario:session-expired", expired);
  try {
    await expect(
      financialRequest("/holdings/cash/amount", input),
    ).rejects.toThrow("expired");
    expect(expired).toHaveBeenCalledTimes(1);
  } finally {
    window.removeEventListener("mario:session-expired", expired);
  }
});
