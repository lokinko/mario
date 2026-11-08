import { useEffect, useRef, useState } from "react";
import { getDailyAssets } from "../../api/portfolio";
import type { DailyHistory, Snapshot } from "../../types";
import { SERVER_DATA_UPDATED, DATA_SAVED } from "../../lib/syncEvents";
export function useAssetHistory(snapshot: Snapshot) {
  const [history, setHistory] = useState<DailyHistory>();
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
    getDailyAssets({ limit: 30 })
      .then((data) => {
        if (current === request.current) setHistory(data);
      })
      .catch(() => {});
    return () => {
      request.current++;
    };
  }, [snapshot.updatedAt, refresh]);
  return history;
}
