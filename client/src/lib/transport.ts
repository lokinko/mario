export class HttpError extends Error {
  constructor(
    message: string,
    readonly status: number,
  ) {
    super(message);
    this.name = "HttpError";
  }
}

export interface RequestOptions extends RequestInit {
  timeoutMs?: number;
  onResponse?: (response: Response) => void;
}

/** Retry only read requests, within one deadline. Never replay a mutation. */
export async function requestJson<T>(
  url: string,
  options: RequestOptions = {},
): Promise<T> {
  const { timeoutMs = 15000, signal, onResponse, ...init } = options;
  const controller = new AbortController();
  const abort = () => controller.abort(signal?.reason);
  const timer = setTimeout(
    () =>
      controller.abort(
        new Error(
          "请求超时。写入可能已经完成，请先刷新核实结果，不要重复提交。",
        ),
      ),
    timeoutMs,
  );
  signal?.addEventListener("abort", abort, { once: true });
  if (signal?.aborted) abort();
  const attempts = !init.method || init.method.toUpperCase() === "GET" ? 3 : 1;
  try {
    for (let attempt = 0; attempt < attempts; attempt++) {
      controller.signal.throwIfAborted();
      let response: Response;
      try {
        response = await fetch(url, { ...init, signal: controller.signal });
      } catch (error) {
        controller.signal.throwIfAborted();
        if (attempt === attempts - 1)
          throw new Error("无法连接服务，请检查服务是否正在运行", {
            cause: error,
          });
        await new Promise((resolve) =>
          setTimeout(resolve, 150 * (attempt + 1)),
        );
        continue;
      }
      if (!response.ok) {
        const body = await response.text();
        let message = body;
        try {
          message = (JSON.parse(body) as { error?: string }).error || body;
        } catch {
          /* Non-JSON provider failure. */
        }
        throw new HttpError(
          message || `服务返回 ${response.status}`,
          response.status,
        );
      }
      if (response.status === 204) {
        onResponse?.(response);
        return undefined as T;
      }
      const result = (await response.json()) as T;
      onResponse?.(response);
      return result;
    }
    throw new Error("无法连接服务");
  } finally {
    clearTimeout(timer);
    signal?.removeEventListener("abort", abort);
  }
}
