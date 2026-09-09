import { Check } from "lucide-react";

export function Toggle({
  icon,
  title,
  detail,
  checked,
  onChange,
}: {
  icon: React.ReactNode;
  title: string;
  detail: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <button
      className={`toggle-card ${checked ? "selected" : ""}`}
      onClick={() => onChange(!checked)}
    >
      <span className="toggle-icon">{icon}</span>
      <div>
        <strong>{title}</strong>
        <small>{detail}</small>
      </div>
      <i>{checked && <Check size={13} />}</i>
    </button>
  );
}
