import { useEffect, useState } from "react";
import { useRequestGuard } from "../../lib/useRequestGuard";
import { LoadState } from "../../components/LoadState";
import {
  AlertTriangle,
  Bell,
  BellOff,
  ChevronRight,
  History,
  Save,
  Sparkles,
} from "lucide-react";
import {
  getDecisions,
  getInvestmentRules,
  getInvestmentRuleHistory,
  getReviewReminders,
  getRuleEffectiveness,
  getSystemReviews,
  saveInvestmentRule,
  saveSystemReview,
  updateInvestmentRule,
} from "../../api";
import type {
  DecisionRecord,
  InvestmentRule,
  InvestmentRuleInput,
  InvestmentRuleRevision,
  SystemReviewInput,
  SystemReviewRecord,
  ReviewReminderSummary,
  RuleEffectivenessSummary,
} from "../../types";
import {
  checkAndSendReviewReminder,
  disableReviewReminders,
  enableReviewReminders,
  isNativeApp,
} from "../../reminders";
import { PageHeader } from "../../components/PageHeader";
import { localDateValue } from "../../lib/dates";
import { View } from "../../app/navigation";
import { formatMoney } from "../../lib/format";

export const emptyRule: InvestmentRuleInput = {
  category: "风险",
  statement: "",
  trigger: "",
  rationale: "",
  active: true,
};

export function defaultSystemReview(): SystemReviewInput {
  const now = new Date();
  const quarter = Math.floor(now.getMonth() / 3) + 1;
  const next = new Date(now);
  next.setMonth(next.getMonth() + 3);
  return {
    periodLabel: `${now.getFullYear()} Q${quarter}`,
    adherenceScore: 3,
    processSummary: "",
    ruleViolations: "",
    lessons: "",
    nextActions: "",
    nextReviewDate: localDateValue(next),
  };
}

export function ruleInput(rule: InvestmentRule): InvestmentRuleInput {
  return {
    category: rule.category,
    statement: rule.statement,
    trigger: rule.trigger,
    rationale: rule.rationale,
    active: rule.active,
    sourceReviewId: rule.sourceReviewId,
  };
}

export function ReviewCenter({
  navigate,
  flash,
}: {
  navigate: (v: View) => void;
  flash: (s: string) => void;
}) {
  const [decisions, setDecisions] = useState<DecisionRecord[]>([]);
  const [rules, setRules] = useState<InvestmentRule[]>([]);
  const [reviews, setReviews] = useState<SystemReviewRecord[]>([]);
  const [review, setReview] = useState<SystemReviewInput>(defaultSystemReview);
  const [rule, setRule] = useState<InvestmentRuleInput>(emptyRule);
  const [editingRuleId, setEditingRuleId] = useState<string | null>(null);
  const [expandedRuleId, setExpandedRuleId] = useState<string | null>(null);
  const [ruleHistories, setRuleHistories] = useState<
    Record<string, InvestmentRuleRevision[]>
  >({});
  const [saving, setSaving] = useState(false);
  const [reminder, setReminder] = useState<ReviewReminderSummary | null>(null);
  const [effectiveness, setEffectiveness] =
    useState<RuleEffectivenessSummary | null>(null);
  const [reminderSaving, setReminderSaving] = useState(false);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  const loadRequest = useRequestGuard();

  const refresh = async () => {
    const isCurrent = loadRequest.begin();
    setLoading(true);
    try {
      const [
        nextDecisions,
        nextRules,
        nextReviews,
        nextReminder,
        nextEffectiveness,
      ] = await Promise.all([
        getDecisions(),
        getInvestmentRules(),
        getSystemReviews(),
        getReviewReminders().catch(() => null),
        getRuleEffectiveness().catch(() => null),
      ]);
      if (!isCurrent()) return;
      setDecisions(nextDecisions);
      setRules(nextRules);
      setReviews(nextReviews);
      setReminder(nextReminder);
      setEffectiveness(nextEffectiveness);
      setError("");
    } catch (nextError) {
      if (isCurrent()) setError(String(nextError));
    } finally {
      if (isCurrent()) setLoading(false);
    }
  };

  useEffect(() => {
    void refresh();
  }, []);

  const today = localDateValue(new Date());
  const dueDecisions = decisions.filter(
    (item) => !item.review && item.reviewDate && item.reviewDate <= today,
  );
  const activeRules = rules.filter((item) => item.active);
  const averageAdherence = reviews.length
    ? reviews.reduce((sum, item) => sum + item.adherenceScore, 0) /
      reviews.length
    : null;
  const latestReview = reviews[0];
  const periodicReviewDue =
    !latestReview || latestReview.nextReviewDate <= today;

  const persistSystemReview = async () => {
    if (
      !review.periodLabel ||
      !review.processSummary ||
      !review.lessons ||
      !review.nextActions ||
      !review.nextReviewDate
    )
      return;
    setSaving(true);
    setError("");
    try {
      await saveSystemReview(review);
      setReview(defaultSystemReview());
      await refresh();
      flash("周期复盘已冻结，并保留当时的组合与方法快照");
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setSaving(false);
    }
  };

  const persistRule = async () => {
    if (!rule.statement || !rule.trigger || !rule.rationale) return;
    setSaving(true);
    setError("");
    try {
      if (editingRuleId) await updateInvestmentRule(editingRuleId, rule);
      else await saveInvestmentRule(rule);
      if (editingRuleId) {
        setRuleHistories((current) => {
          const next = { ...current };
          delete next[editingRuleId];
          return next;
        });
        setExpandedRuleId(null);
      }
      setRule(emptyRule);
      setEditingRuleId(null);
      await refresh();
      flash(
        editingRuleId ? "规则已产生新版本，旧版本仍保留" : "个人投资规则已建立",
      );
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setSaving(false);
    }
  };

  const toggleRule = async (item: InvestmentRule) => {
    setSaving(true);
    setError("");
    try {
      await updateInvestmentRule(item.id, {
        ...ruleInput(item),
        active: !item.active,
      });
      setRuleHistories((current) => {
        const next = { ...current };
        delete next[item.id];
        return next;
      });
      setExpandedRuleId(null);
      await refresh();
      flash(item.active ? "规则已停用，历史版本仍保留" : "规则已重新启用");
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setSaving(false);
    }
  };

  const toggleRuleHistory = async (item: InvestmentRule) => {
    if (expandedRuleId === item.id) {
      setExpandedRuleId(null);
      return;
    }
    setExpandedRuleId(item.id);
    if (ruleHistories[item.id]) return;
    try {
      const history = await getInvestmentRuleHistory(item.id);
      setRuleHistories((current) => ({ ...current, [item.id]: history }));
    } catch (nextError) {
      setError(String(nextError));
    }
  };

  const convertLessonToRule = (item: SystemReviewRecord) => {
    setEditingRuleId(null);
    setRule({
      category: "复盘",
      statement: "",
      trigger: "下次遇到相似决策时",
      rationale: item.lessons,
      active: true,
      sourceReviewId: item.id,
    });
    window.requestAnimationFrame(() =>
      document
        .querySelector(".rule-editor")
        ?.scrollIntoView({ behavior: "smooth", block: "center" }),
    );
  };

  const startAiReview = () => {
    window.sessionStorage.setItem(
      "mario.advisorQuestion",
      "请基于我的个人投资规则、最近周期复盘、财务目标、当前组合和历史决策，完成一次系统复盘：先核对规则违反与风险边界，再识别重复错误，比较至少两种改进路径，并给出下一周期可验证的行动与证伪条件。",
    );
    navigate("advisor");
  };

  const toggleReminders = async () => {
    setReminderSaving(true);
    setError("");
    try {
      if (reminder?.enabled) {
        await disableReviewReminders();
        flash("桌面复盘提醒已关闭，本机到期队列仍会保留");
      } else {
        await enableReviewReminders();
        await checkAndSendReviewReminder();
        flash("桌面复盘提醒已开启");
      }
      await refresh();
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setReminderSaving(false);
    }
  };

  if (
    loading ||
    (error && !decisions.length && !rules.length && !reviews.length)
  )
    return (
      <div className="page narrow">
        <PageHeader
          eyebrow="复盘"
          title="读取复盘与规则"
          description="读取失败不等于没有待复盘记录。"
        />
        <LoadState
          loading={loading}
          error={error}
          onRetry={() => void refresh()}
        />
      </div>
    );

  return (
    <div className="page narrow">
      <PageHeader
        eyebrow="方法论 · 校准层"
        title="让经验沉淀为规则"
        description="周期复盘不是解释盈亏，而是检查纪律、修订规则并冻结当时的证据。"
        action={
          <button className="primary" onClick={startAiReview}>
            <Sparkles size={16} />
            AI 辅助系统复盘
          </button>
        }
      />
      <section className="review-metrics">
        <article>
          <span>到期待复盘</span>
          <strong className={dueDecisions.length ? "warning-text" : ""}>
            {dueDecisions.length}
          </strong>
          <small>按原始证伪条件回看</small>
        </article>
        <article>
          <span>周期复盘</span>
          <strong>{reviews.length}</strong>
          <small className={periodicReviewDue ? "warning-text" : ""}>
            {periodicReviewDue
              ? "现在需要安排一次"
              : `下次 ${latestReview.nextReviewDate}`}
          </small>
        </article>
        <article>
          <span>有效规则</span>
          <strong>{activeRules.length}</strong>
          <small>{rules.length - activeRules.length} 条历史停用规则</small>
        </article>
        <article>
          <span>平均纪律评分</span>
          <strong>
            {averageAdherence === null ? "—" : averageAdherence.toFixed(1)}
          </strong>
          <small>只评价是否按流程行动</small>
        </article>
      </section>

      <section className="panel reminder-panel">
        <div className="reminder-copy">
          <div
            className={`reminder-icon ${reminder?.enabled ? "enabled" : ""}`}
          >
            {reminder?.enabled ? <Bell size={18} /> : <BellOff size={18} />}
          </div>
          <div>
            <strong>桌面复盘提醒</strong>
            <p>
              应用打开时检查到期事项；相同到期状态每天最多提醒一次，锁屏通知不显示资产名称。
            </p>
            <small>
              设置只保存在这台设备，不参与云端同步。关闭应用后不会在后台运行。
            </small>
          </div>
        </div>
        <button
          className={reminder?.enabled ? "secondary" : "primary"}
          disabled={reminderSaving || !isNativeApp()}
          onClick={toggleReminders}
        >
          {reminderSaving
            ? "处理中…"
            : reminder?.enabled
              ? "关闭提醒"
              : isNativeApp()
                ? "开启提醒"
                : "仅原生应用可用"}
        </button>
      </section>

      <section className="panel rule-effectiveness-panel">
        <div className="panel-title">
          <div>
            <span>规则有效性追踪</span>
            <h2>观察纪律与过程质量的关系</h2>
          </div>
          <small className="causality-note">只显示关联，不宣称因果</small>
        </div>
        <div className="effectiveness-summary">
          <article>
            <span>规则检查覆盖</span>
            <strong>
              {effectiveness?.totalDecisions
                ? `${effectiveness.evaluatedDecisions}/${effectiveness.totalDecisions}`
                : "—"}
            </strong>
            <small>启用此能力后的决策才计入</small>
          </article>
          <article>
            <span>适用规则遵守率</span>
            <strong>
              {effectiveness?.adherencePct == null
                ? "—"
                : `${effectiveness.adherencePct.toFixed(0)}%`}
            </strong>
            <small>不适用规则不进入分母</small>
          </article>
          <article>
            <span>已复盘规则样本</span>
            <strong>{effectiveness?.reviewedChecks ?? 0}</strong>
            <small>按决策过程评分比较</small>
          </article>
        </div>
        {!effectiveness?.rules.length && (
          <div className="empty">
            建立个人投资规则后，每次冻结决策都会先要求逐条确认。
          </div>
        )}
        <div className="effectiveness-list">
          {effectiveness?.rules.map((item) => (
            <article
              key={item.ruleId}
              className={item.active ? "" : "inactive"}
            >
              <div className="effectiveness-rule">
                <span>
                  {item.category} · 当前 v{item.currentRevision}
                  {item.active ? "" : " · 已停用"}
                </span>
                <strong>{item.statement}</strong>
                <small>
                  {item.observedRevisions.length
                    ? `已有决策覆盖版本 ${item.observedRevisions.join("、")}`
                    : "尚无决策样本"}
                </small>
              </div>
              <div className="effectiveness-counts">
                <span>
                  适用 <b>{item.applicableCount}</b>
                </span>
                <span>
                  遵守 <b>{item.followedCount}</b>
                </span>
                <span>
                  偏离 <b>{item.deviatedCount}</b>
                </span>
                <span>
                  已复盘 <b>{item.reviewedCount}</b>
                </span>
              </div>
              <div className="process-comparison">
                <span>
                  遵守后的过程评分{" "}
                  <b>
                    {item.followedProcessAverage == null
                      ? "—"
                      : item.followedProcessAverage.toFixed(1)}
                  </b>
                </span>
                <span>
                  偏离后的过程评分{" "}
                  <b>
                    {item.deviatedProcessAverage == null
                      ? "—"
                      : item.deviatedProcessAverage.toFixed(1)}
                  </b>
                </span>
                <strong
                  className={
                    item.signal.startsWith("反常") ? "warning-text" : ""
                  }
                >
                  {item.signal}
                </strong>
              </div>
            </article>
          ))}
        </div>
        <p className="effectiveness-disclaimer">
          过程评分也可能受规则本身影响，样本存在选择偏差。这里用于发现值得复核的规则，不用于证明某条规则提高收益。
        </p>
      </section>

      <LoadState
        loading={loading}
        error={error}
        onRetry={() => void refresh()}
      />

      {dueDecisions.length > 0 && (
        <section className="panel due-review-panel">
          <div className="panel-title">
            <div>
              <span>复盘队列</span>
              <h2>先处理已经到期的原始判断</h2>
            </div>
            <button className="secondary" onClick={() => navigate("decision")}>
              前往决策日志
            </button>
          </div>
          <div className="due-review-list">
            {dueDecisions.map((item) => (
              <div key={item.id}>
                <strong>{item.assetName}</strong>
                <span>置信度 {item.confidencePct}%</span>
                <span>计划复盘日 {item.reviewDate}</span>
                <small>{item.invalidation}</small>
              </div>
            ))}
          </div>
        </section>
      )}

      <section className="panel form-panel system-review-form">
        <div className="panel-title">
          <div>
            <span>周期系统复盘</span>
            <h2>冻结这一周期的过程与约束</h2>
          </div>
          <History size={21} className="muted-icon" />
        </div>
        <div className="form-grid">
          <label>
            <span>复盘周期</span>
            <input
              value={review.periodLabel}
              onChange={(e) =>
                setReview({ ...review, periodLabel: e.target.value })
              }
              placeholder="例如 2026 Q3"
            />
          </label>
          <label>
            <span>纪律执行评分</span>
            <select
              value={review.adherenceScore}
              onChange={(e) =>
                setReview({ ...review, adherenceScore: Number(e.target.value) })
              }
            >
              {[1, 2, 3, 4, 5].map((value) => (
                <option key={value} value={value}>
                  {value} / 5
                </option>
              ))}
            </select>
          </label>
          <label>
            <span>下次复盘日</span>
            <input
              type="date"
              value={review.nextReviewDate}
              onChange={(e) =>
                setReview({ ...review, nextReviewDate: e.target.value })
              }
            />
          </label>
          <label className="span-3">
            <span>这一周期实际执行了什么？</span>
            <textarea
              value={review.processSummary}
              onChange={(e) =>
                setReview({ ...review, processSummary: e.target.value })
              }
              placeholder="只写事实：投入、再平衡、研究和计划外交易。"
            />
          </label>
          <label className="span-3">
            <span>违反了哪些预设规则？</span>
            <textarea
              value={review.ruleViolations}
              onChange={(e) =>
                setReview({ ...review, ruleViolations: e.target.value })
              }
              placeholder="没有则写“无”；不要用盈利为违规行为辩护。"
            />
          </label>
          <label className="span-3">
            <span>哪些认知需要修正？</span>
            <textarea
              value={review.lessons}
              onChange={(e) =>
                setReview({ ...review, lessons: e.target.value })
              }
              placeholder="区分可重复的经验与一次性噪声。"
            />
          </label>
          <label className="span-3">
            <span>下一周期只做哪些行动？</span>
            <textarea
              value={review.nextActions}
              onChange={(e) =>
                setReview({ ...review, nextActions: e.target.value })
              }
              placeholder="使用可检查的动作、期限和触发条件。"
            />
          </label>
        </div>
        <div className="form-actions">
          <p>保存时会同时冻结组合、目标、风险和决策完成度摘要。</p>
          <button
            className="primary"
            disabled={
              saving ||
              !review.processSummary ||
              !review.lessons ||
              !review.nextActions
            }
            onClick={persistSystemReview}
          >
            <Save size={16} />
            冻结周期复盘
          </button>
        </div>
      </section>

      <section className="panel rule-workbench">
        <div className="panel-title">
          <div>
            <span>个人投资规则</span>
            <h2>把经验写成触发时能执行的动作</h2>
          </div>
          <span className="history-count">{activeRules.length} 条有效</span>
        </div>
        <div className="rule-layout">
          <div className="rule-list">
            {rules.length === 0 && (
              <div className="empty">
                还没有个人规则。好的规则应说明“何时触发、具体做什么、为什么”。
              </div>
            )}
            {rules.map((item) => (
              <article key={item.id} className={item.active ? "" : "inactive"}>
                <div>
                  <span>
                    {item.category} · v{item.revision}
                  </span>
                  <strong>{item.statement}</strong>
                  <p>
                    <b>触发</b>
                    {item.trigger}
                  </p>
                  <small>{item.rationale}</small>
                </div>
                <div className="rule-actions">
                  <button
                    className="text-button"
                    onClick={() => toggleRuleHistory(item)}
                  >
                    历史
                  </button>
                  <button
                    className="text-button"
                    onClick={() => {
                      setEditingRuleId(item.id);
                      setRule(ruleInput(item));
                    }}
                  >
                    修订
                  </button>
                  <button
                    className="text-button"
                    disabled={saving}
                    onClick={() => toggleRule(item)}
                  >
                    {item.active ? "停用" : "启用"}
                  </button>
                </div>
                {expandedRuleId === item.id && (
                  <div className="rule-history">
                    {(ruleHistories[item.id] ?? []).map((revision) => (
                      <div key={revision.revision}>
                        <span>
                          v{revision.revision} ·{" "}
                          {new Date(revision.changedAt).toLocaleDateString(
                            "zh-CN",
                          )}
                        </span>
                        <strong>{revision.statement}</strong>
                        <small>
                          {revision.active ? "当时启用" : "当时停用"}
                        </small>
                      </div>
                    ))}
                  </div>
                )}
              </article>
            ))}
          </div>
          <div className="rule-editor">
            <strong>
              {editingRuleId
                ? "修订规则"
                : rule.sourceReviewId
                  ? "从复盘沉淀规则"
                  : "建立一条规则"}
            </strong>
            <label>
              <span>类别</span>
              <select
                value={rule.category}
                onChange={(e) =>
                  setRule({
                    ...rule,
                    category: e.target.value as InvestmentRuleInput["category"],
                  })
                }
              >
                {["资产配置", "风险", "研究", "仓位", "行为", "复盘"].map(
                  (value) => (
                    <option key={value}>{value}</option>
                  ),
                )}
              </select>
            </label>
            <label>
              <span>规则内容</span>
              <textarea
                value={rule.statement}
                onChange={(e) =>
                  setRule({ ...rule, statement: e.target.value })
                }
                placeholder="例如：单一主动仓位不得超过 8%。"
              />
            </label>
            <label>
              <span>触发条件</span>
              <textarea
                value={rule.trigger}
                onChange={(e) => setRule({ ...rule, trigger: e.target.value })}
                placeholder="什么时候必须检查这条规则？"
              />
            </label>
            <label>
              <span>依据</span>
              <textarea
                value={rule.rationale}
                onChange={(e) =>
                  setRule({ ...rule, rationale: e.target.value })
                }
                placeholder="它避免哪一种重复错误？"
              />
            </label>
            <div className="rule-editor-actions">
              {(editingRuleId || rule.sourceReviewId) && (
                <button
                  className="text-button"
                  onClick={() => {
                    setEditingRuleId(null);
                    setRule(emptyRule);
                  }}
                >
                  取消
                </button>
              )}
              <button
                className="secondary"
                disabled={
                  saving || !rule.statement || !rule.trigger || !rule.rationale
                }
                onClick={persistRule}
              >
                {editingRuleId ? "保存新版本" : "建立规则"}
              </button>
            </div>
          </div>
        </div>
      </section>

      {reviews.length > 0 && (
        <section className="panel system-review-history">
          <div className="panel-title">
            <div>
              <span>冻结记录</span>
              <h2>用当时的事实检验方法是否进步</h2>
            </div>
            <span className="history-count">{reviews.length} 期</span>
          </div>
          <div className="system-review-list">
            {reviews.map((item) => (
              <article key={item.id}>
                <div className="system-review-head">
                  <div>
                    <span>{item.periodLabel}</span>
                    <strong>纪律 {item.adherenceScore}/5</strong>
                  </div>
                  <small>
                    {new Date(item.createdAt).toLocaleDateString("zh-CN")}
                  </small>
                </div>
                <p>
                  <b>过程事实</b>
                  {item.processSummary}
                </p>
                <p>
                  <b>规则违反</b>
                  {item.ruleViolations || "无"}
                </p>
                <p>
                  <b>经验修正</b>
                  {item.lessons}
                </p>
                <p>
                  <b>下一步</b>
                  {item.nextActions}
                </p>
                <div className="frozen-snapshot">
                  <span>
                    组合{" "}
                    {item.snapshot.portfolioComparable
                      ? formatMoney(
                          item.snapshot.portfolioValue,
                          item.snapshot.baseCurrency,
                        )
                      : "待补汇率"}
                  </span>
                  <span>
                    集中度{" "}
                    {item.snapshot.portfolioComparable
                      ? `${item.snapshot.concentrationPct.toFixed(1)}%`
                      : "—"}
                  </span>
                  <span>高风险 {item.snapshot.highRiskFindings}</span>
                  <span>
                    目标 {item.snapshot.goalsOnTrack}/{item.snapshot.goalTotal}
                  </span>
                </div>
                <button
                  className="text-button"
                  onClick={() => convertLessonToRule(item)}
                >
                  把经验沉淀为规则 <ChevronRight size={14} />
                </button>
              </article>
            ))}
          </div>
        </section>
      )}
    </div>
  );
}
