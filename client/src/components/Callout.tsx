import type { ReactNode } from "react";
import { Alert } from "antd";
import { AlertTriangle, CheckCircle2, Info, X, XCircle } from "lucide-react";
export function Callout({
  tone = "info",
  title,
  children,
  action,
  onClose,
}: {
  tone?: "info" | "success" | "warning" | "danger";
  title?: string;
  children: ReactNode;
  action?: ReactNode;
  onClose?: () => void;
}) {
  const Icon = {
    info: Info,
    success: CheckCircle2,
    warning: AlertTriangle,
    danger: XCircle,
  }[tone];
  return (
    <Alert
      className={`callout callout-${tone}`}
      role={tone === "danger" ? "alert" : "status"}
      type={tone === "danger" ? "error" : tone}
      showIcon
      icon={<Icon size={18} aria-hidden="true" />}
      title={title || children}
      description={title ? children : undefined}
      action={action}
      closable={
        onClose
          ? { onClose, "aria-label": "关闭提示", closeIcon: <X size={16} /> }
          : false
      }
    />
  );
}
