import {
  HttpError,
  requestJson,
  requestEvents,
  type RequestOptions,
} from "./transport";
import { isDataWrite, requestPolicy } from "./requestPolicy";
import { beforeDataWrite, markDataWrite, DATA_SAVED } from "./syncEvents";
import { serviceUrl, dataRevision, acceptRevision } from "./service";
import { webToken } from "./webSession";

export async function httpRequest<T>(
  path: string,
  init?: RequestInit,
  account = { token: webToken(), baseUrl: serviceUrl() },
  transport: (url: string, options: RequestOptions) => Promise<T> = requestJson,
): Promise<T> {
  const dataWrite = isDataWrite(path, init?.method);
  const release = dataWrite ? markDataWrite() : () => {};
  const { token: authToken, baseUrl } = account;
  const ensureCurrentAccount = () => {
    if (webToken() !== authToken || serviceUrl() !== baseUrl)
      throw new Error("账号已切换，请在当前账号中重新操作。");
  };
  try {
    if (dataWrite) await beforeDataWrite();
    ensureCurrentAccount();
    const baseRevision = dataRevision();
    const result = await transport(`${baseUrl}${path}`, {
      ...init,
      timeoutMs: requestPolicy(path).timeoutMs,
      onResponse: (response) => {
        if (webToken() !== authToken || serviceUrl() !== baseUrl) return;
        if (requestPolicy(path).acceptsRevision(dataWrite)) {
          acceptRevision(
            response.headers.get("x-data-revision") ?? baseRevision,
          );
        }
      },
      headers: {
        "Content-Type": "application/json",
        ...(baseRevision ? { "If-Match": baseRevision } : {}),
        ...(authToken ? { Authorization: `Bearer ${authToken}` } : {}),
        ...(init?.headers ?? {}),
      },
    });
    ensureCurrentAccount();
    if (dataWrite) window.dispatchEvent(new Event(DATA_SAVED));

    return result;
  } catch (error) {
    const resource = path.split("?")[0];
    const receiptable =
      (init?.method === "DELETE" && /^\/holdings\/[^/]+$/.test(resource)) ||
      resource === "/holding-trades" ||
      resource.startsWith("/holding-trades/") ||
      resource.endsWith("/amount") ||
      resource.endsWith("/trade-repair") ||
      resource.endsWith("/corporate-actions") ||
      resource.startsWith("/corporate-actions/");
    if (
      dataWrite &&
      receiptable &&
      (!(error instanceof HttpError) || error.status >= 500) &&
      typeof init?.body === "string"
    ) {
      ensureCurrentAccount();
      try {
        const resolved = await requestJson<{ committed: boolean; snapshot: T }>(
          `${baseUrl}/financial-operations/resolve`,
          {
            method: "POST",
            body: JSON.stringify({
              method: init.method,
              path: `/api${resource}`,
              input: JSON.parse(init.body),
            }),
            timeoutMs: 10000,
            headers: {
              "Content-Type": "application/json",
              ...(authToken ? { Authorization: `Bearer ${authToken}` } : {}),
            },
            onResponse: (response) => {
              ensureCurrentAccount();
              acceptRevision(
                response.headers.get("x-data-revision") ?? dataRevision(),
              );
            },
          },
        );
        ensureCurrentAccount();
        if (resolved.committed) {
          window.dispatchEvent(new Event(DATA_SAVED));
          return resolved.snapshot;
        }
      } catch {
        /* Keep the original attempt and request id; never guess or resubmit. */
      }
      ensureCurrentAccount();
      throw new Error(
        "操作结果待确认，原请求编号和输入已保留。可核对更正历史后使用相同内容重试；不要重新创建一笔交易。",
        { cause: error },
      );
    }
    if (error instanceof HttpError && error.status === 401) {
      const currentToken = webToken();
      if (!currentToken || currentToken === authToken)
        window.dispatchEvent(new Event("mario:session-expired"));
    }
    throw error;
  } finally {
    release();
  }
}

export async function httpStreamRequest<T>(
  path: string,
  init: RequestInit,
  onEvent: (event: unknown) => T | undefined,
): Promise<T> {
  const account = { token: webToken(), baseUrl: serviceUrl() };
  return httpRequest<T>(path, init, account, (url, options) =>
    requestEvents(url, options, (event) => {
      if (webToken() !== account.token || serviceUrl() !== account.baseUrl)
        throw new Error("账号已切换，请在当前账号中重新操作。");
      return onEvent(event);
    }),
  );
}
