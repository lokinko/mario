import { serviceUrl } from "./service";
const key = () => `mario.session:${serviceUrl()}`;
export const isWebApp = () =>
  !("__TAURI_INTERNALS__" in window) && !import.meta.env.DEV;
export function webToken(): string | undefined {
  const stored = localStorage.getItem(key());
  if (stored !== null) {
    try {
      const session = JSON.parse(stored);
      if (
        typeof session?.token === "string" &&
        session.token &&
        Number.isFinite(session.expiresAt) &&
        session.expiresAt * 1000 > Date.now()
      )
        return session.token;
    } catch {
      /* Discard unreadable sessions instead of blocking the login screen. */
    }
    setWebToken();
    return undefined;
  }
  // Preserve a login from an older build. The server still checks its original expiry.
  const legacy = sessionStorage.getItem(key()) ?? undefined;
  if (legacy) setWebToken(legacy);
  return legacy;
}
export function setWebToken(
  token?: string,
  expiresAt = Math.floor(Date.now() / 1000) + 7 * 24 * 3600,
) {
  if (token) localStorage.setItem(key(), JSON.stringify({ token, expiresAt }));
  else localStorage.removeItem(key());
  sessionStorage.removeItem(key());
}
