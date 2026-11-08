import { useEffect, useId, useRef, useState } from "react";
import type { CSSProperties, PointerEvent } from "react";
import type { Observation, WatchTrendPoint } from "../../api/watch";
import { number } from "./presentation";
import "./watch-trends.css";

type ValueKey =
  | "close"
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
}
const bands: Series[] = [
  { key: "close", label: "周收盘", color: "var(--text-1)" },
  {
    key: "monthlyMiddle",
    label: "月线中轨",
    color: "var(--warning)",
    dash: "7 4",
  },
  { key: "weeklyUpper", label: "周线上轨", color: "var(--info)" },
  { key: "weeklyMiddle", label: "周线中轨", color: "var(--brand)" },
  { key: "weeklyLower", label: "周线下轨", color: "var(--success)" },
];
const averages: Series[] = [
  { key: "close", label: "周收盘", color: "var(--text-1)", dash: "2 4" },
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
    let connected = false;
    return points
      .map((p) => {
        const value = p[key];
        if (!finite(value)) {
          connected = false;
          return "";
        }
        const command = connected ? "L" : "M";
        connected = true;
        return `${command}${x(p.day).toFixed(2)},${y(value).toFixed(2)}`;
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
          {series.map((s) => (
            <path
              key={s.key}
              data-series={s.key}
              d={path(s.key)}
              fill="none"
              stroke={s.color}
              strokeWidth={s.key === "close" ? 1.3 : 2}
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
          {series
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
      <div className="watch-trend-date">
        <label htmlFor={`${id}-date`}>查看日期</label>
        <strong>
          {point.day}
          {point.provisional ? " · 本周暂态" : ""}
        </strong>
        {cross && point.goldenCross && (
          <span>{point.provisional ? "暂态金叉" : "金叉"}</span>
        )}
      </div>
      <input
        id={`${id}-date`}
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
              <i style={{ borderTopStyle: s.dash ? "dashed" : "solid" }} />
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
  const [weeks, setWeeks] = useState(104);
  const trends = latest?.trends;
  const points = trends?.slice(-weeks) ?? [];
  const last = points[points.length - 1];
  const relation =
    last?.close != null && last.weeklyMiddle != null
      ? last.close > last.weeklyMiddle
        ? "周中轨上方 · 相对偏强"
        : last.close < last.weeklyMiddle
          ? "周中轨下方 · 相对偏弱"
          : "收盘位于周中轨"
      : "周中轨位置待补齐";
  const latestCross = trends
    ?.slice()
    .reverse()
    .find((p) => p.goldenCross);
  const maState =
    last?.ma20 != null && last.ma60 != null
      ? last.ma20 > last.ma60
        ? "20 周均线在 60 周上方"
        : last.ma20 < last.ma60
          ? "20 周均线在 60 周下方"
          : "20 周与 60 周均线重合"
      : "均线历史待补齐";
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
            </select>
          </label>
        )}
      </div>
      {!points.length ? (
        <p className="watch-meta">尚无趋势资料，下次成功检查收盘后补齐。</p>
      ) : (
        <>
          <p className="watch-meta">
            截至 {latest?.observedOn} · 复权序列统一到该日价格单位 · 按周取点
          </p>
          <TrendChart
            title="趋势图 · 月线中轨与周线 BOLL"
            points={points}
            series={bands}
          />
          <p className="watch-trend-summary">{relation}</p>
          <p className="watch-meta">
            周中轨上方可参考中轨向下观察回调档位；下方关注更低支撑。周线走平时，可观察上下轨区间作为网格波段参考；走平由图形观察，不自动判定。
          </p>
          <TrendChart
            title="金叉图 · 20 周与 60 周均线"
            points={points}
            series={averages}
            cross
          />
          <p className="watch-trend-summary">
            {maState}
            {last.provisional ? "（本周暂态）" : ""}
            {latestCross
              ? ` · 两年内最近${latestCross.provisional ? "暂态" : ""}金叉 ${latestCross.day}`
              : " · 有效相邻周中未见金叉"}
          </p>
          <p className="watch-meta">
            两条线均为对应周期的 BOLL 中轨（SMA）。金叉为上一周 20 周均线不高于
            60
            周、本周上穿。金叉前侧重网格波段、金叉后侧重趋势持仓，作为复盘思路，需结合基本面、现金与真实交易判断。
          </p>
          <details className="watch-method">
            <summary>趋势图计算口径</summary>
            <p>
              周线 BOLL(20,2) 使用 20 周收盘均值与两倍总体标准差；月线中轨为 20
              个月收盘均值，20/60
              周均线为简单移动平均。月线按日历月聚合，每个历史周点只使用当时已知的月内收盘，不使用之后的月末价格。
            </p>
            <p>
              月内、周内尚未结束时使用截至该点的最后收盘；最新周最后交易日在周五前时保守标为暂态。历史不足或有缺口的曲线断开，需分别积累
              20 月、20 周或 60
              周有效历史。曲线和金叉只供参考，不参与买点条件或变化提醒。
            </p>
          </details>
        </>
      )}
    </section>
  );
}
