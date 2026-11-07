import { ConfigProvider, theme as antTheme } from "antd";
import zhCN from "antd/locale/zh_CN";
import { useEffect, useState, type ReactNode } from "react";

const darkQuery = "(prefers-color-scheme: dark)";

function prefersDark() {
  return window.matchMedia?.(darkQuery).matches ?? false;
}

// The appearance follows the system; tokens.css owns both palettes.
export function InterfaceProvider({ children }: { children: ReactNode }) {
  const [dark, setDark] = useState(prefersDark);
  useEffect(() => {
    const media = window.matchMedia?.(darkQuery);
    if (!media) return;
    const follow = () => setDark(media.matches);
    media.addEventListener("change", follow);
    return () => media.removeEventListener("change", follow);
  }, []);
  const styles = getComputedStyle(document.documentElement);
  const token = (name: string) => styles.getPropertyValue(name).trim();
  return (
    <ConfigProvider
      locale={zhCN}
      button={{ autoInsertSpace: false }}
      theme={{
        algorithm: dark ? antTheme.darkAlgorithm : antTheme.defaultAlgorithm,
        token: {
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
          fontSize: parseInt(token("--fs-body")),
          borderRadius: parseInt(token("--radius-sm")),
          borderRadiusLG: parseInt(token("--radius-md")),
          controlHeight: 36,
          boxShadow: token("--shadow-pop"),
          boxShadowSecondary: token("--shadow-pop"),
          motionEaseOut: token("--ease"),
        },
        components: {
          Button: { primaryShadow: "none", defaultShadow: "none" },
          Card: { headerFontSize: parseInt(token("--fs-title-sm")) },
          Tabs: { inkBarColor: token("--brand") },
        },
      }}
    >
      {children}
    </ConfigProvider>
  );
}
