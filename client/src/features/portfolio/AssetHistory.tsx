import { useEffect, useRef, useState } from "react";
import { getDailyAssets } from "../../api/portfolio";
import type { DailyHistory, DailyRecord, Snapshot } from "../../types";
import { formatMoney } from "../../lib/format";
import { SERVER_DATA_UPDATED, DATA_SAVED } from "../../lib/syncEvents";

const money = (value: number | null, currency: string) =>
  value == null ? "总额待补全" : formatMoney(value, currency);
const change = (value: number | null, currency: string) =>
  value == null
    ? "暂不可比较"
    : `${value > 0 ? "+" : ""}${formatMoney(value, currency)}`;

function Trend({ records }: { records: DailyRecord[] }) {
  const segments: DailyRecord[][] = [];
  for (const record of records) {
    if (segments.at(-1)?.[0].baseCurrency !== record.baseCurrency)
      segments.push([]);
    segments.at(-1)!.push(record);
  }
  if (segments.length > 1)
    return (
      <>
        {segments.map((segment) => (
          <div key={segment[0].day}>
            <small>{segment[0].baseCurrency} 记账区间</small>
            <Trend records={segment} />
          </div>
        ))}
      </>
    );
  const points = [...records].reverse();
  const values = points.flatMap((r) =>
    r.totalAssets == null ? [] : [r.totalAssets],
  );
  if (!values.length)
    return <p className="empty">补齐汇率后显示总资产曲线，原币记录已保存。</p>;
  if (points.length < 2)
    return <p className="daily-freshness">记录满两天后显示资产趋势。</p>;
  const min = Math.min(...values),
    max = Math.max(...values),
    span = Math.max(max - min, 1);
  const x = (i: number) => 36 + (i * 648) / Math.max(points.length - 1, 1);
  const y = (r: DailyRecord) =>
    130 - (((r.totalAssets ?? min) - min) / span) * 92;
  return (
    <div className="asset-trend">
      <svg
        viewBox="0 0 720 174"
        role="group"
        aria-label="每日总资产趋势，虚线表示沿用值"
      >
        <line
          x1="36"
          y1="140"
          x2="684"
          y2="140"
          stroke="currentColor"
          opacity=".15"
        />
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
                  stroke="var(--accent, #24876b)"
                  strokeWidth="2.5"
                  strokeDasharray={r.carried ? "5 4" : undefined}
                />
              )}
            {r.totalAssets != null && (
              <circle
                cx={x(i)}
                cy={y(r)}
                r={points.length > 100 ? 2 : 4}
                fill={
                  r.carried ? "var(--surface, #fff)" : "var(--accent, #24876b)"
                }
                stroke="var(--accent, #24876b)"
              >
                <title>
                  {r.day} · {money(r.totalAssets, r.baseCurrency)} ·{" "}
                  {r.carried ? "沿用值" : "有更新"}
                </title>
              </circle>
            )}
          </g>
        ))}
        <text x="36" y="164" fontSize="11" fill="currentColor">
          {points[0]?.day}
        </text>
        <text
          x="684"
          y="164"
          textAnchor="end"
          fontSize="11"
          fill="currentColor"
        >
          {points.at(-1)?.day}
        </text>
        <text x="36" y="19" fontSize="12" fill="currentColor">
          {money(max, points.at(-1)?.baseCurrency ?? "CNY")}
        </text>
      </svg>
      <small>虚线：沿用值</small>
    </div>
  );
}
export function AssetHistory({ snapshot }: { snapshot: Snapshot }) {
  const [history, setHistory] = useState<DailyHistory>();
  const [range, setRange] = useState(30);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const [refresh, setRefresh] = useState(0);
  const request = useRef(0);
  useEffect(() => {
    const wake = () => setRefresh((v) => v + 1);
    window.addEventListener(DATA_SAVED, wake);
    window.addEventListener(SERVER_DATA_UPDATED, wake);
    return () => {
      window.removeEventListener(DATA_SAVED, wake);
      window.removeEventListener(SERVER_DATA_UPDATED, wake);
    };
  }, []);
  useEffect(() => {
    const current = ++request.current;
    setLoading(true);
    setError("");
    getDailyAssets({ limit: range || 400 })
      .then((data) => {
        if (current !== request.current) return;
        setHistory(data);
      })
      .catch((e) => {
        if (current === request.current) setError(String(e));
      })
      .finally(() => {
        if (current === request.current) setLoading(false);
      });
    return () => {
      request.current++;
    };
  }, [range, snapshot.updatedAt, refresh]);
  const loadMore = async () => {
    if (!history?.nextBefore) return;
    const current = ++request.current;
    setLoading(true);
    try {
      const page = await getDailyAssets({
        limit: 400,
        before: history.nextBefore,
      });
      if (current === request.current)
        setHistory({ ...page, records: [...history.records, ...page.records] });
    } catch (e) {
      if (current === request.current) setError(String(e));
    } finally {
      if (current === request.current) setLoading(false);
    }
  };
  const latest = history?.records[0];
  return (
    <section className="panel daily-assets" aria-label="每日资产追踪">
      <div className="panel-title">
        <div>
          <h2>我的资产</h2>
        </div>
        <small title={`记账时区：${history?.timezone ?? ""}`}>
          {latest?.day}
          {loading ? " · 读取中…" : ""}
        </small>
      </div>
      {error && (
        <div role="alert" className="error-box">
          {error}{" "}
          <button
            className="text-button"
            onClick={() => setRefresh((v) => v + 1)}
          >
            重试读取
          </button>
        </div>
      )}
      {latest ? (
        <>
          <div className="daily-metrics">
            <div>
              <span>总资产</span>
              <strong>{money(latest.totalAssets, latest.baseCurrency)}</strong>
            </div>
            <div>
              <span>较昨日</span>
              <strong>
                {change(latest.previousDayChange, latest.baseCurrency)}
              </strong>
            </div>
            <div>
              <span>
                较上次实际更新
                {latest.lastUpdatedDay ? `（${latest.lastUpdatedDay}）` : ""}
              </span>
              <strong>
                {change(latest.sinceLastUpdateChange, latest.baseCurrency)}
              </strong>
            </div>
            {latest.liabilities > 0 && (
              <div>
                <span>
                  净资产 · 已扣负债{" "}
                  {money(latest.liabilities, latest.baseCurrency)}
                </span>
                <strong>{money(latest.netAssets, latest.baseCurrency)}</strong>
              </div>
            )}
          </div>
          <p className="daily-freshness">
            {latest.carried ? "今日沿用上次余额。" : "今日已更新。"}{" "}
            {latest.assets.filter((a) => a.carried && !a.removed).length}{" "}
            项资产沿用原值
          </p>
          {latest.missingFx.length > 0 && (
            <p className="daily-freshness">
              待补汇率：{latest.missingFx.join("、")}
            </p>
          )}
          <details className="daily-trend-details">
            <summary>资产趋势</summary>
            <div
              className="section-switcher"
              role="group"
              aria-label="资产趋势范围"
            >
              {[
                [30, "30 天"],
                [90, "90 天"],
                [365, "1 年"],
                [0, "全部历史"],
              ].map(([n, label]) => (
                <button
                  key={n}
                  aria-pressed={range === n}
                  onClick={() => setRange(Number(n))}
                >
                  {label}
                </button>
              ))}
            </div>
            <Trend records={history?.records ?? []} />
            {range === 0 && history?.nextBefore && (
              <button
                className="secondary"
                disabled={loading}
                onClick={() => void loadMore()}
              >
                加载更早记录
              </button>
            )}
          </details>
        </>
      ) : (
        <p className="empty">
          {loading
            ? "正在读取每日记录…"
            : "记录你的第一笔资产后，这里会自动追踪每天的变化。"}
        </p>
      )}
    </section>
  );
}
