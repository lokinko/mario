// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  getSnapshot,
  saveProfile,
  checkServerVersion,
  previewAnalysis,
  previewPortfolioEventImport,
  commitPortfolioEventImport,
} from "./api";
import { acceptRevision, dataRevision } from "./lib/service";
import { setWebToken } from "./lib/webSession";
import { blockStaleWrites } from "./lib/syncEvents";
import type { FinancialProfile } from "./types";
import type { AnalysisRequest } from "./types";
vi.mock("./lib/service", async (original) => ({
  ...(await original<typeof import("./lib/service")>()),
  isLocalDev: () => false,
}));
beforeEach(() => {
  acceptRevision();
  blockStaleWrites(false);
  setWebToken("test-session");
});
afterEach(() => {
  sessionStorage.clear();
  vi.unstubAllGlobals();
});
const response = (body: unknown, revision: string, status = 200) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { "x-data-revision": revision },
  });
it("does not acknowledge a newer revision from a read-only AI preview", async () => {
  acceptRevision("10");
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(response({}, "12")));
  await previewAnalysis({} as AnalysisRequest);
  expect(dataRevision()).toBe("10");
});
it("does not acknowledge a snapshot whose response body failed to decode", async () => {
  acceptRevision("10");
  vi.stubGlobal(
    "fetch",
    vi
      .fn()
      .mockResolvedValue(
        new Response('{"truncated":', { headers: { "x-data-revision": "12" } }),
      ),
  );
  await expect(getSnapshot()).rejects.toThrow();
  expect(dataRevision()).toBe("10");
});
it("pins writes to displayed data and preserves the old revision on a conflict", async () => {
  const fetcher = vi
    .fn()
    .mockResolvedValueOnce(response({}, "10"))
    .mockResolvedValueOnce(response({ revision: "12" }, "12"))
    .mockResolvedValueOnce(response({ error: "conflict" }, "12", 409));
  vi.stubGlobal("fetch", fetcher);
  await getSnapshot();
  expect(dataRevision()).toBe("10");
  expect((await checkServerVersion()).sync?.localUpdated).toBe(true);
  expect(dataRevision()).toBe("10");
  await expect(saveProfile({} as FinancialProfile)).rejects.toThrow("conflict");
  expect(fetcher.mock.calls[2][1].headers["If-Match"]).toBe("10");
  expect(dataRevision()).toBe("10");
});
it("advances the revision after an acknowledged save and leaves unsent drafts blocked", async () => {
  acceptRevision("10");
  const fetcher = vi.fn().mockResolvedValue(response({}, "11"));
  vi.stubGlobal("fetch", fetcher);
  await saveProfile({} as FinancialProfile);
  expect(dataRevision()).toBe("11");
  blockStaleWrites(true);
  await expect(saveProfile({} as FinancialProfile)).rejects.toThrow(
    "查看最新资料",
  );
  expect(fetcher).toHaveBeenCalledTimes(1);
});

it("acknowledges currency caching during CSV preview before committing its frozen rows", async () => {
  acceptRevision("10");
  const fetcher = vi
    .fn()
    .mockResolvedValueOnce(response({ previewRevision: "fx-rows" }, "12"))
    .mockResolvedValueOnce(response({ insertedCount: 1 }, "13"));
  vi.stubGlobal("fetch", fetcher);
  const preview = await previewPortfolioEventImport("csv");
  expect(dataRevision()).toBe("12");
  await commitPortfolioEventImport("csv", preview.previewRevision);
  expect(fetcher.mock.calls[0][1].headers["If-Match"]).toBe("10");
  expect(fetcher.mock.calls[1][1].headers["If-Match"]).toBe("12");
  expect(dataRevision()).toBe("13");
});
