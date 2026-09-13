const key = "mario.webAccess";
export const isWebApp = () =>
  !("__TAURI_INTERNALS__" in window) && !import.meta.env.DEV;
export const webToken = () => sessionStorage.getItem(key) ?? undefined;
export function setWebToken(token?: string) {
  if (token) sessionStorage.setItem(key, token);
  else sessionStorage.removeItem(key);
}
