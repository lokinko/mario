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
