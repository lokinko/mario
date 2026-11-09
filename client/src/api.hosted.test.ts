import { currentSession } from "./lib/webSession";
// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  getSnapshot,
  getFxSettings,
  setFxOverride,
  saveProfile,
  updateHoldingAmount,
  saveHoldingTrade,
  deleteHolding,
  refreshSecurityPrices,
} from "./api/portfolio";
import { checkServerVersion, prepareDataReplacement } from "./api/account";
import { previewAnalysis } from "./api/research";
import { serviceUrl } from "./lib/service";
import { httpRequest } from "./lib/httpClient";
import { setWebToken, webToken } from "./lib/webSession";

import type { FinancialProfile } from "./types";
import type { AnalysisRequest } from "./types";

it("resolves a lost financial response without resubmitting the mutation", async () => {
  const current = { holdings: [], cashBalances: [{ amount: "900.01" }] };
  const fetcher = vi
    .fn()
    .mockRejectedValueOnce(new TypeError("connection reset after commit"))
    .mockResolvedValueOnce(
      response({ committed: true, snapshot: current }, "91"),
    );
  vi.stubGlobal("fetch", fetcher);
  const input = {
    requestId: "attempt",
    kind: "buy" as const,
    settlementMode: "history" as const,
    holdingId: "holding",
    expectedRevision: "before",
    newHolding: null,
    occurredOn: "2026-01-01",
    sequence: 1,
    quantity: "1",
    price: "100",
    grossAmount: "100",
    fees: "0",
    taxes: "0",
    account: "broker",
    externalId: "",
    note: "",
  };
  expect(await saveHoldingTrade(input)).toEqual(current);
  expect(fetcher).toHaveBeenCalledTimes(2);
  expect(String(fetcher.mock.calls[1][0])).toContain(
    "/financial-operations/resolve",
  );
  expect(JSON.parse(fetcher.mock.calls[1][1].body)).toEqual({
    method: "POST",
    path: "/api/holding-trades",
    input,
  });
  expect(currentSession().revision).toBe("91");
});

it("keeps an unresolved financial write pending instead of declaring failure or retrying it", async () => {
  const fetcher = vi
    .fn()
    .mockRejectedValueOnce(new TypeError("connection lost"))
    .mockResolvedValueOnce(response({ committed: false, snapshot: null }, "7"));
  vi.stubGlobal("fetch", fetcher);
  await expect(
    updateHoldingAmount("cash", "100.01", "old", "same-request"),
  ).rejects.toThrow("操作结果待确认");
  expect(fetcher).toHaveBeenCalledTimes(2);
});

it("resolves a lost whole-position deletion with its original versioned request", async () => {
  const current = {
    holdings: [],
    holdingTrades: [],
    cashBalances: [{ amount: "1000" }],
  };
  const fetcher = vi
    .fn()
    .mockRejectedValueOnce(new TypeError("connection reset after commit"))
    .mockResolvedValueOnce(
      response({ committed: true, snapshot: current }, "92"),
    );
  vi.stubGlobal("fetch", fetcher);
  expect(await deleteHolding("position", "position-v1")).toEqual(current);
  expect(fetcher).toHaveBeenCalledTimes(2);
  expect(fetcher.mock.calls[0][1].method).toBe("DELETE");
  expect(JSON.parse(fetcher.mock.calls[0][1].body)).toEqual({
    expectedRevision: "position-v1",
  });
  expect(String(fetcher.mock.calls[1][0])).toContain(
    "/financial-operations/resolve",
  );
  expect(JSON.parse(fetcher.mock.calls[1][1].body)).toEqual({
    method: "DELETE",
    path: "/api/holdings/position",
    input: { expectedRevision: "position-v1" },
  });
  expect(currentSession().revision).toBe("92");
});
vi.mock("./lib/service", async (original) => ({
  ...(await original<typeof import("./lib/service")>()),
}));
beforeEach(() => {
  currentSession().acceptRevision();
  currentSession().blockStaleWrites(false);
  setWebToken("test-session");
});
afterEach(() => {
  vi.useRealTimers();
  sessionStorage.clear();
  localStorage.clear();
  vi.unstubAllGlobals();
});

it("waits for a slow price refresh beyond 15 seconds and adopts its returned snapshot without resubmitting", async () => {
  vi.useFakeTimers();
  const result = { snapshot: { holdings: [] }, updatedCount: 1, errors: [] };
  const fetcher = vi.fn().mockImplementation(
    (_url, options) =>
      new Promise<Response>((resolve, reject) => {
        options.signal.addEventListener("abort", () =>
          reject(options.signal.reason),
        );
        setTimeout(() => resolve(response(result, "99")), 16000);
      }),
  );
  vi.stubGlobal("fetch", fetcher);
  const pending = expect(refreshSecurityPrices()).resolves.toEqual(result);
  await vi.advanceTimersByTimeAsync(16000);
  await pending;
  expect(fetcher).toHaveBeenCalledTimes(1);
  expect(currentSession().revision).toBe("99");
});

it("keeps the session after a temporary server failure", async () => {
  const expired = vi.fn();
  window.addEventListener("mario:session-expired", expired);
  try {
    vi.stubGlobal(
      "fetch",
      vi
        .fn()
        .mockResolvedValue(
          response({ error: "database unavailable" }, "", 500),
        ),
    );
    await expect(getSnapshot()).rejects.toThrow("database unavailable");
    expect(expired).not.toHaveBeenCalled();
    expect(webToken()).toBe("test-session");
  } finally {
    window.removeEventListener("mario:session-expired", expired);
  }
});

it("ignores a late 401 from an old session but expires the current session", async () => {
  const expired = vi.fn();
  window.addEventListener("mario:session-expired", expired);
  try {
    let complete!: (response: Response) => void;
    vi.stubGlobal(
      "fetch",
      vi.fn().mockImplementation(
        () =>
          new Promise<Response>((resolve) => {
            complete = resolve;
          }),
      ),
    );
    const pending = getSnapshot();
    setWebToken("new-session");
    complete(response({ error: "expired" }, "", 401));
    await expect(pending).rejects.toThrow("expired");
    expect(expired).not.toHaveBeenCalled();
    expect(webToken()).toBe("new-session");
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(response({ error: "expired" }, "", 401)),
    );
    await expect(getSnapshot()).rejects.toThrow("expired");
    expect(expired).toHaveBeenCalledTimes(1);
  } finally {
    window.removeEventListener("mario:session-expired", expired);
  }
});

it("returns to login if the token expires while a request is in flight", async () => {
  const expired = vi.fn();
  window.addEventListener("mario:session-expired", expired);
  try {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockImplementation(async () => {
        setWebToken("test-session", 0);
        return response({ error: "expired" }, "", 401);
      }),
    );
    await expect(getSnapshot()).rejects.toThrow("expired");
    expect(expired).toHaveBeenCalledTimes(1);
    expect(webToken()).toBeUndefined();
  } finally {
    window.removeEventListener("mario:session-expired", expired);
  }
});
const response = (body: unknown, revision: string, status = 200) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { "x-data-revision": revision },
  });
it("never sends a queued old-account edit using the new account's session", async () => {
  const fetcher = vi.fn();
  vi.stubGlobal("fetch", fetcher);
  currentSession().beginSync();
  const pending = saveProfile({} as FinancialProfile);
  setWebToken("new-session");
  currentSession().endSync();
  await expect(pending).rejects.toThrow("账号已切换");
  expect(fetcher).not.toHaveBeenCalled();
});

it("rejects late old-account data without changing the new account's revision", async () => {
  let complete!: (response: Response) => void;
  vi.stubGlobal(
    "fetch",
    vi.fn().mockImplementation(
      () =>
        new Promise<Response>((resolve) => {
          complete = resolve;
        }),
    ),
  );
  const pending = getSnapshot();
  setWebToken("new-session");
  currentSession().acceptRevision("new-account-revision");
  complete(response({ profile: "old-account-data" }, "old-account-revision"));
  await expect(pending).rejects.toThrow("账号已切换");
  expect(currentSession().revision).toBe("new-account-revision");
});

it("discards old-account amount edits waiting in the queue and allows new-account edits", async () => {
  let complete!: (response: Response) => void;
  const fetcher = vi
    .fn()
    .mockImplementationOnce(
      () =>
        new Promise<Response>((resolve) => {
          complete = resolve;
        }),
    )
    .mockResolvedValueOnce(response({}, "new-revision"));
  vi.stubGlobal("fetch", fetcher);
  const first = updateHoldingAmount("old-holding", 10, "old-revision", "first");
  const second = updateHoldingAmount(
    "old-holding",
    20,
    "old-revision",
    "second",
  );
  const firstRejected = expect(first).rejects.toThrow("账号已切换");
  const secondRejected = expect(second).rejects.toThrow("账号已切换");
  await vi.waitFor(() => expect(fetcher).toHaveBeenCalledTimes(1));
  setWebToken("new-session");
  const current = updateHoldingAmount(
    "new-holding",
    30,
    "new-revision",
    "third",
  );
  complete(response({}, "old-revision"));
  await firstRejected;
  await secondRejected;
  await current;
  expect(fetcher).toHaveBeenCalledTimes(2);
  expect(fetcher.mock.calls[1][0]).toBe(
    `${serviceUrl()}/holdings/new-holding/amount`,
  );
  expect(fetcher.mock.calls[1][1].headers.Authorization).toBe(
    "Bearer new-session",
  );
});
it("does not acknowledge a newer revision from a read-only AI preview", async () => {
  currentSession().acceptRevision("10");
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(response({}, "12")));
  await previewAnalysis({} as AnalysisRequest);
  expect(currentSession().revision).toBe("10");
});
it("does not acknowledge a snapshot whose response body failed to decode", async () => {
  currentSession().acceptRevision("10");
  vi.stubGlobal(
    "fetch",
    vi
      .fn()
      .mockResolvedValue(
        new Response('{"truncated":', { headers: { "x-data-revision": "12" } }),
      ),
  );
  await expect(getSnapshot()).rejects.toThrow();
  expect(currentSession().revision).toBe("10");
});
it("pins writes to displayed data and preserves the old revision on a conflict", async () => {
  const fetcher = vi
    .fn()
    .mockResolvedValueOnce(response({}, "10"))
    .mockResolvedValueOnce(response({ revision: "12" }, "12"))
    .mockResolvedValueOnce(response({ error: "conflict" }, "12", 409));
  vi.stubGlobal("fetch", fetcher);
  await getSnapshot();
  expect(currentSession().revision).toBe("10");
  expect((await checkServerVersion()).sync?.localUpdated).toBe(true);
  expect(currentSession().revision).toBe("10");
  await expect(saveProfile({} as FinancialProfile)).rejects.toThrow("conflict");
  expect(fetcher.mock.calls[2][1].headers["If-Match"]).toBe("10");
  expect(currentSession().revision).toBe("10");
});
it("keeps a local FX read from acknowledging unrelated remote portfolio changes", async () => {
  const fetcher = vi
    .fn()
    .mockResolvedValueOnce(response({}, "10"))
    .mockResolvedValueOnce(response({ baseCurrency: "CNY", quotes: [] }, "12"))
    .mockResolvedValueOnce(response({ revision: "12" }, "12"))
    .mockResolvedValueOnce(response({ error: "conflict" }, "12", 409));
  vi.stubGlobal("fetch", fetcher);
  await getSnapshot();
  await getFxSettings();
  expect(currentSession().revision).toBe("10");
  expect((await checkServerVersion()).sync?.localUpdated).toBe(true);
  await expect(saveProfile({} as FinancialProfile)).rejects.toThrow("conflict");
  expect(fetcher.mock.calls[3][1].headers["If-Match"]).toBe("10");
  expect(currentSession().revision).toBe("10");
});

it.each([
  "/market-data/fx-rate?fromCurrency=USD&toCurrency=CNY&onDate=2026-01-02",
  "/market-data/security-price?symbol=AAPL&onDate=2026-01-02",
])("keeps %s from acknowledging a newer account revision", async (path) => {
  const fetcher = vi
    .fn()
    .mockResolvedValueOnce(response({}, "10"))
    .mockResolvedValueOnce(response({ providerCode: "synthetic-quote" }, "12"))
    .mockResolvedValueOnce(response({ error: "conflict" }, "12", 409));
  vi.stubGlobal("fetch", fetcher);
  await getSnapshot();
  await httpRequest(path);
  expect(fetcher.mock.calls[1][0]).toBe(`${serviceUrl()}${path}`);
  expect(currentSession().revision).toBe("10");
  await expect(saveProfile({} as FinancialProfile)).rejects.toThrow("conflict");
  expect(fetcher.mock.calls[2][1].headers["If-Match"]).toBe("10");
  expect(currentSession().revision).toBe("10");
});

it("acknowledges an explicit FX save before loading its resulting portfolio snapshot", async () => {
  currentSession().acceptRevision("10");
  const fetcher = vi.fn().mockResolvedValue(response({}, "11"));
  vi.stubGlobal("fetch", fetcher);
  await setFxOverride("USD", "7.1");
  expect(fetcher.mock.calls[0][1].headers["If-Match"]).toBe("10");
  expect(currentSession().revision).toBe("11");
});

it("advances the revision after an acknowledged save and leaves unsent drafts blocked", async () => {
  currentSession().acceptRevision("10");
  const fetcher = vi.fn().mockResolvedValue(response({}, "11"));
  vi.stubGlobal("fetch", fetcher);
  await saveProfile({} as FinancialProfile);
  expect(currentSession().revision).toBe("11");
  currentSession().blockStaleWrites(true);
  await expect(saveProfile({} as FinancialProfile)).rejects.toThrow(
    "查看最新资料",
  );
  expect(fetcher).toHaveBeenCalledTimes(1);
});

it("pins replacement to its backup revision even if newer data was loaded", async () => {
  currentSession().acceptRevision("10");
  const fetcher = vi
    .fn()
    .mockResolvedValueOnce(response({ tables: ["backup"] }, "12"))
    .mockResolvedValueOnce(response({ error: "资料已变化" }, "13", 409));
  vi.stubGlobal("fetch", fetcher);
  const backup = await prepareDataReplacement();
  expect(backup.data).toEqual({ tables: ["backup"] });
  expect(currentSession().revision).toBe("10");
  currentSession().acceptRevision("13");
  await expect(backup.replace({ tables: ["replacement"] })).rejects.toThrow(
    "资料已变化",
  );
  expect(fetcher.mock.calls[1][0]).toBe(
    `${serviceUrl()}/data/import?replace=true`,
  );
  expect(fetcher.mock.calls[1][1].headers["If-Match"]).toBe("12");
  expect(JSON.parse(fetcher.mock.calls[1][1].body)).toEqual({
    tables: ["replacement"],
  });
  expect(currentSession().revision).toBe("13");
});

it("refuses replacement after switching accounts", async () => {
  const fetcher = vi.fn().mockResolvedValue(response({}, "12"));
  vi.stubGlobal("fetch", fetcher);
  const backup = await prepareDataReplacement();
  setWebToken("other-account");
  await expect(backup.replace({})).rejects.toThrow("账号已切换");
  expect(fetcher).toHaveBeenCalledOnce();
});

it("refuses to prepare replacement without a backup revision", async () => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("{}")));
  await expect(prepareDataReplacement()).rejects.toThrow(
    "未获取到备份的数据版本",
  );
});
