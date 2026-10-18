import { httpRequest } from "../lib/httpClient";
import { dataRevision, serviceUrl } from "../lib/service";
import { requestJson } from "../lib/transport";
import { webToken } from "../lib/webSession";
export async function checkServerVersion(): Promise<
  import("../types").AutoSyncResult
> {
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

/** Pin replacement to the account and revision captured by its rollback backup. */
export async function prepareDataReplacement() {
  const account = { token: webToken(), baseUrl: serviceUrl() };
  let revision: string | null = null;
  const data = await httpRequest<unknown>(
    "/data/export",
    undefined,
    account,
    (url, options) =>
      requestJson(url, {
        ...options,
        onResponse(response) {
          revision = response.headers.get("x-data-revision");
          options.onResponse?.(response);
        },
      }),
  );
  if (!revision) throw new Error("未获取到备份的数据版本，无法覆盖导入。");
  const backupRevision = revision;
  return {
    data,
    replace: (replacement: unknown) =>
      httpRequest(
        "/data/import?replace=true",
        {
          method: "POST",
          headers: { "If-Match": backupRevision },
          body: JSON.stringify(replacement),
        },
        account,
      ),
  };
}
