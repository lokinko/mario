// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import {
  forgetWebAccount,
  savedWebAccounts,
  setWebToken,
  switchWebAccount,
  webToken,
} from "./webSession";

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

it("restores the current session after reloading the module", async () => {
  configure();
  setWebToken("current-token");
  vi.resetModules();
  const reloaded = await import("./webSession");
  expect(reloaded.webToken()).toBe("current-token");
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

it("switches between saved accounts across reloads without extending their expiry or storing passwords", async () => {
  configure();
  const expiresAt = Math.floor(Date.now() / 1000) + 3600;
  setWebToken("account-a", expiresAt, { email: "A@example.test", userId: "a" });
  setWebToken("account-b", expiresAt, { email: "b@example.test", userId: "b" });
  vi.resetModules();
  const reloaded = await import("./webSession");
  expect(reloaded.switchWebAccount("account-a")).toBe(true);
  expect(reloaded.webToken()).toBe("account-a");
  expect(
    reloaded.savedWebAccounts().find((a) => a.token === "account-a"),
  ).toEqual({
    token: "account-a",
    expiresAt,
    email: "a@example.test",
    userId: "a",
  });
  expect(reloaded.switchWebAccount("account-b")).toBe(true);
  expect(reloaded.webToken()).toBe("account-b");
  expect(Object.keys(reloaded.savedWebAccounts()[0]).sort()).toEqual([
    "email",
    "expiresAt",
    "token",
    "userId",
  ]);
});

it("forgets only the logged-out account, including a late logout of an inactive account", () => {
  configure();
  setWebToken("account-a");
  setWebToken("account-b");
  forgetWebAccount("account-a");
  expect(webToken()).toBe("account-b");
  expect(switchWebAccount("account-a")).toBe(false);
  forgetWebAccount("account-b");
  expect(webToken()).toBeUndefined();
  expect(savedWebAccounts()).toEqual([]);
});

it("drops expired saved sessions without logging out a different active account", () => {
  configure();
  vi.useFakeTimers();
  setWebToken("account-a", Date.now() / 1000 + 1);
  setWebToken("account-b", Date.now() / 1000 + 3600);
  vi.advanceTimersByTime(1000);
  expect(switchWebAccount("account-a")).toBe(false);
  expect(savedWebAccounts().map((a) => a.token)).toEqual(["account-b"]);
  expect(webToken()).toBe("account-b");
});

it("replaces an account's previous session when it logs in again", () => {
  configure();
  setWebToken("old", undefined, { email: "a@example.test", userId: "a" });
  setWebToken("new", undefined, { email: "a@example.test", userId: "a" });
  expect(savedWebAccounts().map((a) => a.token)).toEqual(["new"]);
  expect(switchWebAccount("old")).toBe(false);
});

it("isolates saved accounts by server and ignores damaged account lists", () => {
  configure();
  vi.stubEnv("VITE_API_URL", "https://a.example.test/api");
  setWebToken("account-a");
  vi.stubEnv("VITE_API_URL", "https://b.example.test/api");
  expect(savedWebAccounts()).toEqual([]);
  expect(switchWebAccount("account-a")).toBe(false);
  for (const damaged of [
    "{broken",
    "null",
    '[{"token":"expired","expiresAt":0}]',
  ]) {
    localStorage.setItem("mario.accounts:https://b.example.test/api", damaged);
    expect(savedWebAccounts()).toEqual([]);
  }
  setWebToken("account-b");
  vi.stubEnv("VITE_API_URL", "https://a.example.test/api");
  expect(savedWebAccounts().map((a) => a.token)).toEqual(["account-a"]);
});
