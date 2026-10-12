import { useEffect, useState } from "react";
import { getPortfolioEvents } from "../../api";
import type { PortfolioEventRecord, Snapshot } from "../../types";
import type { View } from "../../app/navigation";
import { formatMoney } from "../../lib/format";
import { localDateValue } from "../../lib/dates";
import { DATA_SAVED, SERVER_DATA_UPDATED } from "../../lib/syncEvents";
import { rangeStart, summarizeEvents } from "./analytics";

interface Resource<T> {
  value: T | null;
  error: string;
}
const pending = { value: null, error: "" };

export function InvestmentActivity({
  snapshot,
  navigate,
}: {
  snapshot: Snapshot;
  navigate: (view: View) => void;
}) {
  const [events, setEvents] =
    useState<Resource<PortfolioEventRecord[]>>(pending);
  const [refresh, setRefresh] = useState(0);
  const [days, setDays] = useState(90);
  useEffect(() => {
    const wake = () => setRefresh((value) => value + 1);
    window.addEventListener(DATA_SAVED, wake);
    window.addEventListener(SERVER_DATA_UPDATED, wake);
    return () => {
      window.removeEventListener(DATA_SAVED, wake);
      window.removeEventListener(SERVER_DATA_UPDATED, wake);
    };
  }, []);
  useEffect(() => {
    let active = true;
    setEvents(pending);
    const read = async <T,>(
      fetch: () => Promise<T>,
      set: (value: Resource<T>) => void,
    ) => {
      try {
        const value = await fetch();
        if (active) set({ value, error: "" });
      } catch (error) {
        if (active) set({ value: null, error: String(error) });
      }
    };
    void read(getPortfolioEvents, setEvents);
    return () => {
      active = false;
    };
  }, [snapshot.updatedAt, refresh]);
  const currency = snapshot.profile.baseCurrency;
  const today = localDateValue(new Date());
  const from = rangeStart(today, days);
  const ledger = events.value
    ? summarizeEvents(events.value, currency, from, today)
    : null;
  const issue = (resource: Resource<unknown>, label: string) =>
    resource.error ? (
      <p role="alert" className="dashboard-note loss">
        {label}读取失败，指标暂不可用。
        <button
          className="text-button"
          onClick={() => setRefresh((value) => value + 1)}
        >
          重试读取
        </button>
      </p>
    ) : resource.value == null ? (
      <p role="status" className="dashboard-note">
        正在读取{label}…
      </p>
    ) : null;
  const money = (value: number | null | undefined) =>
    value == null ? "—" : formatMoney(value, currency);
  return (
    <section className="dashboard-section">
      <article className="panel" aria-label="资金流水分析">
        <div className="panel-title">
          <h2>资金流水分析</h2>
          <button className="text-button" onClick={() => navigate("ledger")}>
            记录流水 →
          </button>
        </div>
        <div
          className="section-switcher"
          role="group"
          aria-label="流水统计范围"
        >
          {([30, 90, 365, 0] as const).map((range) => (
            <button
              key={range}
              aria-pressed={days === range}
              onClick={() => setDays(range)}
            >
              {range ? `${range} 天` : "已载入"}
            </button>
          ))}
        </div>
        {issue(events, "流水")}
        {ledger && (
          <>
            <p className="dashboard-note">
              {from ?? "最早已载入记录"} 至 {today} · {ledger.count} 笔 ·{" "}
              {currency}
            </p>
            <dl className="dashboard-facts">
              <div>
                <dt>入金 / 出金</dt>
                <dd>
                  {money(ledger.deposits)} / {money(ledger.withdrawals)}
                </dd>
              </div>
              <div>
                <dt>净外部现金流</dt>
                <dd>{money(ledger.netFlow)}</dd>
              </div>
              <div>
                <dt>分红与利息</dt>
                <dd>{money(ledger.income)}</dd>
              </div>
              <div>
                <dt>费用与税费</dt>
                <dd>{money(ledger.costs)}</dd>
              </div>
              <div>
                <dt>收入扣除费用税费</dt>
                <dd>{money(ledger.income - ledger.costs)}</dd>
              </div>
              <div>
                <dt>买卖总额</dt>
                <dd>{money(ledger.trading)}</dd>
              </div>
            </dl>
            {!ledger.count && (
              <p className="dashboard-note">
                所选区间没有当前币种流水，零值只表示未记录。
              </p>
            )}
            {ledger.excluded > 0 && (
              <p className="dashboard-note">
                另有 {ledger.excluded} 笔按其他基准币种冻结的流水未合并。
              </p>
            )}
            {events.value!.length >= 500 && (
              <p className="dashboard-note warning">
                已达到接口最近 500
                笔上限，所选区间可能包含未载入的更早流水，以下统计可能不完整。
              </p>
            )}
          </>
        )}
        <p className="dashboard-note">
          基于接口返回的最近最多 500
          笔流水，按发生日期及冻结汇率汇总，冲正保留在发生区间内；买卖是内部交易，不计作入出金。
        </p>
      </article>
    </section>
  );
}
