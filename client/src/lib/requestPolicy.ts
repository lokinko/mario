/** HTTP policy belongs to the transport boundary, not page or synchronization state. */
export function isDataWrite(path: string, method = "GET") {
  const resource = path.split("?")[0];
  return (
    method.toUpperCase() !== "GET" &&
    ![
      "/analysis/preview",
      "/model-config/test",
      "/financial-operations/resolve",
      // Shared catalogue writes use their own revision, independent of account data.
      "/watch/catalog",
      "/watch/catalog/resolve",
      "/watch/dividend-calendar/refresh",
    ].includes(resource)
  );
}
export function requestPolicy(path: string) {
  const resource = path.split("?")[0];
  const timeoutMs = ["/analysis", "/analysis/stream"].includes(resource)
    ? 1800000
    : resource === "/watch/catalog/resolve"
      ? 30000
      : resource === "/watch/refresh" ||
          resource === "/watch/catalog/refresh" ||
          resource === "/watch/dividend-calendar/refresh"
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
