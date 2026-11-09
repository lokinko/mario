export const sameOriginServiceUrl = () => `${import.meta.env.BASE_URL}api`;
export function serviceUrl() {
  return (
    (import.meta.env.VITE_API_URL || undefined) ??
    (import.meta.env.DEV ? "http://127.0.0.1:4217/api" : sameOriginServiceUrl())
  );
}
