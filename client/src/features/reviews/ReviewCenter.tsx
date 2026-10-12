import { useEffect, useState } from "react";
import { useRequestGuard } from "../../lib/useRequestGuard";
import { LoadState } from "../../components/LoadState";
import {
  AlertTriangle,
  ChevronRight,
  History,
  Save,
  Sparkles,
} from "lucide-react";
import {
  getDecisions,
  getInvestmentRules,
  getInvestmentRuleHistory,
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
  SystemReviewRecord,
  RuleEffectivenessSummary,
} from "../../types";
import { PageHeader } from "../../components/PageHeader";
import { localDateValue } from "../../lib/dates";
import { View } from "../../app/navigation";
import { systemReviewText } from "../../lib/reviewText";

export const emptyRule: InvestmentRuleInput = {
  category: "风险",
  statement: "",
  trigger: "",
  rationale: "",
  active: true,
};

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
  const [reviewContent, setReviewContent] = useState("");
  const [rule, setRule] = useState<InvestmentRuleInput>(emptyRule);
  const [editingRuleId, setEditingRuleId] = useState<string | null>(null);
  const [expandedRuleId, setExpandedRuleId] = useState<string | null>(null);
  const [ruleHistories, setRuleHistories] = useState<
    Record<string, InvestmentRuleRevision[]>
  >({});
  const [saving, setSaving] = useState(false);
  const [effectiveness, setEffectiveness] =
    useState<RuleEffectivenessSummary | null>(null);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  const [loaded, setLoaded] = useState(false);
  const loadRequest = useRequestGuard();

  const refresh = async () => {
    const isCurrent = loadRequest.begin();
    setLoading(true);
    try {
      const [nextDecisions, nextRules, nextReviews, nextEffectiveness] =
        await Promise.all([
          getDecisions(),
          getInvestmentRules(),
          getSystemReviews(),
          getRuleEffectiveness().catch(() => null),
        ]);
      if (!isCurrent()) return;
      setLoaded(true);
      setDecisions(nextDecisions);
      setRules(nextRules);
      setReviews(nextReviews);
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
  const latestReview = reviews[0];
  const periodicReviewDue =
    !latestReview ||
    Boolean(
      latestReview.nextReviewDate && latestReview.nextReviewDate <= today,
    );

  const persistSystemReview = async () => {
    if (saving || !reviewContent.trim()) return;
    setSaving(true);
    setError("");
    try {
      await saveSystemReview({ content: reviewContent });
      setReviewContent("");
      await refresh();
      flash("复盘记录已保存");
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
      rationale: item.lessons || systemReviewText(item),
      active: true,
      sourceReviewId: item.id,
    });
    const workbench =
      document.querySelector<HTMLDetailsElement>(".rule-workbench");
    if (workbench) workbench.open = true;
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

  if (!loaded && (loading || error))
    return (
      <div className="page narrow">
        <PageHeader title="复盘记录" />
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
        title="复盘记录"
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
        </article>
        <article>
          <span>复盘记录</span>
          <strong>{reviews.length}</strong>
          <small className={periodicReviewDue ? "warning-text" : ""}>
            {periodicReviewDue
              ? "现在需要安排一次"
              : latestReview.nextReviewDate
                ? `下次 ${latestReview.nextReviewDate}`
                : "随时写下新的思考"}
          </small>
        </article>
        <article>
          <span>有效规则</span>
          <strong>{activeRules.length}</strong>
          <small>{rules.length - activeRules.length} 条历史停用规则</small>
        </article>
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
              <h2>到期待复盘</h2>
            </div>
            <button className="secondary" onClick={() => navigate("decision")}>
              前往决策确认
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
            <h2>新建复盘</h2>
          </div>
        </div>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            void persistSystemReview();
          }}
        >
          <label className="decision-content-field">
            <span>我的观点、顾虑和思考</span>
            <textarea
              value={reviewContent}
              rows={10}
              required
              disabled={saving}
              onChange={(event) => setReviewContent(event.target.value)}
              placeholder="自由写下你想表达的内容：你的观点、有哪些顾虑，以及这次复盘带来的思考。"
            />
          </label>
          <div className="form-actions">
            <button
              className="primary"
              type="submit"
              disabled={saving || !reviewContent.trim()}
            >
              <Save size={16} />
              {saving ? "保存中…" : "保存复盘"}
            </button>
          </div>
        </form>
      </section>

      <section className="panel system-review-history">
        <div className="panel-title">
          <div>
            <h2>历史复盘</h2>
          </div>
          <span className="history-count">{reviews.length} 条</span>
        </div>
        {reviews.length === 0 && (
          <div className="empty">
            还没有复盘记录，写下你的第一段观点、顾虑和思考。
          </div>
        )}
        <div className="system-review-list">
          {reviews.map((item) => (
            <article key={item.id}>
              <div className="system-review-head">
                <small>
                  {new Date(item.createdAt).toLocaleDateString("zh-CN")}
                </small>
              </div>
              <p className="review-content">{systemReviewText(item)}</p>
              <button
                className="text-button"
                onClick={() => convertLessonToRule(item)}
              >
                把思考沉淀为规则 <ChevronRight size={14} />
              </button>
            </article>
          ))}
        </div>
      </section>

      <details className="panel disclosure-panel rule-workbench">
        <summary>投资规则</summary>
        <div className="disclosure-content">
          <div className="panel-title">
            <div>
              <h2>投资规则</h2>
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
                <article
                  key={item.id}
                  className={item.active ? "" : "inactive"}
                >
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
                      category: e.target
                        .value as InvestmentRuleInput["category"],
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
                  onChange={(e) =>
                    setRule({ ...rule, trigger: e.target.value })
                  }
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
                    saving ||
                    !rule.statement ||
                    !rule.trigger ||
                    !rule.rationale
                  }
                  onClick={persistRule}
                >
                  {editingRuleId ? "保存新版本" : "建立规则"}
                </button>
              </div>
            </div>
          </div>
        </div>
      </details>

      <details className="panel disclosure-panel">
        <summary>规则表现与样本统计</summary>
        <div className="disclosure-content">
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
              这里汇总历史决策中的规则检查；新增纯文本决策可直接写下对规则的思考。
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
        </div>
      </details>
    </div>
  );
}
