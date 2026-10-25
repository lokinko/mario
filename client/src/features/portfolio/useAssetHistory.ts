import { useEffect, useRef, useState } from "react";
import { getDailyAssets } from "../../api/portfolio";
import type { DailyHistory, Snapshot } from "../../types";
import { SERVER_DATA_UPDATED, DATA_SAVED } from "../../lib/syncEvents";
export function useAssetHistory(snapshot: Snapshot) {
  const [history, setHistory] = useState<DailyHistory>();
  const [range, setRange] = useState(30);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const [refresh, setRefresh] = useState(0);
  const request = useRef(0);
  useEffect(() => {
    const wake = () => setRefresh((v) => v + 1);
    window.addEventListener(DATA_SAVED, wake);
    window.addEventListener(SERVER_DATA_UPDATED, wake);
    return () => {
      window.removeEventListener(DATA_SAVED, wake);
      window.removeEventListener(SERVER_DATA_UPDATED, wake);
    };
  }, []);
  useEffect(() => {
    const current = ++request.current;
    setLoading(true);
    setError("");
    getDailyAssets({ limit: range || 400 })
      .then((data) => {
        if (current !== request.current) return;
        setHistory(data);
      })
      .catch((e) => {
        if (current === request.current) setError(String(e));
      })
      .finally(() => {
        if (current === request.current) setLoading(false);
      });
    return () => {
      request.current++;
    };
  }, [range, snapshot.updatedAt, refresh]);
  const loadMore = async () => {
    if (!history?.nextBefore) return;
    const current = ++request.current;
    setLoading(true);
    try {
      const page = await getDailyAssets({
        limit: 400,
        before: history.nextBefore,
      });
      if (current === request.current)
        setHistory({ ...page, records: [...history.records, ...page.records] });
    } catch (e) {
      if (current === request.current) setError(String(e));
    } finally {
      if (current === request.current) setLoading(false);
    }
  };
  return {
    history,
    range,
    setRange,
    error,
    loading,
    loadMore,
    retry: () => setRefresh((v) => v + 1),
  };
}
