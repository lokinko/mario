import { useEffect, useId, useRef, useState } from "react";
import type { CSSProperties, PointerEvent } from "react";
import type { Observation, WatchTrendPoint } from "../../api/watch";
import { number } from "./presentation";
import "./watch-trends.css";

type ValueKey =
  | "weeklyLower"
  | "weeklyMiddle"
  | "weeklyUpper"
  | "monthlyMiddle"
  | "ma20"
  | "ma60";
interface Series {
  key: ValueKey;
  label: string;
  color: string;
  dash?: string;
  shade?: "upper" | "lower";
}
const bands: Series[] = [
  {
    key: "weeklyUpper",
    label: "周线上轨",
    color: "var(--info)",
    shade: "upper",
  },
  {
    key: "weeklyLower",
    label: "周线下轨",
    color: "var(--info)",
    shade: "lower",
  },
  { key: "weeklyMiddle", label: "周线中轨", color: "var(--info)" },
  {
    key: "monthlyMiddle",
    label: "月线中轨",
    color: "var(--brand)",
  },
];
const averages: Series[] = [
  { key: "ma20", label: "20 周均线", color: "var(--brand)" },
  { key: "ma60", label: "60 周均线", color: "var(--info)", dash: "7 4" },
];
const finite = (value: number | null): value is number =>
  value != null && Number.isFinite(value);
const height = 260;
const left = 58;
const top = 20;
const bottom = height - 34;
const timestamp = (day: string) => Date.parse(`${day}T00:00:00Z`);
// Catmull-Rom tension; below 1 rounds weekly corners without overshooting the data much.
const smoothing = 0.5;
type Point = [number, number];
const coordinate = ([px, py]: Point) => `${px.toFixed(2)},${py.toFixed(2)}`;

function curve(points: Point[]) {
  return points
    .map((p, i) => {
      if (!i) return coordinate(p);
      const before = points[Math.max(i - 2, 0)];
      const from = points[i - 1];
      const after = points[Math.min(i + 1, points.length - 1)];
      const t = smoothing / 6;
      const c1: Point = [
        from[0] + (p[0] - before[0]) * t,
        from[1] + (p[1] - before[1]) * t,
      ];
      const c2: Point = [
        p[0] - (after[0] - from[0]) * t,
        p[1] - (after[1] - from[1]) * t,
      ];
      return `C${coordinate(c1)} ${coordinate(c2)} ${coordinate(p)}`;
    })
    .join(" ");
}

function TrendChart({
  title,
  points,
  series,
  cross = false,
}: {
  title: string;
  points: WatchTrendPoint[];
  series: Series[];
  cross?: boolean;
}) {
  const id = useId();
  const host = useRef<HTMLElement>(null);
  const [width, setWidth] = useState(680);
  useEffect(() => {
    if (!host.current || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(([entry]) => {
      if (entry.contentRect.width > 0)
        setWidth(Math.max(240, Math.round(entry.contentRect.width)));
    });
    observer.observe(host.current);
    return () => observer.disconnect();
  }, []);
  const right = width - 16;
  const [selected, setSelected] = useState<string | null>(null);
  const selectedIndex = points.findIndex((point) => point.day === selected);
  const index = selectedIndex < 0 ? points.length - 1 : selectedIndex;
  const point = points[index];
  const lines = series.filter((s) => !s.shade);
  const shades = series.filter((s) => s.shade);
  const values = points.flatMap((p) =>
    series.flatMap((s) => (finite(p[s.key]) ? [p[s.key] as number] : [])),
  );
  const min = values.length ? Math.min(...values) : 0;
  const max = values.length ? Math.max(...values) : 1;
  const padding = Math.max((max - min) * 0.1, Math.abs(max) * 0.02, 0.01);
  const low = min - padding;
  const high = max + padding;
  const start = timestamp(points[0].day);
  const end = timestamp(points[points.length - 1].day);
  const x = (day: string) =>
    end === start
      ? (left + right) / 2
      : left + ((timestamp(day) - start) / (end - start)) * (right - left);
  const y = (value: number) =>
    bottom - ((value - low) / (high - low)) * (bottom - top);
  const path = (key: ValueKey) => {
    const segments: Point[][] = [];
    let segment: Point[] = [];
    for (const p of points) {
      const value = p[key];
      if (!finite(value)) {
        segment = [];
        continue;
      }
      if (!segment.length) segments.push(segment);
      segment.push([x(p.day), y(value)]);
    }
    return segments.map((s) => `M${curve(s)}`).join(" ");
  };
  const area = (edge: ValueKey) => {
    const segments: WatchTrendPoint[][] = [];
    let segment: WatchTrendPoint[] = [];
    for (const p of points) {
      if (finite(p[edge]) && finite(p.weeklyMiddle)) {
        if (!segment.length) segments.push(segment);
        segment.push(p);
      } else {
        segment = [];
      }
    }
    return segments
      .map((segment) => {
        const boundary = segment.map((p): Point => [x(p.day), y(p[edge]!)]);
        const middle = segment
          .slice()
          .reverse()
          .map((p): Point => [x(p.day), y(p.weeklyMiddle!)]);
        return `M${curve(boundary)} L${curve(middle)} Z`;
      })
      .join(" ");
  };
  const choose = (event: PointerEvent<SVGSVGElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    if (!rect.width) return;
    const position = ((event.clientX - rect.left) / rect.width) * width;
    const nearest = points.reduce((a, b) =>
      Math.abs(x(a.day) - position) < Math.abs(x(b.day) - position) ? a : b,
    );
    setSelected(nearest.day);
  };
  return (
    <section ref={host} className="watch-trend-chart" aria-labelledby={id}>
      <h4 id={id}>{title}</h4>
      {values.length ? (
        <svg
          viewBox={`0 0 ${width} ${height}`}
          role="img"
          aria-label={`${title}，${points[0].day} 至 ${points[points.length - 1].day}，单位元`}
          onPointerMove={choose}
          onPointerDown={choose}
          onPointerLeave={() => setSelected(null)}
        >
          <text x={left} y={12} className="watch-trend-axis">
            元
          </text>
          {[0, 1, 2, 3, 4].map((tick) => {
            const value = low + ((high - low) * tick) / 4;
            return (
              <g key={tick}>
                <line
                  x1={left}
                  x2={right}
                  y1={y(value)}
                  y2={y(value)}
                  className="watch-trend-grid"
                />
                <text
                  x={left - 8}
                  y={y(value) + 4}
                  textAnchor="end"
                  className="watch-trend-axis"
                >
                  {number(value)}
                </text>
              </g>
            );
          })}
          {[
            ...new Set([
              0,
              Math.floor((points.length - 1) / 2),
              points.length - 1,
            ]),
          ].map((i) => (
            <text
              key={i}
              x={x(points[i].day)}
              y={height - 10}
              textAnchor={
                i === 0 ? "start" : i === points.length - 1 ? "end" : "middle"
              }
              className="watch-trend-axis"
            >
              {points[i].day.slice(0, 7)}
            </text>
          ))}
          {shades.map((s) => (
            <path
              key={s.key}
              data-band={s.key}
              d={area(s.key)}
              className={`watch-trend-band watch-trend-band-${s.shade}`}
              fill={s.color}
              stroke="none"
            />
          ))}
          {lines.map((s) => (
            <path
              key={s.key}
              data-series={s.key}
              d={path(s.key)}
              fill="none"
              stroke={s.color}
              strokeWidth={cross ? 2 : 2.5}
              strokeDasharray={s.dash}
              vectorEffect="non-scaling-stroke"
            />
          ))}
          {cross &&
            points
              .filter((p) => p.goldenCross && finite(p.ma20))
              .map((p) => (
                <g
                  key={p.day}
                  className="watch-trend-cross"
                  aria-label={`${p.day} ${p.provisional ? "暂态金叉" : "金叉"}`}
                >
                  <circle cx={x(p.day)} cy={y(p.ma20!)} r={5} />
                  <title>
                    {p.day} ·{" "}
                    {p.provisional
                      ? "暂态金叉（本周未结束）"
                      : "20 周均线上穿 60 周均线"}
                  </title>
                </g>
              ))}
          <line
            x1={x(point.day)}
            x2={x(point.day)}
            y1={top}
            y2={bottom}
            className="watch-trend-cursor"
          />
          {lines
            .filter((s) => finite(point[s.key]))
            .map((s) => (
              <circle
                key={s.key}
                cx={x(point.day)}
                cy={y(point[s.key]!)}
                r={3}
                fill={s.color}
              />
            ))}
        </svg>
      ) : (
        <p className="watch-meta">当前没有可绘制的数据。</p>
      )}
      <input
        type="range"
        min={0}
        max={points.length - 1}
        value={index}
        disabled={points.length < 2}
        aria-label={`${title}查看日期`}
        aria-valuetext={point.day}
        onChange={(e) => setSelected(points[Number(e.target.value)].day)}
      />
      <dl className="watch-trend-legend">
        {series.map((s) => (
          <div
            key={s.key}
            style={{ "--series-color": s.color } as CSSProperties}
          >
            <dt>
              <i
                className={
                  s.shade
                    ? `watch-trend-band watch-trend-band-${s.shade}`
                    : undefined
                }
                style={
                  s.shade
                    ? { border: "none", background: s.color, height: 8 }
                    : { borderTopStyle: s.dash ? "dashed" : "solid" }
                }
              />
              {s.label}
            </dt>
            <dd>{number(point[s.key])}</dd>
          </div>
        ))}
      </dl>
    </section>
  );
}

export function WatchTrends({ latest }: { latest: Observation | null }) {
  const [weeks, setWeeks] = useState(0);
  const trends = latest?.trends;
  const points = trends?.slice(-weeks) ?? [];
  return (
    <section
      className="watch-detail-section watch-trends"
      aria-label="股价趋势"
    >
      <div className="watch-section-title">
        <h3>股价趋势</h3>
        {points.length > 0 && (
          <label>
            范围{" "}
            <select
              aria-label="趋势图范围"
              value={weeks}
              onChange={(e) => setWeeks(Number(e.target.value))}
            >
              <option value={26}>近半年</option>
              <option value={52}>近一年</option>
              <option value={104}>近两年</option>
              <option value={156}>近三年</option>
              <option value={0}>近五年</option>
            </select>
          </label>
        )}
      </div>
      {!points.length ? null : (
        <>
          <TrendChart
            title="趋势图 · 月线中轨与周线 BOLL"
            points={points}
            series={bands}
          />
          <TrendChart
            title="金叉图 · 20 周与 60 周均线"
            points={points}
            series={averages}
            cross
          />
        </>
      )}
    </section>
  );
}
