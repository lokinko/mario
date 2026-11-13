import { useEffect, useRef } from "react";
import { recordActivity, type ActivityPage } from "../api/activity";

/** Observe foreground use; background sync and idle tabs never generate samples. */
export function useActivityTracking(page: ActivityPage) {
  const lastInteraction = useRef(Date.now());
  useEffect(() => {
    const controller = new AbortController();
    let pending = false;
    let sentMinute = -1;
    const sample = () => {
      const now = Date.now();
      const minute = Math.floor(now / 60000);
      if (
        document.visibilityState !== "visible" ||
        now - lastInteraction.current >= 300000 ||
        pending ||
        sentMinute === minute
      )
        return;
      pending = true;
      void recordActivity(page, controller.signal)
        .then(() => {
          sentMinute = minute;
        })
        .catch(() => {
          /* Sampling failure must not interrupt investment work. */
        })
        .finally(() => {
          pending = false;
        });
    };
    const interact = () => {
      lastInteraction.current = Date.now();
      sample();
    };
    const events = ["pointerdown", "keydown", "scroll", "touchstart"] as const;
    events.forEach((event) =>
      window.addEventListener(event, interact, {
        passive: true,
        capture: true,
      }),
    );
    document.addEventListener("visibilitychange", sample);
    sample();
    const timer = window.setInterval(sample, 30000);
    return () => {
      controller.abort();
      clearInterval(timer);
      events.forEach((event) =>
        window.removeEventListener(event, interact, true),
      );
      document.removeEventListener("visibilitychange", sample);
    };
  }, [page]);
}
