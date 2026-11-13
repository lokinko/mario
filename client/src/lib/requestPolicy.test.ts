import { expect, it } from "vitest";
import { isDataWrite, requestPolicy } from "./requestPolicy";

it("keeps data updates behind revision checks and defaults unknown operations to writes", () => {
  expect(isDataWrite("/fx-settings/refresh", "POST")).toBe(true);
  expect(isDataWrite("/analysis/preview?source=test", "POST")).toBe(false);
  expect(isDataWrite("/model-config/test", "POST")).toBe(false);
  expect(isDataWrite("/activity", "POST")).toBe(false);
  expect(isDataWrite("/watch/catalog", "PUT")).toBe(false);
  expect(isDataWrite("/watch/catalog/resolve", "POST")).toBe(false);
  expect(isDataWrite("/watch/grid-preview", "POST")).toBe(false);
  expect(isDataWrite("/watch/600036.SS/grid", "PUT")).toBe(true);
  expect(requestPolicy("/watch/catalog/resolve").timeoutMs).toBe(30000);
  expect(isDataWrite("/watch/catalog/company", "PUT")).toBe(true);
  expect(isDataWrite("/future/preview", "POST")).toBe(true);
  expect(isDataWrite("/snapshot", "get")).toBe(false);
});
it("accepts only revisions corresponding to displayed data or completed ordinary writes", () => {
  expect(requestPolicy("/sync/version").acceptsRevision(false)).toBe(false);
  expect(requestPolicy("/analysis").acceptsRevision(true)).toBe(false);
  expect(requestPolicy("/snapshot").acceptsRevision(false)).toBe(true);
  expect(requestPolicy("/fx-settings?refresh=1").acceptsRevision(false)).toBe(
    false,
  );
  expect(requestPolicy("/fx-settings").acceptsRevision(true)).toBe(true);
  expect(requestPolicy("/fx-settings/refresh").acceptsRevision(true)).toBe(
    true,
  );
  expect(requestPolicy("/holdings/a").acceptsRevision(true)).toBe(true);
  expect(requestPolicy("/analysis").timeoutMs).toBe(1800000);
  expect(requestPolicy("/analysis/stream").timeoutMs).toBe(1800000);
  expect(requestPolicy("/analysis/stream").idleTimeoutMs).toBe(60000);
  expect(isDataWrite("/watch/catalog/refresh", "POST")).toBe(true);
  expect(requestPolicy("/watch/catalog/refresh").timeoutMs).toBe(180000);
  expect(requestPolicy("/market-data/security/refresh").timeoutMs).toBe(180000);
  expect(isDataWrite("/market-data/security/refresh", "POST")).toBe(true);
  expect(
    requestPolicy("/market-data/security-price?symbol=600941.SS").timeoutMs,
  ).toBe(180000);
  expect(requestPolicy("/holdings", "POST").timeoutMs).toBe(180000);
  expect(requestPolicy("/holdings/a", "PUT").timeoutMs).toBe(180000);
  expect(requestPolicy("/holdings/a/verified-valuation", "PUT").timeoutMs).toBe(
    180000,
  );
  expect(requestPolicy("/holdings/a", "DELETE").timeoutMs).toBe(15000);
});
