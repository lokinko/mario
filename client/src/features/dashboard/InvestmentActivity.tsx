import { useEffect, useState } from "react";
import {
  getDecisions,
  getPortfolioEvents,
  getRuleEffectiveness,
} from "../../api";
import type {
  DecisionRecord,
  PortfolioCheckInRecord,
  PortfolioEventRecord,
  RuleEffectivenessSummary,
  Snapshot,
} from "../../types";
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
  checkins,
  checkinsLoading,
  checkinError,
  navigate,
}: {
  snapshot: Snapshot;
  checkins: PortfolioCheckInRecord[];
  checkinsLoading: boolean;
  checkinError: string;
  navigate: (view: View) => void;
}) {
  const [events, setEvents] =
    useState<Resource<PortfolioEventRecord[]>>(pending);
  const [decisions, setDecisions] =
    useState<Resource<DecisionRecord[]>>(pending);
  const [rules, setRules] =
    useState<Resource<RuleEffectivenessSummary>>(pending);
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
    setDecisions(pending);
    setRules(pending);
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
    void Promise.allSettled([
      read(getPortfolioEvents, setEvents),
      read(getDecisions, setDecisions),
      read(getRuleEffectiveness, setRules),
    ]);
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
  const latest = checkins[0];
  const compatible = latest?.baseCurrency === currency;
  const reviewed = decisions.value?.filter((decision) => decision.review);
  const due = decisions.value?.filter(
    (decision) => !decision.review && decision.reviewDate <= today,
  );
  const scored = reviewed?.filter(
    (decision) =>
      decision.review!.processRating != null &&
      Number.isFinite(decision.review!.processRating),
  );
  const average = scored?.length
    ? scored.reduce(
        (sum, decision) => sum + (decision.review!.processRating ?? 0),
        0,
      ) / scored.length
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
    <>
      <section className="panel dashboard-section" aria-label="投资变化归因">
        <div className="panel-title">
          <h2>最近一期投资变化</h2>
          <span>以冻结快照为准</span>
        </div>
        {checkinsLoading ? (
          <p className="empty">正在读取组合快照…</p>
        ) : checkinError ? (
          <p className="dashboard-note loss">
            组合快照读取失败，请在下方“组合变化与快照”重试。
          </p>
        ) : !latest ? (
          <p className="empty">
            在下方建立组合基线，再冻结下一期持仓后，查看现金流调整后的变化。
          </p>
        ) : !compatible ? (
          <p className="dashboard-note">
            最近快照采用 {latest.baseCurrency}，当前基准币种为 {currency}
            ，请重新建立基线后比较。
          </p>
        ) : (
          <>
            <p className="dashboard-note">
              {latest.periodLabel} · 估值日 {latest.valuationDate ?? "未记录"} ·{" "}
              {latest.totalChange == null
                ? "本期为基线，无可比较回报"
                : "冻结期间数据，不随当前余额更新"}
            </p>
            <div className="dashboard-stat-grid">
              <div>
                <span>组合总变化</span>
                <strong>{money(latest.totalChange)}</strong>
              </div>
              <div>
                <span>净外部现金流</span>
                <strong>
                  {latest.totalChange == null
                    ? "—"
                    : money(latest.externalCashFlow)}
                </strong>
              </div>
              <div>
                <span>估值与数据残差</span>
                <strong>{money(latest.valuationResidual)}</strong>
              </div>
              <div>
                <span>现金流调整后期间近似回报</span>
                <strong>
                  {latest.modifiedDietzReturnPct == null
                    ? "—"
                    : `${latest.modifiedDietzReturnPct.toFixed(2)}%`}
                </strong>
              </div>
            </div>
            <p className="dashboard-note">
              近似回报使用 Modified
              Dietz，不是时间加权收益率；残差可能混入价格、汇率及录入变化。
            </p>
          </>
        )}
      </section>
      <section className="two-columns dashboard-section">
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
        <article className="panel" aria-label="投资决策与纪律">
          <div className="panel-title">
            <h2>投资决策与纪律</h2>
            <button className="text-button" onClick={() => navigate("review")}>
              查看复盘 →
            </button>
          </div>
          {issue(decisions, "决策")}
          {decisions.value && (
            <dl className="dashboard-facts">
              <div>
                <dt>已记录决策</dt>
                <dd>{decisions.value.length} 条</dd>
              </div>
              <div>
                <dt>已复盘 / 未复盘</dt>
                <dd>
                  {reviewed!.length} /{" "}
                  {decisions.value.length - reviewed!.length} 条
                </dd>
              </div>
              <div>
                <dt>到期未复盘</dt>
                <dd className={due!.length ? "warning" : ""}>
                  {due!.length} 条
                </dd>
              </div>
              <div>
                <dt>已复盘过程评分均值</dt>
                <dd>{average == null ? "—" : `${average.toFixed(1)} / 5`}</dd>
              </div>
            </dl>
          )}
          {issue(rules, "规则执行统计")}
          {rules.value && (
            <dl className="dashboard-facts">
              <div>
                <dt>适用规则检查</dt>
                <dd>{rules.value.applicableChecks} 次</dd>
              </div>
              <div>
                <dt>规则遵守率</dt>
                <dd>
                  {rules.value.adherencePct == null
                    ? "—"
                    : `${rules.value.adherencePct.toFixed(1)}%`}
                </dd>
              </div>
            </dl>
          )}
          <p className="dashboard-note">
            过程评分和规则遵守情况用于复盘纪律，不据此推断投资能力或未来收益。
          </p>
        </article>
      </section>
    </>
  );
}
