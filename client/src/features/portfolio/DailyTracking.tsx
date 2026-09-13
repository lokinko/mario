import { useEffect, useState } from "react";
import { ensureDailyAssets } from "../../api";
import { CLOUD_DATA_UPDATED } from "../../lib/syncEvents";

export function DailyTracking() {
  const [error, setError] = useState("");
  const [retry, setRetry] = useState(0);
  useEffect(() => {
    let disposed = false,
      busy = false;
    let timezone = Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
    let lastDay = "";
    const localDay = () =>
      ["year", "month", "day"]
        .map(
          (key) =>
            new Intl.DateTimeFormat("en-US", {
              timeZone: timezone,
              year: "numeric",
              month: "2-digit",
              day: "2-digit",
            })
              .formatToParts(new Date())
              .find((part) => part.type === key)?.value,
        )
        .join("-");
    const ensure = async () => {
      if (
        busy ||
        document.visibilityState === "hidden" ||
        lastDay === localDay()
      )
        return;
      busy = true;
      try {
        const history = await ensureDailyAssets(timezone);
        timezone = history.timezone;
        if (!disposed) {
          lastDay = history.today;
          setError("");
        }
      } catch (e) {
        if (!disposed) setError(`每日记录暂未补齐：${String(e)}`);
      } finally {
        busy = false;
      }
    };
    const wake = () => void ensure();
    const remote = () => {
      lastDay = "";
      void ensure();
    };
    wake();
    const timer = window.setInterval(wake, 30000);
    window.addEventListener("focus", wake);
    window.addEventListener(CLOUD_DATA_UPDATED, remote);
    document.addEventListener("visibilitychange", wake);
    return () => {
      disposed = true;
      clearInterval(timer);
      window.removeEventListener("focus", wake);
      window.removeEventListener(CLOUD_DATA_UPDATED, remote);
      document.removeEventListener("visibilitychange", wake);
    };
  }, [retry]);
  return error ? (
    <div className="daily-tracking-error" role="status">
      {error}{" "}
      <button className="text-button" onClick={() => setRetry((v) => v + 1)}>
        重试
      </button>
    </div>
  ) : null;
}
