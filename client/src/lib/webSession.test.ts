// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { setWebToken, webToken } from "./webSession";

afterEach(() => {
  localStorage.clear();
  sessionStorage.clear();
  vi.useRealTimers();
  vi.unstubAllEnvs();
});

const configure = () => {
  vi.stubEnv("DEV", false);
  vi.stubEnv("VITE_API_URL", "");
  vi.stubEnv("BASE_URL", "/mario/");
};

it("migrates an existing tab session and restores it after reloading the module", async () => {
  configure();
  sessionStorage.setItem("mario.session:/mario/api", "legacy-token");
  expect(webToken()).toBe("legacy-token");
  expect(sessionStorage.getItem("mario.session:/mario/api")).toBeNull();
  sessionStorage.clear();
  vi.resetModules();
  const reloaded = await import("./webSession");
  expect(reloaded.webToken()).toBe("legacy-token");
  reloaded.setWebToken();
  expect(reloaded.webToken()).toBeUndefined();
});

it("uses the server expiry without extending it on reads or reviving old tab credentials", () => {
  configure();
  vi.useFakeTimers();
  vi.setSystemTime(new Date("2026-10-11T00:00:00Z"));
  setWebToken("current", Date.now() / 1000 + 60);
  const stored = localStorage.getItem("mario.session:/mario/api");
  vi.advanceTimersByTime(30000);
  expect(webToken()).toBe("current");
  expect(localStorage.getItem("mario.session:/mario/api")).toBe(stored);
  sessionStorage.setItem("mario.session:/mario/api", "old-tab-token");
  vi.advanceTimersByTime(30000);
  expect(webToken()).toBeUndefined();
  expect(webToken()).toBeUndefined();
  expect(localStorage.getItem("mario.session:/mario/api")).toBeNull();
  expect(sessionStorage.getItem("mario.session:/mario/api")).toBeNull();
});

it("discards damaged storage and allows a fresh login", () => {
  configure();
  for (const stored of ["{broken", "null", '{"token":"stale"}']) {
    localStorage.setItem("mario.session:/mario/api", stored);
    expect(webToken()).toBeUndefined();
  }
  setWebToken("fresh");
  expect(webToken()).toBe("fresh");
});
