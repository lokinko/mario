export interface RequestOptions extends RequestInit {
  timeoutMs?: number;
}

/** Retry only read requests, within one deadline. Never replay a mutation. */
export async function requestJson<T>(
  url: string,
  options: RequestOptions = {},
): Promise<T> {
  const { timeoutMs = 15000, signal, ...init } = options;
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
          throw new Error("无法连接本地服务，请检查客户端是否正常运行", {
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
        throw new Error(message || `本地服务返回 ${response.status}`);
      }
      if (response.status === 204) return undefined as T;
      return (await response.json()) as T;
    }
    throw new Error("无法连接本地服务");
  } finally {
    clearTimeout(timer);
    signal?.removeEventListener("abort", abort);
  }
}
