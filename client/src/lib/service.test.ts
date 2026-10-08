// @vitest-environment jsdom
import { afterEach, expect, it } from "vitest";
import { normalizeServerUrl, saveServerUrl } from "./service";
import { setWebToken, webToken } from "./webSession";
afterEach(() => {
  localStorage.clear();
  sessionStorage.clear();
});
it("normalizes server URLs and refuses remote cleartext or credential-bearing URLs", () => {
  expect(normalizeServerUrl("https://mario.example.com/api/")).toBe(
    "https://mario.example.com/api",
  );
  expect(normalizeServerUrl("http://127.0.0.1:4217")).toBe(
    "http://127.0.0.1:4217/api",
  );
  for (const url of [
    "http://remote.example.com",
    "https://user:pass@example.com",
    "https://example.com/?token=secret",
    "https://example.com/other",
  ]) {
    expect(() => normalizeServerUrl(url)).toThrow();
  }
});
it("never reuses one server's token with another server", () => {
  saveServerUrl("https://a.example.com");
  setWebToken("session-a");
  saveServerUrl("https://b.example.com");
  expect(webToken()).toBeUndefined();
  saveServerUrl("https://a.example.com");
  expect(webToken()).toBe("session-a");
});
