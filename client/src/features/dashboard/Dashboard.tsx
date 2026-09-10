import { useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  ArrowRight,
  ChevronRight,
  CircleDollarSign,
  History,
  ShieldCheck,
  Target,
} from "lucide-react";
import {
  getPortfolioCheckins,
  getReviewReminders,
  savePortfolioCheckin,
} from "../../api";
import type {
  PortfolioCheckInInput,
  PortfolioCheckInRecord,
  ReviewReminderSummary,
  Snapshot,
} from "../../types";
import { nextAction } from "./nextAction";
import { PageHeader } from "../../components/PageHeader";
import { View, FoundationSection } from "../../app/navigation";
import { holdingValueInBase } from "../portfolio/valuation";
import { formatMoney } from "../../lib/format";
import {
  allocationGradient,
  goalStatus,
  riskStatus,
} from "../portfolio/presentation";

export function Dashboard({
  snapshot,
  navigate,
  flash,
}: {
  snapshot: Snapshot;
  navigate: (view: View, section?: FoundationSection) => void;
  flash: (message: string) => void;
}) {
  const [allFindings, setAllFindings] = useState(false);
  const [checkins, setCheckins] = useState<PortfolioCheckInRecord[]>([]);
  const [reminders, setReminders] = useState<ReviewReminderSummary | null>(
    null,
  );
  const action = nextAction(snapshot, reminders);
  useEffect(() => {
    let active = true;
    getReviewReminders()
      .then((value) => {
        if (active) setReminders(value);
      })
      .catch(() => {
        if (active) setReminders(null);
      });
    return () => {
      active = false;
    };
  }, []);
  const [checkin, setCheckin] = useState<PortfolioCheckInInput>({
    periodLabel: new Intl.DateTimeFormat("zh-CN", {
      year: "numeric",
      month: "long",
    }).format(new Date()),
    externalCashFlow: 0,
    note: "",
    resetBaseline: false,
    useLedgerCashFlows: true,
  });
  const [checkinSaving, setCheckinSaving] = useState(false);
  const [checkinError, setCheckinError] = useState("");
  const allocation = useMemo(() => {
    if (!snapshot.valuationStatus.comparable) return [];
    const totals = new Map<string, number>();
    snapshot.holdings.forEach((h) =>
      totals.set(
        h.assetClass,
        (totals.get(h.assetClass) ?? 0) +
          holdingValueInBase(h, snapshot.profile.baseCurrency),
      ),
    );
    return [...totals.entries()].map(([name, value]) => ({
      name,
      value,
      pct: snapshot.totalValue ? (value / snapshot.totalValue) * 100 : 0,
    }));
  }, [snapshot]);

  const latestCheckin = checkins[0];
  const baselineMode =
    !latestCheckin ||
    !latestCheckin.valuationDate ||
    latestCheckin.baseCurrency !== snapshot.profile.baseCurrency ||
    checkin.resetBaseline;

  useEffect(() => {
    getPortfolioCheckins()
      .then((records) => {
        setCheckins(records);
        setCheckinError("");
      })
      .catch((error) => setCheckinError(String(error)));
  }, []);

  const persistCheckin = async () => {
    if (!checkin.periodLabel) return;
    setCheckinSaving(true);
    setCheckinError("");
    try {
      await savePortfolioCheckin(
        baselineMode
          ? {
              ...checkin,
              externalCashFlow: 0,
              resetBaseline: Boolean(latestCheckin),
            }
          : checkin,
      );
      setCheckins(await getPortfolioCheckins());
      setCheckin({
        ...checkin,
        externalCashFlow: 0,
        note: "",
        resetBaseline: false,
      });
      flash(
        latestCheckin && !checkin.resetBaseline
          ? "组合变化已归因并冻结"
          : "组合变化基线已建立",
      );
    } catch (error) {
      setCheckinError(String(error));
    } finally {
      setCheckinSaving(false);
    }
  };

  return (
    <div className="page">
      <PageHeader title="总览" />

      <section className="panel next-action" aria-label="下一步">
        <div>
          <h2>{action.title}</h2>
          <p>{action.detail}</p>
        </div>
        {action.destination && (
          <button
            className="primary"
            onClick={() => navigate(action.destination!, action.section)}
          >
            <ArrowRight size={17} />
            {action.label}
          </button>
        )}
      </section>

      <section className="metric-grid">
        <article className="metric hero-metric">
          <span>持仓总值 · {snapshot.profile.baseCurrency}</span>
          <strong>
            {snapshot.valuationStatus.comparable
              ? formatMoney(snapshot.totalValue, snapshot.profile.baseCurrency)
              : "等待汇率"}
          </strong>
          <small>
            {snapshot.valuationStatus.comparable
              ? `最近更新 · ${new Date(snapshot.updatedAt).toLocaleDateString("zh-CN")}`
              : `${snapshot.valuationStatus.missingFxHoldings.length} 项外币持仓未折算`}
          </small>
        </article>
        <article className="metric">
          <span>应急覆盖</span>
          <strong>
            {snapshot.emergencyMonths.toFixed(1)} <em>个月</em>
          </strong>
          <meter
            className="coverage-meter"
            aria-label="应急资金覆盖月数"
            min={0}
            max={12}
            low={6}
            optimum={12}
            value={Math.max(0, snapshot.emergencyMonths)}
          />
          <small
            className={snapshot.emergencyMonths >= 6 ? "positive" : "warning"}
          >
            {snapshot.emergencyMonths >= 6 ? "处于建议区间" : "建议优先补足"}
          </small>
        </article>
        <article className="metric">
          <span>最大资产占比</span>
          <strong>
            {snapshot.valuationStatus.comparable
              ? `${snapshot.concentrationPct.toFixed(1)}%`
              : "—"}
          </strong>
          <small>
            {snapshot.valuationStatus.comparable
              ? "单项持仓占总值"
              : "补齐汇率后再计算"}
          </small>
        </article>
      </section>

      {snapshot.valuationStatus.warnings.length > 0 && (
        <section className="valuation-warning">
          <AlertTriangle size={17} />
          <div>
            <strong>估值待完善</strong>
            <p>
              {snapshot.valuationStatus.warnings.join("；")}。基准币种为{" "}
              {snapshot.profile.baseCurrency}
              {snapshot.valuationStatus.alignedValuationDate
                ? `，统一估值日 ${snapshot.valuationStatus.alignedValuationDate}`
                : ""}
              。
            </p>
          </div>
          <button
            className="text-button"
            onClick={() => navigate("foundation")}
          >
            完善持仓
          </button>
        </section>
      )}

      <section className="two-columns">
        <article className="panel">
          <div className="panel-title">
            <div>
              <h2>资产分布</h2>
            </div>
            <button
              className="text-button"
              onClick={() => navigate("foundation")}
            >
              管理资产 <ChevronRight size={15} />
            </button>
          </div>
          {allocation.length === 0 ? (
            <div className="empty-state">
              <CircleDollarSign size={32} />
              <p>
                {snapshot.holdings.length ? "补齐汇率后查看分布" : "还没有持仓"}
              </p>
              <button
                className="secondary"
                onClick={() => navigate("foundation")}
              >
                {snapshot.holdings.length ? "完善持仓" : "添加持仓"}
              </button>
            </div>
          ) : (
            <div className="allocation">
              <div
                className="donut"
                style={{
                  background: allocation.length
                    ? allocationGradient(allocation)
                    : "#dedfd9",
                }}
              >
                <div>
                  <strong>
                    {snapshot.valuationStatus.comparable
                      ? allocation.length
                      : "—"}
                  </strong>
                  <span>
                    {snapshot.valuationStatus.comparable
                      ? "类资产"
                      : "等待汇率"}
                  </span>
                </div>
              </div>
              <div className="legend">
                {allocation.map((item, index) => (
                  <div key={item.name}>
                    <i className={`color-${index % 5}`} />
                    <span>{item.name}</span>
                    <strong>
                      {item.pct.toFixed(1)}%
                      <small>
                        {formatMoney(item.value, snapshot.profile.baseCurrency)}
                      </small>
                    </strong>
                  </div>
                ))}
              </div>
            </div>
          )}
        </article>

        <article className="panel">
          <div className="panel-title">
            <div>
              <h2>风险提醒</h2>
            </div>
          </div>
          <div className="finding-list">
            {snapshot.findings.length === 0 && (
              <div className="empty">暂无风险提醒</div>
            )}
            {[...snapshot.findings]
              .sort(
                (a, b) =>
                  ({ high: 0, medium: 1, low: 2 })[a.level] -
                  { high: 0, medium: 1, low: 2 }[b.level],
              )
              .slice(0, allFindings ? undefined : 3)
              .map((finding, index) => (
                <div
                  className={`finding ${finding.level}`}
                  key={`${finding.title}-${index}`}
                >
                  <AlertTriangle size={17} />
                  <div>
                    <strong>{finding.title}</strong>
                    <details className="inline-help">
                      <summary>查看依据</summary>
                      <p>{finding.detail}</p>
                    </details>
                    <small>{finding.action}</small>
                  </div>
                </div>
              ))}
            {snapshot.findings.length > 3 && (
              <button
                className="text-button"
                onClick={() => setAllFindings(!allFindings)}
              >
                {allFindings
                  ? "收起"
                  : "查看全部 " + snapshot.findings.length + " 项"}
              </button>
            )}
          </div>
        </article>
      </section>

      <section className="planning-grid">
        <article className="panel">
          <div className="panel-title">
            <div>
              <h2>目标进度</h2>
            </div>
          </div>
          {snapshot.plan.goalProjections.length === 0 ? (
            <div className="empty">
              {snapshot.valuationStatus.comparable
                ? "添加目标后查看模拟进度"
                : "补齐汇率后查看目标进度"}
            </div>
          ) : (
            <div className="projection-list">
              {snapshot.plan.goalProjections.map((goal) => (
                <div key={goal.goalId}>
                  <div className="projection-head">
                    <strong>{goal.name}</strong>
                    <span className={goal.status}>
                      {goalStatus(goal.status)}
                    </span>
                  </div>
                  <div className="projection-bar">
                    <i
                      style={{
                        width: `${Math.min(100, goal.estimatedSuccessPct)}%`,
                      }}
                    />
                  </div>
                  <div className="projection-stats">
                    <span>
                      模拟达成率 <b>{goal.estimatedSuccessPct.toFixed(0)}%</b>
                    </span>
                    <span>
                      月度缺口{" "}
                      <b>
                        {formatMoney(
                          goal.monthlyGap,
                          snapshot.profile.baseCurrency,
                        )}
                      </b>
                    </span>
                    <span>
                      剩余 <b>{goal.monthsRemaining} 个月</b>
                    </span>
                  </div>
                </div>
              ))}
            </div>
          )}
        </article>
        <article className="panel">
          <div className="panel-title">
            <div>
              <h2>风险预算</h2>
            </div>
          </div>
          <div className={"risk-comparison " + snapshot.plan.riskStatus}>
            {[
              ["压力损失估计", snapshot.plan.stressLossPct],
              ["风险容量", snapshot.plan.riskCapacityPct],
            ].map(([label, value], index) => (
              <div className="risk-comparison-row" key={label}>
                <span>{label}</span>
                <strong>
                  {snapshot.valuationStatus.comparable &&
                  snapshot.holdings.length
                    ? Number(value).toFixed(1) + "%"
                    : "—"}
                </strong>
                <div className="risk-track" aria-hidden="true">
                  <i
                    className={index ? "capacity" : "stress"}
                    style={{
                      width:
                        snapshot.valuationStatus.comparable &&
                        snapshot.holdings.length
                          ? (Math.max(0, Number(value)) /
                              Math.max(
                                30,
                                snapshot.plan.stressLossPct,
                                snapshot.plan.riskCapacityPct,
                              )) *
                              100 +
                            "%"
                          : "0%",
                    }}
                  />
                </div>
              </div>
            ))}
            <span className="risk-status">
              {snapshot.valuationStatus.comparable
                ? riskStatus(snapshot.plan.riskStatus)
                : "等待汇率"}
            </span>
          </div>
          {snapshot.plan.rebalancing.length > 0 ? (
            <div className="rebalance-list">
              {snapshot.plan.rebalancing.slice(0, 4).map((item) => (
                <div key={item.holdingId}>
                  <span>{item.name}</span>
                  <small>
                    {item.currentPct.toFixed(1)}% → {item.targetPct.toFixed(1)}%
                  </small>
                  <strong>
                    {item.direction}{" "}
                    {formatMoney(item.amount, snapshot.profile.baseCurrency)}
                  </strong>
                </div>
              ))}
            </div>
          ) : (
            <p className="planning-empty">
              {snapshot.valuationStatus.comparable && snapshot.holdings.length
                ? "暂无再平衡提示"
                : "完善持仓后检查再平衡"}
            </p>
          )}
          <details className="inline-help">
            <summary>计算假设</summary>
            <p>{snapshot.plan.assumptions}</p>
            <p>目标权重合计 100% 后，显示偏差超过 3 个百分点的再平衡项。</p>
          </details>
        </article>
      </section>

      <details className="panel disclosure-panel portfolio-attribution">
        <summary>
          组合变化与快照
          <span>
            {checkins.length ? `${checkins.length} 次记录` : "尚未建立基线"}
          </span>
        </summary>
        <div className="disclosure-content">
          <p className="holding-hint">
            区分资金投入和估值变化；残差不代表收益率。
          </p>
          {!latestCheckin && (
            <div className="attribution-baseline">
              <History size={18} />
              <div>
                <strong>先建立一条组合基线</strong>
                <p>保存当前持仓，作为下次比较的起点。</p>
              </div>
            </div>
          )}
          {latestCheckin && baselineMode && (
            <div className="attribution-baseline">
              <History size={18} />
              <div>
                <strong>本次将重新建立比较基线</strong>
                <p>
                  {latestCheckin.baseCurrency !== snapshot.profile.baseCurrency
                    ? `基准币种已从 ${latestCheckin.baseCurrency} 改为 ${snapshot.profile.baseCurrency}，两个口径不能直接比较。`
                    : "适用于估值口径发生实质变化的情况；新记录不会计算与上一条的差额。"}
                </p>
              </div>
            </div>
          )}
          {latestCheckin?.totalChange != null && (
            <div className="attribution-metrics">
              <article>
                <span>组合总值变化</span>
                <strong
                  className={latestCheckin.totalChange >= 0 ? "gain" : "loss"}
                >
                  {latestCheckin.totalChange >= 0 ? "+" : ""}
                  {formatMoney(
                    latestCheckin.totalChange,
                    latestCheckin.baseCurrency,
                  )}
                </strong>
                <small>
                  {formatMoney(
                    latestCheckin.previousTotalValue ?? 0,
                    latestCheckin.baseCurrency,
                  )}{" "}
                  →{" "}
                  {formatMoney(
                    latestCheckin.totalValue,
                    latestCheckin.baseCurrency,
                  )}
                </small>
              </article>
              <article>
                <span>期间净外部现金流</span>
                <strong>
                  {latestCheckin.externalCashFlow >= 0 ? "+" : ""}
                  {formatMoney(
                    latestCheckin.externalCashFlow,
                    latestCheckin.baseCurrency,
                  )}
                </strong>
                <small>
                  {latestCheckin.cashFlowSource === "ledger"
                    ? `${latestCheckin.eventIds.length} 笔流水自动汇总`
                    : "旧记录为手工净额"}
                </small>
              </article>
              <article>
                <span>估值与数据变动残差</span>
                <strong
                  className={
                    (latestCheckin.valuationResidual ?? 0) >= 0
                      ? "gain"
                      : "loss"
                  }
                >
                  {(latestCheckin.valuationResidual ?? 0) >= 0 ? "+" : ""}
                  {formatMoney(
                    latestCheckin.valuationResidual ?? 0,
                    latestCheckin.baseCurrency,
                  )}
                </strong>
                <small>总值变化减净现金流</small>
              </article>
              <article>
                <span>现金流调整后期间回报</span>
                <strong>
                  {latestCheckin.modifiedDietzReturnPct == null
                    ? "—"
                    : `${latestCheckin.modifiedDietzReturnPct >= 0 ? "+" : ""}${latestCheckin.modifiedDietzReturnPct.toFixed(2)}%`}
                </strong>
                <small>Modified Dietz 近似，不是 TWR</small>
              </article>
            </div>
          )}
          {latestCheckin?.allocationChanges.length > 0 && (
            <div className="allocation-change-list">
              {latestCheckin.allocationChanges.slice(0, 5).map((item) => (
                <div key={item.assetClass}>
                  <strong>{item.assetClass}</strong>
                  <span>
                    {formatMoney(
                      item.previousValue,
                      latestCheckin.baseCurrency,
                    )}{" "}
                    →{" "}
                    {formatMoney(item.currentValue, latestCheckin.baseCurrency)}
                  </span>
                  <em className={item.pctPointChange >= 0 ? "gain" : "loss"}>
                    {item.pctPointChange >= 0 ? "+" : ""}
                    {item.pctPointChange.toFixed(1)} pct
                  </em>
                </div>
              ))}
            </div>
          )}
          <div className="attribution-entry">
            <label>
              <span>记录周期</span>
              <input
                maxLength={100}
                value={checkin.periodLabel}
                onChange={(event) =>
                  setCheckin({ ...checkin, periodLabel: event.target.value })
                }
                placeholder="例如 2026 年 9 月"
              />
            </label>
            <label>
              <span>期间现金流</span>
              <input
                disabled
                value={
                  baselineMode
                    ? "本次仅建立比较基线"
                    : "按两个估值日之间的流水自动汇总"
                }
              />
              <small>
                <button
                  className="inline-link"
                  onClick={() => navigate("ledger")}
                >
                  前往组合流水
                </button>
              </small>
            </label>
            <label className="attribution-note">
              <span>估值口径说明（可选）</span>
              <input
                maxLength={2000}
                value={checkin.note}
                onChange={(event) =>
                  setCheckin({ ...checkin, note: event.target.value })
                }
                placeholder={
                  latestCheckin
                    ? "例如：所有持仓均按同一日收盘价更新"
                    : "例如：首次冻结，持仓按同一日口径录入"
                }
              />
            </label>
            <button
              className="secondary"
              disabled={
                checkinSaving ||
                !snapshot.holdings.length ||
                !snapshot.valuationStatus.comparable ||
                !snapshot.valuationStatus.alignedValuationDate ||
                !checkin.periodLabel ||
                Boolean(
                  !baselineMode &&
                  latestCheckin?.valuationDate &&
                  snapshot.valuationStatus.alignedValuationDate <=
                    latestCheckin.valuationDate,
                )
              }
              onClick={persistCheckin}
            >
              {checkinSaving
                ? "保存中…"
                : baselineMode
                  ? "建立组合基线"
                  : "冻结本期变化"}
            </button>
          </div>
          {latestCheckin &&
            latestCheckin.baseCurrency === snapshot.profile.baseCurrency && (
              <button
                className="text-button attribution-reset"
                onClick={() =>
                  setCheckin({
                    ...checkin,
                    resetBaseline: !checkin.resetBaseline,
                    externalCashFlow: 0,
                  })
                }
              >
                {checkin.resetBaseline
                  ? "继续原有比较链"
                  : "估值口径变化？重新建立基线"}
              </button>
            )}
          {checkinError && (
            <div className="error-box">
              <AlertTriangle size={16} />
              {checkinError}
            </div>
          )}
          {checkins.length > 0 && (
            <details className="attribution-history">
              <summary>查看历史快照（{checkins.length}）</summary>
              <div>
                {checkins.slice(0, 8).map((item) => (
                  <article key={item.id}>
                    <div>
                      <strong>{item.periodLabel}</strong>
                      <small>
                        {item.valuationDate ?? "旧记录未保存估值日"} · 核验价{" "}
                        {item.holdingValuations.length}/{item.holdings.length} ·{" "}
                        {new Date(item.createdAt).toLocaleString("zh-CN")}
                      </small>
                    </div>
                    <span>
                      {formatMoney(item.totalValue, item.baseCurrency)}
                    </span>
                    <em>
                      {item.totalChange == null
                        ? "组合基线"
                        : `变化 ${item.totalChange >= 0 ? "+" : ""}${formatMoney(item.totalChange, item.baseCurrency)} · 净现金流 ${item.externalCashFlow >= 0 ? "+" : ""}${formatMoney(item.externalCashFlow, item.baseCurrency)} · 近似回报 ${item.modifiedDietzReturnPct == null ? "—" : `${item.modifiedDietzReturnPct.toFixed(2)}%`}`}
                    </em>
                  </article>
                ))}
              </div>
            </details>
          )}
          <p className="effectiveness-disclaimer">
            若持仓缺失、币种未换算、估值日期不同或录入错误，残差也会变化。它只能帮助分离外部现金流，不能替代时间加权收益率或完整业绩归因。
          </p>
        </div>
      </details>
    </div>
  );
}
