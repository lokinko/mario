// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { serviceUrl } from "./service";
import { setWebToken, webToken } from "./webSession";

afterEach(() => {
  localStorage.clear();
  sessionStorage.clear();
  vi.unstubAllEnvs();
});
it("uses the mounted API, ignores old client server settings, and isolates sessions", () => {
  vi.stubEnv("DEV", false);
  vi.stubEnv("VITE_API_URL", "");
  vi.stubEnv("BASE_URL", "/");
  localStorage.setItem("mario.serverUrl", "https://old.example.com/api");
  expect(serviceUrl()).toBe("/api");
  vi.stubEnv("BASE_URL", "/mario/");
  localStorage.setItem(
    "mario.serverUrl:/mario/",
    "https://old.example.com/api",
  );
  expect(serviceUrl()).toBe("/mario/api");
  setWebToken("mario-session");
  vi.stubEnv("BASE_URL", "/other/");
  expect(webToken()).toBeUndefined();
  vi.stubEnv("BASE_URL", "/mario/");
  expect(webToken()).toBe("mario-session");
});
it("uses the local development API unless an explicit API URL is configured", () => {
  vi.stubEnv("DEV", true);
  vi.stubEnv("VITE_API_URL", "");
  expect(serviceUrl()).toBe("http://127.0.0.1:4217/api");
  vi.stubEnv("VITE_API_URL", "https://dev.example.com/mario/api");
  expect(serviceUrl()).toBe("https://dev.example.com/mario/api");
});
