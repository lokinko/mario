import { HttpError, requestJson } from "./transport";
import { isDataWrite, requestPolicy } from "./requestPolicy";
import { beforeDataWrite, markDataWrite, DATA_SAVED } from "./syncEvents";
import { serviceUrl, dataRevision, acceptRevision } from "./service";
import { webToken } from "./webSession";

export async function httpRequest<T>(
  path: string,
  init?: RequestInit,
  account = { token: webToken(), baseUrl: serviceUrl() },
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
    const result = await requestJson<T>(`${baseUrl}${path}`, {
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
