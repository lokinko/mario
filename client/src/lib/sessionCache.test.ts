// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { accountSession, resetAccountSession } from "./accountSession";
import {
  clearSessionCacheForToken,
  consumePrefetch,
  prefetchForSession,
  readSessionCache,
  writeSessionCache,
} from "./sessionCache";

afterEach(() => {
  sessionStorage.clear();
  resetAccountSession();
});

it("deduplicates and consumes prefetched promises within one account session", async () => {
  const session = accountSession("token-a", "/api");
  const loader = vi.fn().mockResolvedValue({ ok: true });
  const first = prefetchForSession(session, "watch:board", loader);
  const second = prefetchForSession(session, "watch:board", loader);
  expect(second).toBe(first);
  expect(loader).toHaveBeenCalledTimes(1);

  const consumed = await consumePrefetch(session, "watch:board", loader);
  expect(consumed).toEqual({ ok: true });
  expect(loader).toHaveBeenCalledTimes(1);

  // Subsequent reads fetch fresh data after the one-shot bootstrap handoff is consumed.
  await consumePrefetch(session, "watch:board", loader);
  expect(loader).toHaveBeenCalledTimes(2);
});

it("isolates sessionStorage snapshots per token and clears them on sign-out", () => {
  const alice = accountSession("token-alice", "/api");
  writeSessionCache(alice, "snapshot", { user: "alice" });
  expect(readSessionCache(alice, "snapshot")).toEqual({ user: "alice" });

  resetAccountSession();
  const bob = accountSession("token-bob", "/api");
  expect(readSessionCache(bob, "snapshot")).toBeNull();
  writeSessionCache(bob, "snapshot", { user: "bob" });

  clearSessionCacheForToken("/api", "token-alice");
  resetAccountSession();
  expect(
    readSessionCache(accountSession("token-alice", "/api"), "snapshot"),
  ).toBeNull();
  resetAccountSession();
  expect(
    readSessionCache(accountSession("token-bob", "/api"), "snapshot"),
  ).toEqual({ user: "bob" });
});

it("never persists cache entries when signed out", () => {
  const signedOut = accountSession(undefined, "/api");
  writeSessionCache(signedOut, "snapshot", { leaked: true });
  expect(sessionStorage.length).toBe(0);
  expect(readSessionCache(signedOut, "snapshot")).toBeNull();
});
