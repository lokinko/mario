import type { ReactNode } from "react";
import { Tag } from "antd";
export function Badge({
  children,
  tone = "neutral",
  title,
}: {
  children: ReactNode;
  tone?: "neutral" | "warning" | "success" | "info";
  title?: string;
}) {
  return (
    <Tag
      className={`badge badge-${tone}`}
      title={title}
      color={
        tone === "neutral" ? undefined : tone === "info" ? "processing" : tone
      }
    >
      {children}
    </Tag>
  );
}
