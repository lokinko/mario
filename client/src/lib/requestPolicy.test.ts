import { expect, it } from "vitest";
import { isDataWrite, requestPolicy } from "./requestPolicy";

it("keeps data updates behind revision checks and defaults unknown operations to writes", () => {
  expect(isDataWrite("/fx-settings/refresh", "POST")).toBe(true);
  expect(isDataWrite("/analysis/preview?source=test", "POST")).toBe(false);
  expect(isDataWrite("/model-config/test", "POST")).toBe(false);
  expect(isDataWrite("/future/preview", "POST")).toBe(true);
  expect(isDataWrite("/snapshot", "get")).toBe(false);
});
it("accepts only revisions corresponding to displayed data or completed ordinary writes", () => {
  expect(requestPolicy("/sync/version").acceptsRevision(false)).toBe(false);
  expect(requestPolicy("/analysis").acceptsRevision(true)).toBe(false);
  expect(requestPolicy("/snapshot").acceptsRevision(false)).toBe(true);
  expect(requestPolicy("/fx-settings?refresh=1").acceptsRevision(false)).toBe(
    true,
  );
  expect(requestPolicy("/holdings/a").acceptsRevision(true)).toBe(true);
  expect(requestPolicy("/analysis").timeoutMs).toBe(300000);
});
