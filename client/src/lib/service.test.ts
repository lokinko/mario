// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { normalizeServerUrl, saveServerUrl, serviceUrl } from "./service";
import { setWebToken, webToken } from "./webSession";
afterEach(() => {
  localStorage.clear();
  sessionStorage.clear();
  vi.unstubAllEnvs();
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
    "https://example.com/unsafe%20path",
    "https://example.com/mario//api",
  ]) {
    expect(() => normalizeServerUrl(url)).toThrow();
  }
});
it("routes the mounted web app to its own API and isolates sessions from the root deployment", () => {
  vi.stubEnv("DEV", false);
  vi.stubEnv("VITE_API_URL", "");
  vi.stubEnv("BASE_URL", "/");
  localStorage.setItem("mario.serverUrl", "https://old.example.com/api");
  vi.stubEnv("BASE_URL", "/mario/");
  expect(serviceUrl()).toBe("/mario/api");
  setWebToken("mario-session");
  vi.stubEnv("BASE_URL", "/other/");
  expect(webToken()).toBeUndefined();
  vi.stubEnv("BASE_URL", "/mario/");
  expect(webToken()).toBe("mario-session");
});
it("accepts the public mount path for client connections", () => {
  for (const suffix of ["/mario", "/mario/", "/mario/api", "/mario/api/"])
    expect(normalizeServerUrl(`https://www.madeagents.ai${suffix}`)).toBe(
      "https://www.madeagents.ai/mario/api",
    );
});
it("never reuses one server's token with another server", () => {
  saveServerUrl("https://a.example.com");
  setWebToken("session-a");
  saveServerUrl("https://b.example.com");
  expect(webToken()).toBeUndefined();
  saveServerUrl("https://a.example.com");
  expect(webToken()).toBe("session-a");
});
