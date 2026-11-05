import { ConfigProvider } from "antd";
import zhCN from "antd/locale/zh_CN";
import type { ReactNode } from "react";

// CSS tokens also theme the existing domain controls during the visual migration.
export function InterfaceProvider({ children }: { children: ReactNode }) {
  const styles = getComputedStyle(document.documentElement);
  const token = (name: string) => styles.getPropertyValue(name).trim();
  return (
    <ConfigProvider
      locale={zhCN}
      button={{ autoInsertSpace: false }}
      theme={{
        token: {
          colorPrimary: token("--brand"),
          colorInfo: token("--info"),
          colorSuccess: token("--success"),
          colorWarning: token("--warning"),
          colorError: token("--danger"),
          colorText: token("--text-1"),
          colorTextSecondary: token("--text-3"),
          colorBorder: token("--line-strong"),
          colorBgContainer: token("--surface"),
          fontFamily: token("--font-sans"),
          fontSize: parseInt(token("--fs-body")),
          borderRadius: parseInt(token("--radius-md")),
          controlHeight: 36,
        },
        components: { Button: { primaryShadow: "none" } },
      }}
    >
      {children}
    </ConfigProvider>
  );
}
