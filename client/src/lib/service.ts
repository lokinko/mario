export const serviceUrlStorageKey = () =>
  `mario.serviceUrl:${import.meta.env.BASE_URL}`;

export const sameOriginServiceUrl = () => `${import.meta.env.BASE_URL}api`;
export function defaultServiceUrl() {
  return (
    (import.meta.env.VITE_API_URL || undefined) ??
    (import.meta.env.DEV ? "http://127.0.0.1:4217/api" : sameOriginServiceUrl())
  );
}
function storedServiceUrl() {
  try {
    const stored = localStorage.getItem(serviceUrlStorageKey());
    return stored ? normalizeServiceUrl(stored) : null;
  } catch {
    return null;
  }
}
export function serviceUrl() {
  return storedServiceUrl() ?? defaultServiceUrl();
}
export function hasCustomServiceUrl() {
  return storedServiceUrl() !== null;
}
/** Absolute http(s) address without query, fragment or trailing slash. */
export function normalizeServiceUrl(value: string) {
  let url: URL;
  try {
    url = new URL(value.trim(), window.location.origin);
  } catch {
    throw new Error("服务器地址格式无效");
  }
  if (url.protocol !== "http:" && url.protocol !== "https:")
    throw new Error("服务器地址须以 http:// 或 https:// 开头");
  return `${url.origin}${url.pathname.replace(/\/+$/, "")}`;
}
/** Sessions are keyed by address, so switching keeps each server's sign-in separate. */
export function setServiceUrl(value: string | null) {
  const next = value === null ? null : normalizeServiceUrl(value);
  if (next === null || next === normalizeServiceUrl(defaultServiceUrl()))
    localStorage.removeItem(serviceUrlStorageKey());
  else localStorage.setItem(serviceUrlStorageKey(), next);
}
