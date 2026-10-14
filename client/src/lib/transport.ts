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

/** POST SSE with one deadline for headers and body; never replay a generation. */
export async function requestEvents<T>(
  url: string,
  options: RequestOptions,
  onEvent: (event: unknown) => T | undefined,
): Promise<T> {
  const { timeoutMs = 300000, signal, onResponse, ...init } = options;
  const controller = new AbortController();
  const abort = () => controller.abort(signal?.reason);
  const timer = setTimeout(
    () =>
      controller.abort(
        new Error("回复超时，已显示的内容尚未完成。请先查看历史记录再重试。"),
      ),
    timeoutMs,
  );
  signal?.addEventListener("abort", abort, { once: true });
  if (signal?.aborted) abort();
  let reader: ReadableStreamDefaultReader<Uint8Array> | undefined;
  const cancel = () => {
    void reader?.cancel().catch(() => {});
  };
  controller.signal.addEventListener("abort", cancel, { once: true });
  try {
    controller.signal.throwIfAborted();
    const response = await fetch(url, { ...init, signal: controller.signal });
    if (!response.ok) {
      let message = `服务返回 ${response.status}`;
      try {
        message = (await response.json()).error || message;
      } catch {
        /* Keep a safe HTTP message. */
      }
      throw new HttpError(message, response.status);
    }
    if (
      !response.body ||
      !response.headers.get("content-type")?.includes("text/event-stream")
    )
      throw new Error("服务未返回流式回复，请更新服务后重试。");
    reader = response.body.getReader();
    const decoder = new TextDecoder();
    let buffer = "";
    let data: string[] = [];
    while (true) {
      controller.signal.throwIfAborted();
      const { value, done } = await reader.read();
      controller.signal.throwIfAborted();
      buffer += done
        ? decoder.decode()
        : decoder.decode(value, { stream: true });
      if (buffer.length > 4 * 1024 * 1024) throw new Error("流式响应过大");
      let end: number;
      while ((end = buffer.indexOf("\n")) !== -1) {
        const line = buffer.slice(0, end).replace(/\r$/, "");
        buffer = buffer.slice(end + 1);
        if (!line) {
          if (!data.length) continue;
          const event = JSON.parse(data.join("\n"));
          data = [];
          const result = onEvent(event);
          if (result !== undefined) {
            onResponse?.(response);
            return result;
          }
        } else if (line.startsWith("data:")) {
          data.push(line.slice(5).replace(/^ /, ""));
          if (
            data.reduce((sum, item) => sum + item.length, 0) >
            4 * 1024 * 1024
          )
            throw new Error("流式事件过大");
        }
      }
      if (done)
        throw new Error(
          "回复连接中断，已显示的内容尚未完成。请先查看历史记录再重试。",
        );
    }
  } catch (error) {
    controller.signal.throwIfAborted();
    throw error;
  } finally {
    clearTimeout(timer);
    signal?.removeEventListener("abort", abort);
    controller.signal.removeEventListener("abort", cancel);
    await reader?.cancel().catch(() => {});
    reader?.releaseLock();
  }
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
