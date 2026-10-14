/** HTTP policy belongs to the transport boundary, not page or synchronization state. */
export function isDataWrite(path: string, method = "GET") {
  const resource = path.split("?")[0];
  return (
    method.toUpperCase() !== "GET" &&
    !["/analysis/preview", "/model-config/test"].includes(resource)
  );
}
export function requestPolicy(path: string) {
  const resource = path.split("?")[0];
  const timeoutMs = [
    "/analysis",
    "/analysis/stream",
    "/model-config/test",
    "/model-config/codex",
  ].includes(resource)
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
    acceptsRevision: (dataWrite: boolean) =>
      resource === "/snapshot" ||
      resource === "/fx-settings" ||
      (dataWrite && !["/analysis", "/analysis/stream"].includes(resource)),
  };
}
