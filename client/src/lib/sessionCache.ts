import type { AccountSession } from "./accountSession";

const inflight = new WeakMap<AccountSession, Map<string, Promise<unknown>>>();
const CACHE_PREFIX = "mario.cache:";

function fingerprint(value: string): string {
  // Fast non-cryptographic hash so raw bearer tokens never appear in storage keys.
  let h1 = 0x811c9dc5;
  let h2 = 0x9e3779b9;
  for (let i = 0; i < value.length; i += 1) {
    const ch = value.charCodeAt(i);
    h1 = Math.imul(h1 ^ ch, 0x01000193);
    h2 = Math.imul(h2 ^ ch, 0x85ebca6b);
  }
  return `${(h1 >>> 0).toString(36)}_${(h2 >>> 0).toString(36)}`;
}

function accountPrefix(baseUrl: string, token: string): string {
  return `${CACHE_PREFIX}${fingerprint(`${baseUrl}\0${token}`)}:`;
}

function storageKey(session: AccountSession, key: string): string | null {
  if (!session.token) return null;
  return `${accountPrefix(session.baseUrl, session.token)}${key}`;
}

export function prefetchForSession<T>(
  session: AccountSession,
  key: string,
  loader: () => Promise<T>,
): Promise<T> {
  let map = inflight.get(session);
  if (!map) {
    map = new Map();
    inflight.set(session, map);
  }
  const existing = map.get(key) as Promise<T> | undefined;
  if (existing) return existing;
  const promise = loader().catch((error) => {
    if (map?.get(key) === promise) map.delete(key);
    throw error;
  });
  map.set(key, promise);
  return promise;
}

export function consumePrefetch<T>(
  session: AccountSession,
  key: string,
  loader: () => Promise<T>,
): Promise<T> {
  const map = inflight.get(session);
  const existing = map?.get(key) as Promise<T> | undefined;
  if (existing) {
    map?.delete(key);
    return existing;
  }
  return loader();
}

export function readSessionCache<T>(
  session: AccountSession,
  key: string,
): T | null {
  const fullKey = storageKey(session, key);
  if (!fullKey) return null;
  try {
    const raw = sessionStorage.getItem(fullKey);
    return raw ? (JSON.parse(raw) as T) : null;
  } catch {
    return null;
  }
}

export function writeSessionCache<T>(
  session: AccountSession,
  key: string,
  value: T,
): void {
  const fullKey = storageKey(session, key);
  if (!fullKey) return;
  try {
    sessionStorage.setItem(fullKey, JSON.stringify(value));
  } catch {
    /* Ignore quota or restricted storage errors. */
  }
}

export function clearSessionCacheForToken(
  baseUrl: string,
  token?: string,
): void {
  try {
    const prefix = token ? accountPrefix(baseUrl, token) : CACHE_PREFIX;
    const toRemove: string[] = [];
    for (let i = 0; i < sessionStorage.length; i += 1) {
      const k = sessionStorage.key(i);
      if (k && k.startsWith(prefix)) toRemove.push(k);
    }
    for (const k of toRemove) sessionStorage.removeItem(k);
  } catch {
    /* Ignore restricted storage errors. */
  }
}
