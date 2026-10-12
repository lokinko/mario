export const isLocalDev = () =>
  import.meta.env.DEV && !import.meta.env.VITE_API_URL;
export const sameOriginServiceUrl = () => `${import.meta.env.BASE_URL}api`;
export function serviceUrl() {
  return (
    (import.meta.env.VITE_API_URL || undefined) ??
    (isLocalDev() ? "http://127.0.0.1:4217/api" : sameOriginServiceUrl())
  );
}
let revision: string | undefined;
export function dataRevision() {
  return revision;
}
export function acceptRevision(value?: string) {
  revision = value;
}
