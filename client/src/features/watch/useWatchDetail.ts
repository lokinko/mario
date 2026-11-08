import { useEffect, useState } from "react";
import type { WatchMarketRow } from "../../api/watch";

/** Detail evidence belongs to this open company. Never reuse a late response for another row. */
export function useWatchDetail<T extends WatchMarketRow>(
  summary: T,
  symbol: string,
  load: (symbol: string, signal?: AbortSignal) => Promise<T>,
) {
  const [attempt, setAttempt] = useState(0);
  const [result, setResult] = useState<{
    summary: T;
    attempt: number;
    detail?: T;
    error?: string;
  } | null>(null);
  useEffect(() => {
    if (!summary.latest) return;
    const controller = new AbortController();
    void load(symbol, controller.signal).then(
      (detail) => {
        if (!controller.signal.aborted) setResult({ summary, attempt, detail });
      },
      (error: unknown) => {
        if (!controller.signal.aborted)
          setResult({ summary, attempt, error: String(error) });
      },
    );
    return () => controller.abort();
  }, [summary, symbol, load, attempt]);
  const current =
    result?.summary === summary && result.attempt === attempt ? result : null;
  return {
    row: current?.detail ?? summary,
    loading: !!summary.latest && !current,
    error: current?.error ?? "",
    retry: () => setAttempt((value) => value + 1),
  };
}
