import { httpRequest } from "../../lib/httpClient";
import { requestJson, HttpError } from "../../lib/transport";
import { serviceUrl } from "../../lib/service";
import { webToken } from "../../lib/webSession";
import type { Snapshot } from "../../types";

export class FinancialRequestError extends Error {
  readonly kind = "pending";
  constructor(cause?: unknown) {
    super(
      "操作结果待确认，原请求编号和输入已保留。可核对更正历史后使用相同内容重试；不要重新创建一笔交易。",
      { cause },
    );
    this.name = "FinancialRequestError";
  }
}

export function isPendingFinancialRequest(
  error: unknown,
): error is FinancialRequestError {
  return error instanceof FinancialRequestError && error.kind === "pending";
}

/** Reconcile a financial mutation's receipt while the normal write guard stays held. */
export function financialRequest(
  path: string,
  init: RequestInit,
  account = { token: webToken(), baseUrl: serviceUrl() },
): Promise<Snapshot> {
  const ensureCurrentAccount = () => {
    if (webToken() !== account.token || serviceUrl() !== account.baseUrl)
      throw new Error("账号已切换，请在当前账号中重新操作。");
  };
  return httpRequest<Snapshot>(path, init, account, async (url, options) => {
    try {
      return await requestJson<Snapshot>(url, options);
    } catch (error) {
      if (error instanceof HttpError && error.status < 500) throw error;
      ensureCurrentAccount();
      try {
        const resolved = await requestJson<{
          committed: boolean;
          snapshot: Snapshot;
        }>(`${account.baseUrl}/financial-operations/resolve`, {
          ...options,
          method: "POST",
          body: JSON.stringify({
            method: init.method,
            path: `/api${path.split("?")[0]}`,
            input: JSON.parse(String(init.body)),
          }),
          timeoutMs: 10000,
        });
        ensureCurrentAccount();
        if (resolved.committed) return resolved.snapshot;
      } catch (resolutionError) {
        // Authentication failures must still reach the shared session handler.
        if (
          resolutionError instanceof HttpError &&
          resolutionError.status === 401
        )
          throw resolutionError;
      }
      ensureCurrentAccount();
      throw new FinancialRequestError(error);
    }
  });
}
