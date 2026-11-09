import {
  HttpError,
  requestJson,
  requestEvents,
  type RequestOptions,
} from "./transport";
import { isDataWrite, requestPolicy } from "./requestPolicy";
import { DATA_SAVED } from "./syncEvents";
import { serviceUrl } from "./service";
import { currentSession, webToken } from "./webSession";

export async function httpRequest<T>(
  path: string,
  init?: RequestInit,
  account = { token: webToken(), baseUrl: serviceUrl() },
  transport: (url: string, options: RequestOptions) => Promise<T> = requestJson,
): Promise<T> {
  const session = currentSession();
  const dataWrite = isDataWrite(path, init?.method);
  const release = dataWrite ? session.markDataWrite() : () => {};
  const { token: authToken, baseUrl } = account;
  const ensureCurrentAccount = () => {
    session.assertActive();
    if (
      currentSession() !== session ||
      session.token !== authToken ||
      session.baseUrl !== baseUrl
    )
      throw new Error("账号已切换，请在当前账号中重新操作。");
  };
  try {
    if (dataWrite) await session.beforeDataWrite();
    ensureCurrentAccount();
    const baseRevision = session.revision;
    const result = await transport(`${baseUrl}${path}`, {
      ...init,
      signal: init?.signal
        ? AbortSignal.any([init.signal, session.signal])
        : session.signal,
      timeoutMs: requestPolicy(path, init?.method).timeoutMs,
      onResponse: (response) => {
        if (currentSession() !== session || session.signal.aborted) return;
        if (requestPolicy(path).acceptsRevision(dataWrite)) {
          session.acceptRevision(
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
      const alreadyDisposed = session.signal.aborted;
      const currentToken = webToken();
      if (!currentToken || (!alreadyDisposed && currentToken === authToken))
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
  const session = currentSession();
  const account = { token: webToken(), baseUrl: serviceUrl() };
  return httpRequest<T>(path, init, account, (url, options) =>
    requestEvents(url, options, (event) => {
      if (currentSession() !== session || session.signal.aborted)
        throw new Error("账号已切换，请在当前账号中重新操作。");
      return onEvent(event);
    }),
  );
}
