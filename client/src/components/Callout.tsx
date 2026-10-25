import type { ReactNode } from "react";
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
    <div
      className={`callout callout-${tone}`}
      role={tone === "danger" ? "alert" : "status"}
    >
      <Icon size={18} aria-hidden="true" />
      <div>
        {title && <strong>{title}</strong>}
        <div>{children}</div>
      </div>
      {action}
      {onClose && (
        <button
          type="button"
          className="icon-button"
          aria-label="关闭提示"
          onClick={onClose}
        >
          <X size={16} />
        </button>
      )}
    </div>
  );
}
