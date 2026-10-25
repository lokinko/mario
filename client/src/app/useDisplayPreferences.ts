import { useLayoutEffect, useState } from "react";
const key = "mario.display";
export function useDisplayPreferences() {
  const [preferences, setPreferences] = useState<{
    pnl: "cn" | "intl";
    maskAmounts: boolean;
  }>(() => {
    try {
      const saved = JSON.parse(localStorage.getItem(key) ?? "{}");
      return {
        pnl: saved.pnl === "intl" ? "intl" : "cn",
        maskAmounts: saved.maskAmounts === true,
      };
    } catch {
      return { pnl: "cn", maskAmounts: false };
    }
  });
  useLayoutEffect(() => {
    document.documentElement.dataset.pnl = preferences.pnl;
    document.documentElement.toggleAttribute(
      "data-mask-amounts",
      preferences.maskAmounts,
    );
    try {
      localStorage.setItem(key, JSON.stringify(preferences));
    } catch {
      /* Display preferences work even when storage is unavailable. */
    }
  }, [preferences]);
  return {
    ...preferences,
    setPnl: (pnl: "cn" | "intl") =>
      setPreferences((current) => ({ ...current, pnl })),
    setMaskAmounts: (maskAmounts: boolean) =>
      setPreferences((current) => ({ ...current, maskAmounts })),
  };
}
