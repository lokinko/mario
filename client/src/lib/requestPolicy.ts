/** HTTP policy belongs to the transport boundary, not page or synchronization state. */
export function isDataWrite(path: string, method = "GET") {
  const resource = path.split("?")[0];
  return (
    method.toUpperCase() !== "GET" &&
    !(method.toUpperCase() === "PUT" && resource === "/watch/catalog") &&
    ![
      "/analysis/preview",
      "/activity",
      "/model-config/test",
      "/financial-operations/resolve",
      "/watch/catalog/resolve",
      "/watch/grid-preview",
    ].includes(resource)
  );
}
export function requestPolicy(path: string, method = "GET") {
  const resource = path.split("?")[0];
  const preparesPrices =
    resource === "/market-data/security-price" ||
    resource.endsWith("/verified-valuation") ||
    (["POST", "PUT"].includes(method.toUpperCase()) &&
      (resource === "/holdings" || /^\/holdings\/[^/]+$/.test(resource)));
  const timeoutMs = ["/analysis", "/analysis/stream"].includes(resource)
    ? 1800000
    : resource === "/watch/catalog/resolve"
      ? 30000
      : resource === "/watch/refresh" ||
          resource === "/watch/catalog/refresh" ||
          resource === "/market-data/security/refresh" ||
          preparesPrices
        ? 180000
        : resource === "/model-config/test"
          ? 300000
          : resource === "/snapshot" ||
              resource.startsWith("/fx-settings") ||
              resource.startsWith("/market-data/fx-rate")
            ? 30000
            : resource.startsWith("/research-evidence") ||
                resource.startsWith("/data/")
              ? 60000
              : 15000;
  return {
    timeoutMs,
    ...(resource === "/analysis/stream" ? { idleTimeoutMs: 60000 } : {}),
    acceptsRevision: (dataWrite: boolean) =>
      resource === "/snapshot" ||
      (dataWrite && !["/analysis", "/analysis/stream"].includes(resource)),
  };
}
