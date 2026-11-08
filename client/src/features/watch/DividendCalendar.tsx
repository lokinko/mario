import { useEffect, useState } from "react";
import { Button, Tabs } from "antd";
import { RefreshCw } from "lucide-react";
import {
  getDividendCalendar,
  refreshDividendCalendar,
  type DividendCalendarBoard,
  type DividendCalendarEntry,
  type DividendCalendarScope,
  type DividendPlanStage,
} from "../../api/watch";
import { PageHeader } from "../../components/PageHeader";
import { Callout } from "../../components/Callout";
import { useRequestGuard } from "../../lib/useRequestGuard";
import { formatNumber } from "../../lib/format";
import "./dividend-calendar.css";

const stages: Record<DividendPlanStage, string> = {
  preannounced: "预披露",
  proposed: "董事会预案",
  approved: "已获批准，日期待定",
  scheduled: "已公布实施安排",
  cancelled: "已取消",
  no_distribution: "不分配",
  unknown: "进度待核实",
};
const dates = (row: DividendCalendarEntry) =>
  [
    { date: row.plan.recordDate, label: "登记", kind: "record" },
    { date: row.plan.exDate, label: "除息", kind: "ex" },
    { date: row.plan.paymentDate, label: "派息", kind: "payment" },
  ].filter(
    (item): item is { date: string; label: string; kind: string } =>
      !!item.date,
  );
function addDays(day: string, amount: number) {
  const date = new Date(`${day}T00:00:00Z`);
  date.setUTCDate(date.getUTCDate() + amount);
  return date.toISOString().slice(0, 10);
}
const stamp = (time: string | null) =>
  time
    ? new Date(time).toLocaleString("zh-CN", { timeZone: "Asia/Shanghai" })
    : "尚未获取";
function PlanCard({
  row,
  busy,
  retry,
}: {
  row: DividendCalendarEntry;
  busy: boolean;
  retry: (symbol: string) => void;
}) {
  const plan = row.plan;
  return (
    <article className="dividend-plan-card">
      <div className="dividend-plan-heading">
        <h3>
          {row.name} <small>{row.symbol}</small>
        </h3>
        {row.personal && <span className="badge">自选</span>}
      </div>
      <p>
        {stages[plan.stage]} · 报告期 {plan.reportDate} · 公告日期{" "}
        {plan.announcedOn}
      </p>
      <div className="dividend-plan-amount">
        每 10 股税前现金：
        <strong>
          {plan.cashPerTen === null
            ? "未披露"
            : `${formatNumber(plan.cashPerTen, 4)} 元`}
        </strong>
        {plan.bonusPerTen && Number(plan.bonusPerTen) > 0 && (
          <span> · 送 {plan.bonusPerTen} 股</span>
        )}
        {plan.transferPerTen && Number(plan.transferPerTen) > 0 && (
          <span> · 转增 {plan.transferPerTen} 股</span>
        )}
      </div>
      {plan.cashPerTen === null && plan.profile && <p>{plan.profile}</p>}
      {plan.stage === "scheduled" && (
        <dl className="dividend-plan-dates">
          <div>
            <dt>股权登记日</dt>
            <dd>{plan.recordDate ?? "未提供"}</dd>
          </div>
          <div>
            <dt>除权除息日</dt>
            <dd>{plan.exDate ?? "未提供"}</dd>
          </div>
          <div>
            <dt>派息日</dt>
            <dd>{plan.paymentDate ?? "来源未提供"}</dd>
          </div>
        </dl>
      )}
      {(plan.issues.includes("invalid_dates") ||
        plan.issues.includes("invalid_amount")) && (
        <Callout tone="danger">
          此计划的{plan.issues.includes("invalid_dates") ? "日期" : "金额"}
          有待核对，未使用异常值。
          <Button
            size="small"
            disabled={busy}
            onClick={() => retry(row.symbol)}
          >
            重新抓取 {row.name}
          </Button>
        </Callout>
      )}
      <div className="dividend-plan-meta">
        <span>资料获取于 {stamp(row.fetchedAt)}</span>
        {plan.sources[0] && (
          <a href={plan.sources[0]} target="_blank" rel="noreferrer">
            分红计划来源 ↗
          </a>
        )}
      </div>
    </article>
  );
}
export function DividendCalendar() {
  const [board, setBoard] = useState<DividendCalendarBoard | null>(null);
  const [scope, setScope] = useState<DividendCalendarScope>("all");
  const [from, setFrom] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const [tab, setTab] = useState("scheduled");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const guard = useRequestGuard();
  const load = async () => {
    const current = guard.begin();
    try {
      const next = await getDividendCalendar(scope, from);
      if (current()) {
        setBoard(next);
        setError("");
      }
    } catch (error) {
      if (current()) setError(String(error));
    }
  };
  useEffect(() => {
    if (busy) return;
    void load();
    const interval = window.setInterval(() => {
      if (document.visibilityState === "visible") void load();
    }, 30000);
    return () => window.clearInterval(interval);
  }, [scope, from, busy]);
  const refresh = async (symbol?: string) => {
    if (busy) return;
    const current = guard.begin();
    setBusy(true);
    setError("");
    try {
      const next = await refreshDividendCalendar(
        scope,
        from,
        symbol ? [symbol] : undefined,
      );
      if (current()) setBoard(next);
    } catch (error) {
      if (current()) setError(String(error));
    } finally {
      if (current()) setBusy(false);
    }
  };
  const days = board
    ? Array.from({ length: 30 }, (_, i) => addDays(board.start, i))
    : [];
  const offset = board
    ? (new Date(`${board.start}T00:00:00Z`).getUTCDay() + 6) % 7
    : 0;
  const relevant =
    board?.entries.filter(
      (row) => !selected || dates(row).some((item) => item.date === selected),
    ) ?? [];
  const incomplete =
    board?.coverage.filter((row) => !row.fetchedAt || row.refreshFailed) ?? [];
  return (
    <div className="page dividend-calendar-page" data-presentation>
      <PageHeader
        title="分红日历"
        action={
          <Button
            disabled={busy}
            loading={busy}
            icon={<RefreshCw size={15} />}
            onClick={() => void refresh()}
          >
            更新分红计划
          </Button>
        }
      />
      <div className="dividend-calendar">
        <div className="dividend-calendar-toolbar">
          <label>
            查看范围{" "}
            <select
              aria-label="分红日历范围"
              value={scope}
              disabled={busy}
              onChange={(event) => {
                setScope(event.target.value as DividendCalendarScope);
                setSelected(null);
              }}
            >
              <option value="all">公共目录＋我的自选</option>
              <option value="favorites">仅我的自选</option>
              <option value="public">仅公共目录</option>
            </select>
          </label>
        </div>
        {error && (
          <Callout tone="danger">
            {error}
            <Button disabled={busy} onClick={() => void load()}>
              重新读取日历
            </Button>
          </Callout>
        )}
        {!board && !error && <p role="status">正在读取分红日历…</p>}
        {board && (
          <>
            <div className="dividend-calendar-navigation">
              <Button
                disabled={busy}
                onClick={() => {
                  setFrom(addDays(board.start, -30));
                  setSelected(null);
                }}
              >
                前30天
              </Button>
              <strong>
                {board.start} — {board.end}
              </strong>
              <Button
                disabled={busy}
                onClick={() => {
                  setFrom(addDays(board.start, 30));
                  setSelected(null);
                }}
              >
                后30天
              </Button>
            </div>
            <Tabs
              activeKey={tab}
              onChange={setTab}
              items={[
                { key: "scheduled", label: `已定日期 ${board.entries.length}` },
                { key: "pending", label: `待定计划 ${board.pending.length}` },
              ]}
            />
            {tab === "scheduled" ? (
              <>
                <div
                  className="dividend-calendar-grid"
                  aria-label="未来30天分红日期"
                >
                  {["一", "二", "三", "四", "五", "六", "日"].map((day) => (
                    <span key={day} className="dividend-calendar-weekday">
                      {day}
                    </span>
                  ))}
                  {Array.from({ length: offset }, (_, i) => (
                    <span key={`empty-${i}`} aria-hidden="true" />
                  ))}
                  {days.map((day) => {
                    const events = board.entries.flatMap((row) =>
                      dates(row)
                        .filter((item) => item.date === day)
                        .map((item) => ({ ...item, row })),
                    );
                    return (
                      <button
                        type="button"
                        key={day}
                        aria-label={`${day}，${events.length}项安排`}
                        aria-pressed={selected === day}
                        className={`dividend-calendar-day ${day === board.asOf ? "is-today" : ""}`}
                        onClick={() =>
                          setSelected(selected === day ? null : day)
                        }
                      >
                        <span>{day.slice(5).replace("-", "/")}</span>
                        {events.slice(0, 2).map((event) => (
                          <span
                            key={`${event.row.plan.id}:${event.kind}`}
                            className={`dividend-calendar-event event-${event.kind}`}
                          >
                            {event.label} · {event.row.name}
                          </span>
                        ))}
                        {events.length > 2 && (
                          <small>还有 {events.length - 2} 项</small>
                        )}
                      </button>
                    );
                  })}
                </div>
                <div className="dividend-calendar-section-heading">
                  <h3>
                    {selected ? `${selected} 的安排` : "未来30天已公布的安排"}
                  </h3>
                  {selected && (
                    <Button onClick={() => setSelected(null)}>
                      查看全部日期
                    </Button>
                  )}
                </div>
                {!relevant.length && (
                  <p>
                    {selected
                      ? "当天暂无已获取的确定安排。"
                      : "当前已获取资料中，未来30天暂无已公布日期的分红安排。"}
                  </p>
                )}
                {relevant.map((row) => (
                  <PlanCard
                    key={row.plan.id}
                    row={row}
                    busy={busy}
                    retry={(symbol) => void refresh(symbol)}
                  />
                ))}
              </>
            ) : (
              <>
                {!board.pending.length && <p>当前已获取资料中暂无待定计划。</p>}
                {board.pending.map((row) => (
                  <PlanCard
                    key={row.plan.id}
                    row={row}
                    busy={busy}
                    retry={(symbol) => void refresh(symbol)}
                  />
                ))}
              </>
            )}
            <details className="dividend-calendar-coverage">
              <summary>
                数据覆盖与更新记录
                {incomplete.length ? `（${incomplete.length} 家待处理）` : ""}
              </summary>
              {board.coverage.map((row) => (
                <div
                  key={row.symbol}
                  className="dividend-calendar-coverage-row"
                >
                  <span>
                    {row.name} · {row.symbol}
                    <small>
                      {row.refreshFailed
                        ? "本次来源读取失败，保留原资料和日期。"
                        : row.fetchedAt
                          ? "资料获取于"
                          : "尚未抓取"}{" "}
                      {row.fetchedAt ? stamp(row.fetchedAt) : ""}
                    </small>
                  </span>
                  <Button
                    size="small"
                    disabled={busy}
                    onClick={() => void refresh(row.symbol)}
                  >
                    重新抓取 {row.name}
                  </Button>
                </div>
              ))}
            </details>
          </>
        )}
      </div>
    </div>
  );
}
