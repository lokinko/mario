import { useId, useState } from "react";
import type { DailyRecord } from "../../types";
import { formatMoney, formatSigned } from "../../lib/format";
export function AssetTrend({
  records,
  mini = false,
}: {
  records: DailyRecord[];
  mini?: boolean;
}) {
  const [active, setActive] = useState<number | null>(null);
  const id = useId().replace(/:/g, "");
  if (!mini && new Set(records.map((record) => record.baseCurrency)).size > 1) {
    const groups: DailyRecord[][] = [];
    for (const record of records) {
      if (groups.at(-1)?.[0].baseCurrency !== record.baseCurrency)
        groups.push([]);
      groups.at(-1)!.push(record);
    }
    return (
      <>
        {groups.map((group) => (
          <div key={group[0].day}>
            <small>{group[0].baseCurrency} 记账区间</small>
            <AssetTrend records={group} />
          </div>
        ))}
      </>
    );
  }
  const points = [...records].reverse();
  const values = points.flatMap((r) =>
    r.totalAssets == null ? [] : [r.totalAssets],
  );
  if (!values.length)
    return mini ? null : (
      <p className="empty">当前总值不可用，原币记录已保存。</p>
    );
  if (points.length < 2)
    return mini ? null : (
      <p className="daily-freshness">记录满两天后显示资产趋势。</p>
    );
  const min = Math.min(...values),
    max = Math.max(...values),
    span = Math.max(max - min, 1);
  const x = (i: number) => 20 + (i * 620) / Math.max(points.length - 1, 1);
  const y = (r: DailyRecord) =>
    130 - (((r.totalAssets ?? min) - min) / span) * 100;
  const selected = active == null ? undefined : points[active];
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
  const short = (value: number) =>
    value >= 10000
      ? `${(value / 10000).toFixed(1)}万`
      : value.toLocaleString("zh-CN", { maximumFractionDigits: 0 });
  return (
    <div className={mini ? "asset-sparkline" : "asset-trend"}>
      <svg
        viewBox={mini ? "0 12 660 132" : "0 0 720 174"}
        role={mini ? "img" : "group"}
        aria-label={
          mini
            ? "最近 30 日资产走势"
            : "每日总资产趋势，虚线表示沿用值，左右方向键逐日查看"
        }
        tabIndex={mini ? undefined : 0}
        onPointerMove={
          mini
            ? undefined
            : (event) => {
                const bounds = event.currentTarget.getBoundingClientRect();
                setActive(
                  Math.max(
                    0,
                    Math.min(
                      points.length - 1,
                      Math.round(
                        ((((event.clientX - bounds.left) / bounds.width) * 720 -
                          20) /
                          620) *
                          (points.length - 1),
                      ),
                    ),
                  ),
                );
              }
        }
        onPointerLeave={() => setActive(null)}
        onKeyDown={(event) => {
          if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key))
            return;
          event.preventDefault();
          setActive(
            event.key === "Home"
              ? 0
              : event.key === "End"
                ? points.length - 1
                : Math.max(
                    0,
                    Math.min(
                      points.length - 1,
                      (active ?? points.length - 1) +
                        (event.key === "ArrowLeft" ? -1 : 1),
                    ),
                  ),
          );
        }}
      >
        <defs>
          <linearGradient id={id} x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="var(--brand)" stopOpacity=".08" />
            <stop offset="100%" stopColor="var(--brand)" stopOpacity="0" />
          </linearGradient>
        </defs>
        {!mini && (
          <line
            x1="20"
            y1="140"
            x2="640"
            y2="140"
            stroke="var(--line)"
            strokeDasharray="4 4"
          />
        )}
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
            {r.totalAssets != null &&
              (mini ? i === points.length - 1 : points.length <= 100) && (
                <circle
                  cx={x(i)}
                  cy={y(r)}
                  r={mini ? 3 : 2.5}
                  fill={r.carried ? "var(--surface)" : "var(--brand)"}
                  stroke="var(--brand)"
                />
              )}
          </g>
        ))}
        {!mini && (
          <>
            <text x="20" y="166">
              {points[0].day}
            </text>
            <text x="640" y="166" textAnchor="end">
              {points.at(-1)?.day}
            </text>
            <text className="axis-money" x="710" y="34" textAnchor="end">
              {short(max)}
            </text>
            <text className="axis-money" x="710" y="134" textAnchor="end">
              {short(min)}
            </text>
          </>
        )}
        {!mini && selected?.totalAssets != null && active != null && (
          <>
            <line
              x1={x(active)}
              x2={x(active)}
              y1="20"
              y2="140"
              stroke="var(--line-strong)"
            />
            <circle cx={x(active)} cy={y(selected)} r="4" fill="var(--brand)" />
          </>
        )}
      </svg>
      {!mini && selected && (
        <div className="trend-tooltip" aria-live="polite">
          <strong>
            {selected.day} · {selected.baseCurrency}
          </strong>
          <span className="num" data-money>
            {selected.totalAssets == null
              ? "—"
              : formatMoney(
                  selected.exactTotalAssets ?? selected.totalAssets,
                  selected.baseCurrency,
                )}
          </span>
          <span className="num" data-money>
            较前一日{" "}
            {selected.previousDayChange == null
              ? "—"
              : formatSigned(selected.previousDayChange, (value) =>
                  formatMoney(value, selected.baseCurrency),
                )}
          </span>
          <small>{selected.carried ? "沿用值" : "有更新"}</small>
        </div>
      )}
      {!mini && (
        <p className="daily-freshness">
          实线：有更新 · 虚线：沿用值；不同记账币种分段显示
        </p>
      )}
    </div>
  );
}
