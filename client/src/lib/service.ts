const serverKey = "mario.serverUrl";
export const isNative = () => "__TAURI_INTERNALS__" in window;
export const isLocalDev = () =>
  import.meta.env.DEV && !import.meta.env.VITE_API_URL && !isNative();
export function serviceUrl() {
  return (
    localStorage.getItem(serverKey) ??
    import.meta.env.VITE_API_URL ??
    (isLocalDev() ? "http://127.0.0.1:4217/api" : "/api")
  );
}
export function normalizeServerUrl(value: string) {
  const url = new URL(value.trim());
  if (
    url.protocol !== "https:" &&
    !(
      url.protocol === "http:" &&
      ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname)
    )
  )
    throw new Error("远程服务器必须使用 HTTPS");
  if (
    url.username ||
    url.password ||
    url.search ||
    url.hash ||
    !["", "/", "/api", "/api/"].includes(url.pathname)
  )
    throw new Error("请输入服务器地址，不要包含凭据、查询或额外路径");
  return `${url.origin}/api`;
}
export function saveServerUrl(url: string) {
  localStorage.setItem(serverKey, normalizeServerUrl(url));
}
let revision: string | undefined;
export function dataRevision() {
  return revision;
}
export function acceptRevision(value?: string) {
  revision = value;
}
