import { useEffect, useState } from "react";
import { Button, Segmented } from "antd";
import { ChevronLeft, ChevronRight } from "lucide-react";
import {
  getDividendCalendar,
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
const scopes: { value: DividendCalendarScope; label: string }[] = [
  { value: "all", label: "公共目录＋我的自选" },
  { value: "favorites", label: "仅我的自选" },
  { value: "public", label: "仅公共目录" },
];
function rangeTitle(start: string, end: string) {
  const [y1, m1, d1] = start.split("-").map(Number);
  const [y2, m2, d2] = end.split("-").map(Number);
  return `${y1}年${m1}月${d1}日 – ${y2 === y1 ? "" : `${y2}年`}${m2}月${d2}日`;
}
const stamp = (time: string | null) =>
  time
    ? new Date(time).toLocaleString("zh-CN", { timeZone: "Asia/Shanghai" })
    : "尚未获取";
function PlanCard({
  row,
  onAsk,
  onInspectWatch,
}: {
  row: DividendCalendarEntry;
  onAsk?: (question: string) => void;
  onInspectWatch?: (symbol: string, personal: boolean) => void;
}) {
  const plan = row.plan;
  return (
    <article className="dividend-plan-card">
      <div className="dividend-plan-heading">
        <h3>
          {row.name} <small>{row.symbol}</small>
        </h3>
        <span className="dividend-plan-badges">
          {plan.stage !== "scheduled" && (
            <span className="badge">{stages[plan.stage]}</span>
          )}
          {plan.issues.includes("invalid_dates") && (
            <span className="badge">日期待核对</span>
          )}
          {plan.issues.includes("invalid_amount") && (
            <span className="badge">金额待核对</span>
          )}
          {row.personal && <span className="badge">自选</span>}
        </span>
      </div>
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
      <div className="dividend-plan-meta">
        <span>资料获取于 {stamp(row.fetchedAt)}</span>
        <div className="dividend-plan-links">
          {onInspectWatch && (
            <button
              type="button"
              className="text-button"
              onClick={() => onInspectWatch(row.symbol, row.personal)}
            >
              查看观察
            </button>
          )}
          {onAsk && (
            <button
              type="button"
              className="text-button"
              onClick={() =>
                onAsk(
                  `请研究${row.name}（${row.symbol}）本次分红计划（${plan.profile || `每 10 股税前 ${plan.cashPerTen ?? "待核实"} 元`}，股权登记日 ${plan.recordDate ?? "待定"}，除权除息日 ${plan.exDate ?? "待定"}）。请核对最新公告、分红可持续性以及对我当前组合与观察计划的影响。`,
                )
              }
            >
              问 AI 分红
            </button>
          )}
          {plan.sources[0] && (
            <a href={plan.sources[0]} target="_blank" rel="noreferrer">
              分红计划来源 ↗
            </a>
          )}
        </div>
      </div>
    </article>
  );
}
export function DividendCalendar({
  onAsk,
  onInspectWatch,
  embedded = false,
}: {
  onAsk?: (question: string) => void;
  onInspectWatch?: (symbol: string, personal: boolean) => void;
  embedded?: boolean;
} = {}) {
  const [loaded, setLoaded] = useState<{
    scope: DividendCalendarScope;
    from: string;
    board: DividendCalendarBoard;
  }>();
  const [scope, setScope] = useState<DividendCalendarScope>("all");
  const [from, setFrom] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const [failure, setFailure] = useState<{
    scope: DividendCalendarScope;
    from: string;
    message: string;
  }>();
  const board =
    loaded?.scope === scope && loaded.from === from ? loaded.board : null;
  const error =
    failure?.scope === scope && failure.from === from ? failure.message : "";
  const navigationStart = from || loaded?.board.start;
  const guard = useRequestGuard();
  const load = async () => {
    const current = guard.begin();
    setFailure(undefined);
    try {
      const next = await getDividendCalendar(scope, from);
      if (current()) setLoaded({ scope, from, board: next });
    } catch (error) {
      if (current()) setFailure({ scope, from, message: String(error) });
    }
  };
  useEffect(() => {
    void load();
    const interval = window.setInterval(() => {
      if (document.visibilityState === "visible") void load();
    }, 30000);
    return () => {
      window.clearInterval(interval);
      guard.invalidate();
    };
  }, [scope, from]);
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
  return (
    <div
      className={
        embedded ? "dividend-calendar-embedded" : "page dividend-calendar-page"
      }
      data-presentation
    >
      {!embedded && <PageHeader title="分红日历" />}
      <div className="dividend-calendar">
        <div className="dividend-calendar-toolbar">
          {navigationStart && (
            <div className="dividend-calendar-navigation">
              <Button
                autoInsertSpace={false}
                disabled={!from}
                onClick={() => {
                  setFrom("");
                  setSelected(null);
                }}
              >
                今天
              </Button>
              <Button
                type="text"
                aria-label="前30天"
                title="前30天"
                icon={<ChevronLeft size={18} />}
                onClick={() => {
                  setFrom((date) => addDays(date || navigationStart, -30));
                  setSelected(null);
                }}
              />
              <Button
                type="text"
                aria-label="后30天"
                title="后30天"
                icon={<ChevronRight size={18} />}
                onClick={() => {
                  setFrom((date) => addDays(date || navigationStart, 30));
                  setSelected(null);
                }}
              />
              <h2>
                {rangeTitle(
                  navigationStart,
                  board?.end ?? addDays(navigationStart, 29),
                )}
              </h2>
            </div>
          )}
          <Segmented
            aria-label="分红日历范围"
            value={scope}
            options={scopes}
            onChange={(value) => {
              setScope(value);
              setSelected(null);
            }}
          />
        </div>
        {error && (
          <Callout tone="danger">
            {error}
            <Button onClick={() => void load()}>重新读取日历</Button>
          </Callout>
        )}
        {!board && !error && <p role="status">正在读取分红日历…</p>}
        {board && (
          <>
            <section className="panel dividend-calendar-board">
              <div
                className="dividend-calendar-grid"
                aria-label={from ? "当前30天分红日期" : "未来30天分红日期"}
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
                      onClick={() => setSelected(selected === day ? null : day)}
                    >
                      <span className="dividend-calendar-date">
                        {day.slice(5).replace("-", "/")}
                      </span>
                      {events.slice(0, 2).map((event) => (
                        <span
                          key={`${event.row.plan.id}:${event.kind}`}
                          className={`dividend-calendar-event event-${event.kind}`}
                        >
                          {event.label} · {event.row.name}
                        </span>
                      ))}
                      {events.length > 2 && (
                        <small className="dividend-calendar-more">
                          还有 {events.length - 2} 项
                        </small>
                      )}
                      {events.length > 0 && (
                        <span
                          className="dividend-calendar-dots"
                          aria-hidden="true"
                        >
                          {events.slice(0, 3).map((event) => (
                            <i
                              key={`${event.row.plan.id}:${event.kind}:dot`}
                              className={`dot-${event.kind}`}
                            />
                          ))}
                          {events.length > 3 && <b>+{events.length - 3}</b>}
                        </span>
                      )}
                    </button>
                  );
                })}
              </div>
            </section>
            <div className="dividend-calendar-section-heading">
              <h3>
                {selected
                  ? `${selected} 的安排`
                  : from
                    ? `${rangeTitle(board.start, board.end)} 已公布的安排`
                    : "未来30天已公布的安排"}
              </h3>
              {selected && (
                <Button onClick={() => setSelected(null)}>查看全部日期</Button>
              )}
            </div>
            {!relevant.length ? (
              <p className="panel dividend-calendar-empty">
                {selected
                  ? "当天暂无已获取的确定安排。"
                  : from
                    ? "当前已获取资料中，该30天内暂无已公布日期的分红安排。"
                    : "当前已获取资料中，未来30天暂无已公布日期的分红安排。"}
              </p>
            ) : (
              <div className="dividend-plan-grid">
                {relevant.map((row) => (
                  <PlanCard
                    key={row.plan.id}
                    row={row}
                    onAsk={onAsk}
                    onInspectWatch={onInspectWatch}
                  />
                ))}
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}
