import { useEffect, useRef, useState } from "react";
import { AlertTriangle, ArrowRight, History } from "lucide-react";
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
import { AssetHistory } from "../portfolio/AssetHistory";
import { FinancialSummary } from "./FinancialSummary";
import { HoldingAnalysis } from "./HoldingAnalysis";
import { InvestmentActivity } from "./InvestmentActivity";
import { DATA_SAVED, SERVER_DATA_UPDATED } from "../../lib/syncEvents";
import { formatMoney } from "../../lib/format";
import { goalStatus } from "../portfolio/presentation";

export function Dashboard({
  snapshot,
  navigate,
  flash,
}: {
  snapshot: Snapshot;
  navigate: (view: View, section?: FoundationSection) => void;
  flash: (message: string) => void;
}) {
  const [refresh, setRefresh] = useState(0);
  const [checkins, setCheckins] = useState<PortfolioCheckInRecord[]>([]);
  const [reminders, setReminders] = useState<ReviewReminderSummary | null>(
    null,
  );
  const action = nextAction(snapshot, reminders);
  useEffect(() => {
    let active = true;
    setReminders(null);
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
  }, [snapshot.updatedAt, refresh]);
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
  const [checkinsLoading, setCheckinsLoading] = useState(true);
  const [checkinLoadError, setCheckinLoadError] = useState("");
  const checkinRequest = useRef(0);
  useEffect(() => {
    const wake = () => setRefresh((value) => value + 1);
    window.addEventListener(DATA_SAVED, wake);
    window.addEventListener(SERVER_DATA_UPDATED, wake);
    return () => {
      window.removeEventListener(DATA_SAVED, wake);
      window.removeEventListener(SERVER_DATA_UPDATED, wake);
    };
  }, []);

  const latestCheckin = checkins[0];
  const baselineMode =
    !latestCheckin ||
    !latestCheckin.valuationDate ||
    latestCheckin.baseCurrency !== snapshot.profile.baseCurrency ||
    checkin.resetBaseline;

  useEffect(() => {
    const request = ++checkinRequest.current;
    setCheckinsLoading(true);
    setCheckinLoadError("");
    getPortfolioCheckins()
      .then((records) => {
        if (request !== checkinRequest.current) return;
        setCheckins(records);
      })
      .catch((error) => {
        if (request !== checkinRequest.current) return;
        setCheckins([]);
        setCheckinLoadError(String(error));
      })
      .finally(() => {
        if (request === checkinRequest.current) setCheckinsLoading(false);
      });
    return () => {
      checkinRequest.current++;
    };
  }, [snapshot.updatedAt, refresh]);

  const persistCheckin = async () => {
    if (
      !checkin.periodLabel.trim() ||
      checkinSaving ||
      checkinsLoading ||
      checkinLoadError
    )
      return;
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
    <div className="page dashboard-page">
      <PageHeader
        title="分析看板"
        description="基于你已提供的资产、收支、目标和投资记录，观察财务状况与投资过程。"
        action={
          <div className="dashboard-actions">
            <button className="secondary" onClick={() => navigate("facts")}>
              更新我的情况
            </button>
            <button
              className="primary"
              onClick={() => {
                window.sessionStorage.setItem(
                  "mario.advisorQuestion",
                  "请基于我已提供的资产、负债、收支、持仓成本、目标、组合流水和复盘记录，做一次整体资产与投资分析。分析资产配置、流动性、集中度、外币风险、投入能力、目标缺口和投资纪律，区分资产变化、现金流与投资收益，指出数据不足，并给出有依据、可复核的下一步建议。",
                );
                window.sessionStorage.removeItem("mario.dailyAssetRange");
                navigate("advisor");
              }}
            >
              继续做整体分析
            </button>
          </div>
        }
      />
      <FinancialSummary snapshot={snapshot} />

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

      <AssetHistory snapshot={snapshot} />
      <HoldingAnalysis snapshot={snapshot} navigate={navigate} />
      <section className="panel dashboard-section">
        <div className="panel-title">
          <h2>分析数据完整度</h2>
        </div>
        <dl className="dashboard-facts">
          <div>
            <dt>基准币种</dt>
            <dd>{snapshot.profile.baseCurrency}</dd>
          </div>
          <div>
            <dt>最近资料更新</dt>
            <dd>{new Date(snapshot.updatedAt).toLocaleString("zh-CN")}</dd>
          </div>
          <div>
            <dt>缺少汇率的持仓</dt>
            <dd>{snapshot.valuationStatus.missingFxHoldings.length} 项</dd>
          </div>
          <div>
            <dt>缺少估值日期</dt>
            <dd>{snapshot.valuationStatus.undatedHoldingCount} 项</dd>
          </div>
          <div>
            <dt>统一估值日</dt>
            <dd>
              {snapshot.valuationStatus.alignedValuationDate ?? "尚未对齐"}
            </dd>
          </div>
        </dl>
        <p className="dashboard-note">
          汇率完整时可汇总资产。冻结期间回报还需要同日估值、相邻快照和完整流水。
        </p>
      </section>

      <section className="panel dashboard-section">
        <div className="panel-title">
          <div>
            <h2>目标进度</h2>
          </div>
          <button
            className="text-button"
            onClick={() => navigate("foundation", "goals")}
          >
            管理目标 →
          </button>
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
                  <span className={goal.status}>{goalStatus(goal.status)}</span>
                </div>
                <div className="projection-bar">
                  <i
                    style={{
                      width: `${Math.max(0, Math.min(100, goal.fundedPct))}%`,
                    }}
                  />
                </div>
                <div className="projection-stats">
                  <span>
                    已筹备 <b>{goal.fundedPct.toFixed(1)}%</b>
                  </span>
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
                <details className="inline-help">
                  <summary>投入缺口与模拟金额</summary>
                  <p>
                    所需月投入{" "}
                    {formatMoney(
                      goal.requiredMonthlyContribution,
                      snapshot.profile.baseCurrency,
                    )}{" "}
                    · 保守情景金额{" "}
                    {formatMoney(
                      goal.conservativeAmount,
                      snapshot.profile.baseCurrency,
                    )}{" "}
                    · 中位情景金额{" "}
                    {formatMoney(
                      goal.medianAmount,
                      snapshot.profile.baseCurrency,
                    )}
                  </p>
                  <p>上述金额与达成率来自示例模型，不是实际收益预测或保证。</p>
                </details>
              </div>
            ))}
          </div>
        )}
      </section>

      <InvestmentActivity snapshot={snapshot} navigate={navigate} />
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
                checkinsLoading ||
                Boolean(checkinLoadError) ||
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
          {checkinLoadError && (
            <div className="error-box" role="alert">
              组合快照读取失败，暂时不能冻结新记录。
              <button
                className="text-button"
                onClick={() => setRefresh((value) => value + 1)}
              >
                重试读取快照
              </button>
            </div>
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
          <p className="analysis-note">
            若持仓缺失、币种未换算、估值日期不同或录入错误，残差也会变化。它只能帮助分离外部现金流，不能替代时间加权收益率或完整业绩归因。
          </p>
        </div>
      </details>
    </div>
  );
}
