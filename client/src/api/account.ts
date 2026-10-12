import { httpRequest } from "../lib/httpClient";
import { dataRevision, isLocalDev } from "../lib/service";
export async function checkServerVersion(): Promise<
  import("../types").AutoSyncResult
> {
  if (isLocalDev()) return { state: "disabled" };
  const baseline = dataRevision();
  const result = await httpRequest<{ revision: string }>("/sync/version");
  return {
    state: "synced",
    sync: { localUpdated: baseline !== result.revision },
  };
}

export function exportData() {
  return httpRequest<unknown>("/data/export");
}

export function importData(data: unknown) {
  return httpRequest("/data/import", {
    method: "POST",
    body: JSON.stringify(data),
  });
}
