import { useEffect, useMemo, useState } from "react";
import { AlertTriangle, BrainCircuit, FilePenLine, Save } from "lucide-react";
import {
  getDecisions,
  getInvestmentRules,
  saveDecision,
  saveDecisionReview,
} from "../../api";
import type {
  DecisionEntry,
  DecisionRecord,
  DecisionReview,
  DecisionRuleCheck,
  InvestmentRule,
  RuleCheckStatus,
} from "../../types";
import { PageHeader } from "../../components/PageHeader";
import { NumberField } from "../../components/NumberField";
import { localDateValue } from "../../lib/dates";
import { useRequestGuard } from "../../lib/useRequestGuard";

export const emptyDecision: DecisionEntry = {
  assetName: "",
  thesis: "",
  counterThesis: "",
  expectedReturnPct: 0,
  downsidePct: 0,
  confidencePct: 50,
  positionPct: 0,
  invalidation: "",
  reviewDate: "",
  ruleChecks: [],
};

function checksForRules(rules: InvestmentRule[]): DecisionRuleCheck[] {
  return rules
    .filter((rule) => rule.active)
    .map((rule) => ({
      ruleId: rule.id,
      ruleRevision: rule.revision,
      category: rule.category,
      statement: rule.statement,
      trigger: rule.trigger,
      status: "待确认",
      note: "",
    }));
}

export function DecisionJournal({
  flash,
  seed,
  clearSeed,
  onOpenAnalysis,
}: {
  flash: (s: string) => void;
  seed: DecisionEntry | null;
  clearSeed: () => void;
  onOpenAnalysis: (id: string) => void;
}) {
  const [entry, setEntry] = useState({ ...emptyDecision });
  const [records, setRecords] = useState<DecisionRecord[]>([]);
  const [rules, setRules] = useState<InvestmentRule[]>([]);
  const [reviewingId, setReviewingId] = useState<string | null>(null);
  const [review, setReview] = useState<DecisionReview>({
    outcomeSummary: "",
    thesisStatus: "尚不明确",
    processRating: 3,
    lessons: "",
  });
  const [saving, setSaving] = useState(false);
  const [rulesReady, setRulesReady] = useState(false);
  const [loading, setLoading] = useState(true);
  const loadRequest = useRequestGuard();
  const [error, setError] = useState("");
  const expectedValue =
    (entry.confidencePct / 100) * entry.expectedReturnPct -
    (1 - entry.confidencePct / 100) * Math.abs(entry.downsidePct);

  const refresh = async () => {
    const isCurrent = loadRequest.begin();
    setLoading(true);
    setRulesReady(false);
    try {
      const [nextRecords, nextRules] = await Promise.all([
        getDecisions(),
        getInvestmentRules(),
      ]);
      if (!isCurrent()) return;
      setRecords(nextRecords);
      setRules(nextRules);
      setEntry((current) =>
        current.ruleChecks?.length
          ? current
          : { ...current, ruleChecks: checksForRules(nextRules) },
      );
      setError("");
      setRulesReady(true);
    } catch (nextError) {
      if (isCurrent()) setError(String(nextError));
    } finally {
      if (isCurrent()) setLoading(false);
    }
  };

  useEffect(() => {
    void refresh();
  }, []);
  // A late rules response must enrich the draft, never replace user edits.
  useEffect(() => {
    if (seed) setEntry({ ...seed, ruleChecks: checksForRules(rules) });
  }, [seed]);

  const activeRules = rules.filter((rule) => rule.active);
  const incompleteRuleChecks =
    !rulesReady ||
    activeRules.length !== (entry.ruleChecks?.length ?? 0) ||
    (entry.ruleChecks ?? []).some(
      (check) =>
        check.status === "待确认" ||
        (check.status === "偏离" && !check.note.trim()),
    );

  const updateRuleCheck = (
    ruleId: string,
    update: Partial<DecisionRuleCheck>,
  ) => {
    setEntry({
      ...entry,
      ruleChecks: (entry.ruleChecks ?? []).map((check) =>
        check.ruleId === ruleId ? { ...check, ...update } : check,
      ),
    });
  };

  const reviewed = records.filter((item) => item.review);
  const calibration = useMemo(() => {
    const measurable = reviewed.filter(
      (item) =>
        item.review?.thesisStatus === "成立" ||
        item.review?.thesisStatus === "失效",
    );
    if (!measurable.length) return { count: 0, error: null };
    const brier =
      measurable.reduce((sum, item) => {
        const outcome = item.review?.thesisStatus === "成立" ? 1 : 0;
        return sum + Math.pow(item.confidencePct / 100 - outcome, 2);
      }, 0) / measurable.length;
    return { count: measurable.length, error: brier };
  }, [reviewed]);

  const processAverage = reviewed.length
    ? reviewed.reduce(
        (sum, item) => sum + (item.review?.processRating ?? 0),
        0,
      ) / reviewed.length
    : null;

  const persist = async () => {
    if (
      !entry.assetName ||
      !entry.thesis ||
      !entry.counterThesis ||
      !entry.invalidation ||
      !entry.reviewDate ||
      incompleteRuleChecks
    )
      return;
    setSaving(true);
    try {
      await saveDecision(entry);
      flash("决策快照已冻结，可用于未来复盘");
      setEntry({ ...emptyDecision, ruleChecks: checksForRules(rules) });
      clearSeed();
      await refresh();
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setSaving(false);
    }
  };

  const beginReview = (record: DecisionRecord) => {
    setReviewingId(record.id);
    setReview(
      record.review ?? {
        outcomeSummary: "",
        thesisStatus: "尚不明确",
        processRating: 3,
        lessons: "",
      },
    );
  };

  const persistReview = async () => {
    if (!reviewingId || !review.outcomeSummary || !review.lessons) return;
    setSaving(true);
    try {
      await saveDecisionReview(reviewingId, review);
      await refresh();
      setReviewingId(null);
      flash("复盘已保存，判断校准数据已更新");
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="page narrow">
      <PageHeader title="决策日志" description="记录判断，按期复盘。" />
      <section className="review-metrics">
        <article>
          <span>决策记录</span>
          <strong>{records.length}</strong>
        </article>
        <article>
          <span>已完成复盘</span>
          <strong>{reviewed.length}</strong>
          <small>
            {records.length
              ? `${Math.round((reviewed.length / records.length) * 100)}% 完成率`
              : "等待第一条记录"}
          </small>
        </article>
        <article>
          <span>概率误差</span>
          <strong>
            {calibration.error === null ? "—" : calibration.error.toFixed(3)}
          </strong>
          <small>{`${calibration.count} 个明确成立/失效样本 · 非能力评分`}</small>
        </article>
        <article>
          <span>过程评分</span>
          <strong>
            {processAverage === null ? "—" : processAverage.toFixed(1)}
          </strong>
          <small>独立于实际盈亏</small>
        </article>
      </section>
      <section className="panel form-panel decision-card">
        <div className="panel-title">
          <div>
            <h2>新建决策</h2>
          </div>
        </div>
        {entry.sourceAnalysisId && (
          <div className="decision-draft-notice">
            <BrainCircuit size={18} />
            <div>
              <strong>来自 AI 分析的待确认草稿</strong>
              <p>
                只复制了文字线索。投资对象、收益、损失、置信度、仓位和复盘日期必须由你判断；点击“冻结”前不会形成决策。
              </p>
            </div>
            <button
              className="text-button"
              onClick={() => {
                setEntry({ ...emptyDecision });
                clearSeed();
              }}
            >
              放弃草稿
            </button>
          </div>
        )}
        <div className="form-grid">
          <label className="span-2">
            <span>投资对象</span>
            <input
              value={entry.assetName}
              onChange={(e) =>
                setEntry({ ...entry, assetName: e.target.value })
              }
              placeholder="你真正购买的是什么？"
            />
          </label>
          <label className="span-2">
            <span>核心逻辑</span>
            <textarea
              value={entry.thesis}
              onChange={(e) => setEntry({ ...entry, thesis: e.target.value })}
              placeholder="收益从哪里来？市场可能错在哪里？"
            />
          </label>
          <label className="span-2">
            <span>最强反方观点</span>
            <textarea
              value={entry.counterThesis}
              onChange={(e) =>
                setEntry({ ...entry, counterThesis: e.target.value })
              }
              placeholder="站在反方立场，什么最可能让这笔投资失败？"
            />
          </label>
          <NumberField
            label="上行情景收益"
            value={entry.expectedReturnPct}
            onChange={(v) =>
              setEntry({ ...entry, expectedReturnPct: Number(v) })
            }
            suffix="%"
          />
          <NumberField
            label="下行情景损失"
            value={entry.downsidePct}
            onChange={(v) => setEntry({ ...entry, downsidePct: Number(v) })}
            suffix="%"
          />
          <NumberField
            label="判断置信度"
            value={entry.confidencePct}
            onChange={(v) => setEntry({ ...entry, confidencePct: Number(v) })}
            suffix="%"
          />
          <NumberField
            label="计划仓位"
            value={entry.positionPct}
            onChange={(v) => setEntry({ ...entry, positionPct: Number(v) })}
            suffix="%"
          />
          <label className="span-2">
            <span>证伪条件</span>
            <textarea
              value={entry.invalidation}
              onChange={(e) =>
                setEntry({ ...entry, invalidation: e.target.value })
              }
              placeholder="出现什么证据时，你会承认原始判断已经失效？"
            />
          </label>
          <label>
            <span>计划复盘日</span>
            <input
              type="date"
              value={entry.reviewDate}
              onChange={(e) =>
                setEntry({ ...entry, reviewDate: e.target.value })
              }
            />
          </label>
          <div
            className={`ev-card ${expectedValue >= 0 ? "positive-bg" : "negative-bg"}`}
          >
            <span>粗略概率加权结果</span>
            <strong>
              {expectedValue > 0 ? "+" : ""}
              {expectedValue.toFixed(1)}%
            </strong>
            <small>仅作思考校准，不代表预测</small>
          </div>
        </div>
        <div className="rule-check-workbench">
          <div className="rule-check-title">
            <div>
              <span>决策前规则检查</span>
              <strong>逐条确认，而不是事后声称自己遵守了纪律</strong>
            </div>
            <small>
              {activeRules.length
                ? `${activeRules.length} 条当前有效规则`
                : "当前没有有效规则"}
            </small>
          </div>
          {activeRules.length === 0 && (
            <div className="empty">
              还没有需要确认的个人规则。可在“复盘与规则”中把经验沉淀成可执行约束。
            </div>
          )}
          <div className="rule-check-list">
            {(entry.ruleChecks ?? []).map((check) => (
              <article key={check.ruleId} className={`status-${check.status}`}>
                <div>
                  <span>
                    {check.category} · v{check.ruleRevision}
                  </span>
                  <strong>{check.statement}</strong>
                  <small>触发：{check.trigger}</small>
                </div>
                <label>
                  <span>本次判断</span>
                  <select
                    value={check.status}
                    onChange={(event) =>
                      updateRuleCheck(check.ruleId, {
                        status: event.target.value as RuleCheckStatus,
                        note: event.target.value === "偏离" ? check.note : "",
                      })
                    }
                  >
                    <option>待确认</option>
                    <option>遵守</option>
                    <option>偏离</option>
                    <option>不适用</option>
                  </select>
                </label>
                {check.status === "偏离" && (
                  <label className="rule-deviation-note">
                    <span>偏离原因（必填）</span>
                    <input
                      maxLength={1000}
                      value={check.note}
                      onChange={(event) =>
                        updateRuleCheck(check.ruleId, {
                          note: event.target.value,
                        })
                      }
                      placeholder="为什么仍决定偏离？需要什么证据纠正？"
                    />
                  </label>
                )}
              </article>
            ))}
          </div>
        </div>
        <div className="form-actions">
          <p>
            {!rulesReady
              ? "规则尚未读取成功，请等待或重新加载"
              : incompleteRuleChecks
                ? "请先完成所有有效规则的逐条确认"
                : "必填：投资对象、正反逻辑、证伪条件与复盘日期"}
          </p>
          <button
            className="primary"
            onClick={persist}
            disabled={
              saving ||
              !entry.assetName ||
              !entry.thesis ||
              !entry.counterThesis ||
              !entry.invalidation ||
              !entry.reviewDate ||
              incompleteRuleChecks
            }
          >
            <Save size={16} />
            冻结决策快照
          </button>
        </div>
      </section>

      <section className="panel decision-history">
        <div className="panel-title">
          <div>
            <h2>历史决策</h2>
          </div>
          <span className="history-count">{records.length} 条</span>
        </div>
        {error && (
          <div className="error-box" role="alert">
            <AlertTriangle size={18} />
            {error}
            <button
              className="secondary"
              disabled={loading}
              onClick={() => void refresh()}
            >
              重新加载决策与规则
            </button>
          </div>
        )}
        {loading && <p role="status">正在读取决策与规则…</p>}
        {!loading && !error && records.length === 0 && (
          <div className="empty">
            还没有决策记录。先冻结一张决策卡，未来才有可复盘的证据。
          </div>
        )}
        <div className="decision-list">
          {records.map((record) => {
            const due = Boolean(
              record.reviewDate &&
              record.reviewDate <= localDateValue(new Date()) &&
              !record.review,
            );
            return (
              <article
                key={record.id}
                className={reviewingId === record.id ? "reviewing" : ""}
              >
                <div className="decision-summary">
                  <div className="decision-name">
                    <span
                      className={
                        record.review ? "reviewed" : due ? "due" : "planned"
                      }
                    >
                      {record.review ? "已复盘" : due ? "待复盘" : "观察中"}
                    </span>
                    <strong>{record.assetName}</strong>
                    <small>
                      {new Date(record.createdAt).toLocaleDateString("zh-CN")}
                    </small>
                  </div>
                  <div>
                    <span>置信度</span>
                    <strong>{record.confidencePct}%</strong>
                  </div>
                  <div>
                    <span>计划仓位</span>
                    <strong>{record.positionPct}%</strong>
                  </div>
                  <div>
                    <span>复盘日</span>
                    <strong>{record.reviewDate || "未设定"}</strong>
                  </div>
                  <button
                    className="secondary"
                    onClick={() => beginReview(record)}
                  >
                    {record.review ? "更新复盘" : "开始复盘"}
                  </button>
                </div>
                <div className="decision-thesis">
                  <p>
                    <b>原始逻辑</b>
                    {record.thesis}
                  </p>
                  <p>
                    <b>证伪条件</b>
                    {record.invalidation}
                  </p>
                  {record.sourceAnalysisId && (
                    <button
                      className="decision-provenance"
                      onClick={() => onOpenAnalysis(record.sourceAnalysisId!)}
                    >
                      <BrainCircuit size={12} />
                      源自 AI 分析 · 行动 {(record.sourceActionIndex ?? 0) +
                        1}{" "}
                      · 打开原记录
                    </button>
                  )}
                </div>
                {record.ruleChecks?.length > 0 && (
                  <div className="decision-rule-snapshot">
                    {record.ruleChecks.map((check) => (
                      <div key={check.ruleId}>
                        <span
                          className={`rule-check-status status-${check.status}`}
                        >
                          {check.status}
                        </span>
                        <strong>{check.statement}</strong>
                        <small>
                          v{check.ruleRevision}
                          {check.note ? ` · ${check.note}` : ""}
                        </small>
                      </div>
                    ))}
                  </div>
                )}
                {record.review && reviewingId !== record.id && (
                  <div className="review-result">
                    <span>{record.review.thesisStatus}</span>
                    <p>{record.review.outcomeSummary}</p>
                    <strong>过程 {record.review.processRating}/5</strong>
                    {typeof record.review.actualReturnPct === "number" && (
                      <em
                        className={
                          record.review.actualReturnPct >= 0 ? "gain" : "loss"
                        }
                      >
                        {record.review.actualReturnPct >= 0 ? "+" : ""}
                        {record.review.actualReturnPct}%
                      </em>
                    )}
                  </div>
                )}
                {reviewingId === record.id && (
                  <div className="review-form">
                    <label>
                      <span>原始逻辑结果</span>
                      <select
                        value={review.thesisStatus}
                        onChange={(e) =>
                          setReview({
                            ...review,
                            thesisStatus: e.target
                              .value as DecisionReview["thesisStatus"],
                          })
                        }
                      >
                        <option>成立</option>
                        <option>部分成立</option>
                        <option>失效</option>
                        <option>尚不明确</option>
                      </select>
                    </label>
                    <label>
                      <span>实际收益（可选）</span>
                      <div className="input-affix">
                        <input
                          type="number"
                          value={review.actualReturnPct ?? ""}
                          onChange={(e) =>
                            setReview({
                              ...review,
                              actualReturnPct:
                                e.target.value === ""
                                  ? undefined
                                  : Number(e.target.value),
                            })
                          }
                        />
                        <i>%</i>
                      </div>
                    </label>
                    <label>
                      <span>决策过程评分</span>
                      <select
                        value={review.processRating}
                        onChange={(e) =>
                          setReview({
                            ...review,
                            processRating: Number(e.target.value),
                          })
                        }
                      >
                        {[1, 2, 3, 4, 5].map((value) => (
                          <option key={value} value={value}>
                            {value} / 5
                          </option>
                        ))}
                      </select>
                    </label>
                    <label className="span-3">
                      <span>实际发生了什么？</span>
                      <textarea
                        value={review.outcomeSummary}
                        onChange={(e) =>
                          setReview({
                            ...review,
                            outcomeSummary: e.target.value,
                          })
                        }
                        placeholder="只记录事实，区分价格结果与逻辑变化。"
                      />
                    </label>
                    <label className="span-3">
                      <span>如何修正未来决策？</span>
                      <textarea
                        value={review.lessons}
                        onChange={(e) =>
                          setReview({ ...review, lessons: e.target.value })
                        }
                        placeholder="保留、修改或删除哪条规则？"
                      />
                    </label>
                    <div className="span-3 review-actions">
                      <button
                        className="text-button"
                        onClick={() => setReviewingId(null)}
                      >
                        取消
                      </button>
                      <button
                        className="primary"
                        disabled={
                          saving || !review.outcomeSummary || !review.lessons
                        }
                        onClick={persistReview}
                      >
                        <Save size={15} />
                        保存复盘
                      </button>
                    </div>
                  </div>
                )}
              </article>
            );
          })}
        </div>
      </section>
    </div>
  );
}
