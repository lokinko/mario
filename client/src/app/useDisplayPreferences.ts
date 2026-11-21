import { useLayoutEffect, useState } from "react";
import type { ThemePreference } from "../components/DisplayPreferences";
const key = "mario.display";
export type { ThemePreference };
export function useDisplayPreferences() {
  const [preferences, setPreferences] = useState<{
    pnl: "cn" | "intl";
    maskAmounts: boolean;
    theme: ThemePreference;
  }>(() => {
    try {
      const saved = JSON.parse(localStorage.getItem(key) ?? "{}");
      return {
        pnl: saved.pnl === "intl" ? "intl" : "cn",
        maskAmounts: saved.maskAmounts === true,
        theme:
          saved.theme === "light" || saved.theme === "dark"
            ? saved.theme
            : "system",
      };
    } catch {
      return { pnl: "cn", maskAmounts: false, theme: "system" };
    }
  });
  useLayoutEffect(() => {
    document.documentElement.dataset.pnl = preferences.pnl;
    document.documentElement.dataset.theme = preferences.theme;
    document.documentElement.toggleAttribute(
      "data-mask-amounts",
      preferences.maskAmounts,
    );
    window.dispatchEvent(new Event("mario:theme-change"));
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
    setTheme: (theme: ThemePreference) =>
      setPreferences((current) => ({ ...current, theme })),
  };
}
