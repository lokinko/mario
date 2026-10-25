import { Clock3 } from "lucide-react";
export function DataStamp({
  date,
  source,
}: {
  date?: string;
  source?: string;
}) {
  return (
    <span className="data-stamp">
      <Clock3 size={13} aria-hidden="true" />
      {date || "时点待补"}
      {source && ` · ${source}`}
    </span>
  );
}
