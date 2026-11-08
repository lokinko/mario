import { useId } from "react";
import type { DailyRecord } from "../../types";
export function AssetTrend({ records }: { records: DailyRecord[] }) {
  const id = useId().replace(/:/g, "");
  const points = [...records].reverse();
  const values = points.flatMap((r) =>
    r.totalAssets == null ? [] : [r.totalAssets],
  );
  if (!values.length || points.length < 2) return null;
  const min = Math.min(...values),
    max = Math.max(...values),
    span = Math.max(max - min, 1);
  const x = (i: number) => 20 + (i * 620) / Math.max(points.length - 1, 1);
  const y = (r: DailyRecord) =>
    130 - (((r.totalAssets ?? min) - min) / span) * 100;
  const segments: DailyRecord[][] = [];
  points.forEach((point) => {
    if (point.totalAssets == null) {
      segments.push([]);
      return;
    }
    const last = segments.at(-1);
    if (!last?.length || last.at(-1)?.baseCurrency !== point.baseCurrency)
      segments.push([point]);
    else last.push(point);
  });
  return (
    <div className="asset-sparkline">
      <svg
        viewBox="0 12 660 132"
        role="img"
        aria-label="最近 30 日资产走势，虚线表示沿用值"
      >
        <defs>
          <linearGradient id={id} x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="var(--brand)" stopOpacity=".08" />
            <stop offset="100%" stopColor="var(--brand)" stopOpacity="0" />
          </linearGradient>
        </defs>
        {segments
          .filter((s) => s.length > 1)
          .map((segment, index) => (
            <path
              key={index}
              d={
                `M ${x(points.indexOf(segment[0]))} 140 ` +
                segment
                  .map((r) => `L ${x(points.indexOf(r))} ${y(r)}`)
                  .join(" ") +
                ` L ${x(points.indexOf(segment.at(-1)!))} 140 Z`
              }
              fill={`url(#${id})`}
            />
          ))}
        {points.map((r, i) => (
          <g key={r.day}>
            {i > 0 &&
              r.totalAssets != null &&
              points[i - 1].totalAssets != null &&
              r.baseCurrency === points[i - 1].baseCurrency && (
                <line
                  x1={x(i - 1)}
                  y1={y(points[i - 1])}
                  x2={x(i)}
                  y2={y(r)}
                  stroke="var(--brand)"
                  strokeWidth="1.5"
                  strokeDasharray={r.carried ? "5 4" : undefined}
                />
              )}
            {r.totalAssets != null && i === points.length - 1 && (
              <circle
                cx={x(i)}
                cy={y(r)}
                r={3}
                fill={r.carried ? "var(--surface)" : "var(--brand)"}
                stroke="var(--brand)"
              />
            )}
          </g>
        ))}
      </svg>
    </div>
  );
}
