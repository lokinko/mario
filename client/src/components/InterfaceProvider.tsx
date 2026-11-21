import { ConfigProvider, theme as antTheme } from "antd";
import zhCN from "antd/locale/zh_CN";
import { useEffect, useMemo, useState, type ReactNode } from "react";

const darkQuery = "(prefers-color-scheme: dark)";
const buttonConfig = { autoInsertSpace: false };

function resolveDark() {
  const mode = document.documentElement.dataset.theme;
  if (mode === "dark") return true;
  if (mode === "light") return false;
  return window.matchMedia?.(darkQuery).matches ?? false;
}

function readTokens() {
  const styles = getComputedStyle(document.documentElement);
  const token = (name: string) => styles.getPropertyValue(name).trim();
  return {
    colorPrimary: token("--brand"),
    colorInfo: token("--info"),
    colorSuccess: token("--success"),
    colorWarning: token("--warning"),
    colorError: token("--danger"),
    colorText: token("--text-1"),
    colorTextSecondary: token("--text-3"),
    colorTextTertiary: token("--text-3"),
    colorTextQuaternary: token("--text-4"),
    colorTextLightSolid: token("--on-brand"),
    colorBorder: token("--line-strong"),
    colorBorderSecondary: token("--line"),
    colorSplit: token("--line"),
    colorBgContainer: token("--surface"),
    colorBgElevated: token("--surface-raised"),
    colorBgLayout: token("--bg"),
    colorFillTertiary: token("--surface-sunken"),
    colorFillSecondary: token("--surface-hover"),
    fontFamily: token("--font-sans"),
    fontSize: parseInt(token("--fs-body")) || 14,
    borderRadius: parseInt(token("--radius-sm")) || 8,
    borderRadiusLG: parseInt(token("--radius-md")) || 12,
    controlHeight: 36,
    boxShadow: token("--shadow-pop"),
    boxShadowSecondary: token("--shadow-pop"),
    motionEaseOut: token("--ease"),
    cardHeaderFontSize: parseInt(token("--fs-title-sm")) || 16,
  };
}

// Appearance follows user preference or system; tokens.css owns both palettes.
export function InterfaceProvider({ children }: { children: ReactNode }) {
  const [dark, setDark] = useState(resolveDark);
  const [palette, setPalette] = useState(readTokens);
  useEffect(() => {
    let frame = 0;
    const sync = () => {
      setDark(resolveDark());
      window.cancelAnimationFrame(frame);
      frame = window.requestAnimationFrame(() => setPalette(readTokens()));
    };
    const media = window.matchMedia?.(darkQuery);
    media?.addEventListener("change", sync);
    window.addEventListener("mario:theme-change", sync);
    return () => {
      window.cancelAnimationFrame(frame);
      media?.removeEventListener("change", sync);
      window.removeEventListener("mario:theme-change", sync);
    };
  }, []);

  const themeConfig = useMemo(() => {
    const { cardHeaderFontSize, ...antTokens } = palette;
    return {
      algorithm: dark ? antTheme.darkAlgorithm : antTheme.defaultAlgorithm,
      token: antTokens,
      components: {
        Button: {
          primaryShadow: "none",
          defaultShadow: "none",
          fontWeight: 500,
        },
        Card: { headerFontSize: cardHeaderFontSize },
        Tabs: { inkBarColor: antTokens.colorPrimary },
      },
    };
  }, [dark, palette]);

  return (
    <ConfigProvider locale={zhCN} button={buttonConfig} theme={themeConfig}>
      {children}
    </ConfigProvider>
  );
}
