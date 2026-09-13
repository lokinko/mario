import { useEffect, useRef, useState } from "react";
import { compareDailyAssets, getDailyAssets } from "../../api";
import type {
  DailyComparison,
  DailyHistory,
  DailyRecord,
  Snapshot,
} from "../../types";
import type { View } from "../../app/navigation";
import { formatMoney } from "../../lib/format";
import { CLOUD_DATA_UPDATED, DATA_SAVED } from "../../lib/syncEvents";

const money = (value: number | null, currency: string) =>
  value == null ? "总额待补全" : formatMoney(value, currency);
const change = (value: number | null, currency: string) =>
  value == null
    ? "暂不可比较"
    : `${value > 0 ? "+" : ""}${formatMoney(value, currency)}`;

function Trend({
  records,
  select,
}: {
  records: DailyRecord[];
  select: (day: string) => void;
}) {
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
            <Trend records={segment} select={select} />
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
        role="img"
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
                onClick={() => select(r.day)}
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
      <small>
        虚线 /
        空心点为沿用值；币种变化或总额不完整时断开。详细金额见下方日记录。
      </small>
    </div>
  );
}
export function AssetHistory({
  snapshot,
  navigate,
}: {
  snapshot: Snapshot;
  navigate: (view: View) => void;
}) {
  const [history, setHistory] = useState<DailyHistory>();
  const [range, setRange] = useState(30);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const [comparison, setComparison] = useState<DailyComparison>();
  const [compareError, setCompareError] = useState("");
  const [comparing, setComparing] = useState(false);
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [selected, setSelected] = useState("");
  const [refresh, setRefresh] = useState(0);
  const request = useRef(0),
    compareRequest = useRef(0);
  useEffect(() => {
    const wake = () => setRefresh((v) => v + 1);
    window.addEventListener(DATA_SAVED, wake);
    window.addEventListener(CLOUD_DATA_UPDATED, wake);
    return () => {
      window.removeEventListener(DATA_SAVED, wake);
      window.removeEventListener(CLOUD_DATA_UPDATED, wake);
    };
  }, []);
  useEffect(() => {
    const current = ++request.current;
    ++compareRequest.current;
    setComparison(undefined);
    setCompareError("");
    setComparing(false);
    setLoading(true);
    setError("");
    getDailyAssets({ limit: range || 400 })
      .then((data) => {
        if (current !== request.current) return;
        setHistory(data);
        setTo(data.records[0]?.day ?? "");
        setFrom(data.records.at(-1)?.day ?? "");
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
  const compare = async () => {
    const current = ++compareRequest.current;
    setComparing(true);
    setComparison(undefined);
    setCompareError("");
    try {
      const result = await compareDailyAssets(from, to);
      if (current === compareRequest.current) setComparison(result);
    } catch (e) {
      if (current === compareRequest.current) setCompareError(String(e));
    } finally {
      if (current === compareRequest.current) setComparing(false);
    }
  };
  const editRange = (value: string, side: "from" | "to") => {
    ++compareRequest.current;
    setComparing(false);
    setComparison(undefined);
    side === "from" ? setFrom(value) : setTo(value);
  };
  const analyze = () => {
    if (!comparison) return;
    window.sessionStorage.setItem(
      "mario.advisorQuestion",
      `请分析 ${comparison.from.day} 至 ${comparison.to.day} 的资产金额与配置变化，区分实际更新和沿用值，说明变化最大的资产。结合已有流水分析可能原因；缺少依据时不要推断收益或买卖行为。`,
    );
    window.sessionStorage.setItem(
      "mario.dailyAssetRange",
      JSON.stringify({ from: comparison.from.day, to: comparison.to.day }),
    );
    navigate("advisor");
  };
  const latest = history?.records[0];
  const detail = history?.records.find((r) => r.day === selected) ?? latest;
  return (
    <section className="panel daily-assets" aria-label="每日资产追踪">
      <div className="panel-title">
        <div>
          <h2>我的资产</h2>
          <p className="section-intro">更新金额即可，每天保留一份记录。</p>
        </div>
        <small>
          {latest?.day}
          {latest?.day === history?.today ? " · 更新中" : ""}
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
            {latest.carried
              ? "今日沿用上次余额，尚无新更新。"
              : "已合并本日更新。"}{" "}
            {latest.assets.filter((a) => a.carried && !a.removed).length}{" "}
            项资产沿用原值 · 记账时区 {history?.timezone}
          </p>
          {latest.missingFx.length > 0 && (
            <p className="daily-freshness">
              待补汇率：{latest.missingFx.join("、")}
            </p>
          )}
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
          <Trend records={history?.records ?? []} select={setSelected} />
          {range === 0 && history?.nextBefore && (
            <button
              className="secondary"
              disabled={loading}
              onClick={() => void loadMore()}
            >
              加载更早记录
            </button>
          )}
          <details
            className="daily-history-details"
            open={selected ? true : undefined}
          >
            <summary>资产历史与区间比较</summary>
            <div className="daily-compare-controls">
              <label>
                开始日期
                <input
                  type="date"
                  value={from}
                  max={to}
                  onChange={(e) => editRange(e.target.value, "from")}
                />
              </label>
              <label>
                结束日期
                <input
                  type="date"
                  value={to}
                  min={from}
                  max={history?.today}
                  onChange={(e) => editRange(e.target.value, "to")}
                />
              </label>
              <button
                className="secondary"
                disabled={!from || !to || from > to || comparing || loading}
                onClick={() => void compare()}
              >
                {comparing ? "比较中…" : "比较变化"}
              </button>
            </div>
            {compareError && <p role="alert">{compareError}</p>}
            {comparison && (
              <div className="daily-comparison">
                <div className="panel-title">
                  <strong>
                    区间资产金额变化{" "}
                    {change(
                      comparison.amountChange,
                      comparison.to.baseCurrency,
                    )}
                  </strong>
                  <button className="secondary" onClick={analyze}>
                    分析这段变化
                  </button>
                </div>
                <p>按金额变化绝对值排序；新增或移除记录不代表买卖。</p>
                {comparison.assets.map((a) => (
                  <div className="daily-change-row" key={a.id}>
                    <span>
                      {a.name}{" "}
                      {a.status === "added"
                        ? "· 新增记录"
                        : a.status === "removed"
                          ? "· 移除记录"
                          : ""}
                    </span>
                    <strong>
                      {change(a.amountChange, comparison.to.baseCurrency)}
                    </strong>
                    <small>
                      {a.pctPointChange == null
                        ? "占比暂不可比较"
                        : `占比 ${a.pctPointChange > 0 ? "+" : ""}${a.pctPointChange.toFixed(2)} 个百分点`}
                    </small>
                  </div>
                ))}
                {comparison.allocationChanges.length > 0 && (
                  <details>
                    <summary>资产类别占比变化</summary>
                    {comparison.allocationChanges.map((a) => (
                      <p key={a.assetClass}>
                        {a.assetClass} · {a.previousPct.toFixed(1)}% →{" "}
                        {a.currentPct.toFixed(1)}% ·{" "}
                        {change(a.valueChange, comparison.to.baseCurrency)}
                      </p>
                    ))}
                  </details>
                )}
              </div>
            )}
            <label className="daily-date-select">
              查看日记录
              <select
                value={detail?.day ?? ""}
                onChange={(e) => setSelected(e.target.value)}
              >
                {history?.records.map((r) => (
                  <option key={r.day} value={r.day}>
                    {r.day} · {r.carried ? "沿用值" : "有更新"} ·{" "}
                    {money(r.totalAssets, r.baseCurrency)}
                  </option>
                ))}
              </select>
            </label>
            {detail && (
              <div className="daily-detail">
                <p>
                  {detail.day} · 总资产{" "}
                  {money(detail.totalAssets, detail.baseCurrency)} · 负债{" "}
                  {money(detail.liabilities, detail.baseCurrency)} · 净资产{" "}
                  {money(detail.netAssets, detail.baseCurrency)}
                </p>
                {detail.assets.map((a) => (
                  <div className="daily-change-row" key={a.holding.id}>
                    <span>
                      {a.holding.name}
                      {a.removed ? " · 移除记录" : a.carried ? " · 沿用值" : ""}
                    </span>
                    <strong>
                      {money(a.holding.marketValue, a.holding.currency)}
                    </strong>
                    <small>
                      {a.changeKind === "added" ? "新增记录 · " : ""}
                      较昨日{" "}
                      {change(
                        a.previousDayChange ?? null,
                        detail.baseCurrency,
                      )}{" "}
                      ·
                      {a.pctPointChange != null
                        ? ` 占比 ${a.pctPointChange > 0 ? "+" : ""}${a.pctPointChange.toFixed(2)} 个百分点 · `
                        : " "}
                      确认于 {a.confirmedOn ?? "未知日期"}
                      {a.holding.currency !== detail.baseCurrency
                        ? ` · 汇率 ${a.holding.fxRateToBase ?? "待补"} · 观察于 ${a.holding.fxRateObservedOn || "未知日期"}`
                        : ""}
                    </small>
                  </div>
                ))}
              </div>
            )}
          </details>
          <p className="holding-hint">
            金额和占比变化不等于投资收益；沿用值不代表当日重新估值。
          </p>
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
