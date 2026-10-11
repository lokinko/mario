import { useEffect, useState } from "react";
import { AlertTriangle, BrainCircuit, FilePenLine, Save } from "lucide-react";
import { getDecisions, saveDecision, saveDecisionReview } from "../../api";
import type { DecisionEntry, DecisionRecord } from "../../types";
import { PageHeader } from "../../components/PageHeader";
import { decisionReviewText } from "../../lib/reviewText";
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

function decisionText(entry: DecisionEntry): string {
  if (entry.content != null) return entry.content;
  return [
    entry.assetName && `投资对象：${entry.assetName}`,
    entry.thesis,
    entry.counterThesis && `顾虑与反方观点：${entry.counterThesis}`,
    entry.invalidation && `需要重新考虑的情况：${entry.invalidation}`,
    entry.expectedReturnPct && `上行情景收益：${entry.expectedReturnPct}%`,
    entry.downsidePct && `下行情景损失：${entry.downsidePct}%`,
    entry.assetName && `判断置信度：${entry.confidencePct}%`,
    entry.positionPct && `计划仓位：${entry.positionPct}%`,
    entry.reviewDate && `计划复盘日：${entry.reviewDate}`,
  ]
    .filter(Boolean)
    .join("\n\n");
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
  const [content, setContent] = useState(() =>
    seed ? decisionText(seed) : "",
  );
  const [records, setRecords] = useState<DecisionRecord[]>([]);
  const [reviewingId, setReviewingId] = useState<string | null>(null);
  const [reviewContent, setReviewContent] = useState("");
  const [saving, setSaving] = useState(false);
  const [loading, setLoading] = useState(true);
  const loadRequest = useRequestGuard();
  const [error, setError] = useState("");
  const refresh = async () => {
    const isCurrent = loadRequest.begin();
    setLoading(true);
    try {
      const nextRecords = await getDecisions();
      if (!isCurrent()) return;
      setRecords(nextRecords);
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
  useEffect(() => {
    if (seed) setContent(decisionText(seed));
  }, [seed]);

  const reviewed = records.filter((item) => item.review);

  const persist = async () => {
    if (saving || !content.trim()) return;
    setError("");
    setSaving(true);
    try {
      await saveDecision({
        content,
        ...(seed?.sourceAnalysisId
          ? { sourceAnalysisId: seed.sourceAnalysisId }
          : {}),
        ...(seed?.sourceActionIndex != null
          ? { sourceActionIndex: seed.sourceActionIndex }
          : {}),
      });
      flash("决策记录已保存");
      setContent("");
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
    setReviewContent(record.review ? decisionReviewText(record.review) : "");
  };

  const persistReview = async () => {
    if (saving || !reviewingId || !reviewContent.trim()) return;
    setError("");
    setSaving(true);
    try {
      await saveDecisionReview(reviewingId, { content: reviewContent });
      await refresh();
      setReviewingId(null);
      flash("复盘已保存");
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="page narrow">
      <PageHeader title="决策确认" />
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
      </section>
      <section className="panel form-panel decision-card">
        <div className="panel-title">
          <div>
            <h2>新建决策</h2>
          </div>
        </div>
        {seed?.sourceAnalysisId && (
          <div className="decision-draft-notice">
            <BrainCircuit size={18} />
            <div>
              <strong>来自 AI 分析的待确认草稿</strong>
              <p>可以在下方补充或改写你的观点、顾虑和思考，确认后保存。</p>
            </div>
            <button
              className="text-button"
              onClick={() => {
                setContent("");
                clearSeed();
              }}
            >
              放弃草稿
            </button>
          </div>
        )}
        <form
          onSubmit={(event) => {
            event.preventDefault();
            void persist();
          }}
        >
          <label className="decision-content-field">
            <span>我的观点、顾虑和思考</span>
            <textarea
              value={content}
              rows={10}
              required
              disabled={saving}
              onChange={(event) => setContent(event.target.value)}
              placeholder="自由写下你想表达的内容：你的观点、为什么这样想、有哪些顾虑，以及还需要考虑什么。"
            />
          </label>
          <div className="form-actions">
            <button
              className="primary"
              type="submit"
              disabled={saving || !content.trim()}
            >
              <Save size={16} />
              {saving ? "保存中…" : "保存决策"}
            </button>
          </div>
        </form>
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
              重新加载决策记录
            </button>
          </div>
        )}
        {loading && <p role="status">正在读取决策记录…</p>}
        {!loading && !error && records.length === 0 && (
          <div className="empty">
            还没有决策记录，写下你的第一段观点、顾虑和思考。
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
                  <button
                    className="secondary"
                    disabled={saving}
                    onClick={() => beginReview(record)}
                  >
                    {record.review ? "更新复盘" : "开始复盘"}
                  </button>
                </div>
                <div className="decision-thesis">
                  <p className="decision-content">{decisionText(record)}</p>
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
                    <p className="review-content">
                      {decisionReviewText(record.review)}
                    </p>
                  </div>
                )}
                {reviewingId === record.id && (
                  <form
                    className="review-form"
                    onSubmit={(event) => {
                      event.preventDefault();
                      void persistReview();
                    }}
                  >
                    <label className="span-3">
                      <span>复盘内容</span>
                      <textarea
                        value={reviewContent}
                        rows={8}
                        required
                        disabled={saving}
                        onChange={(event) =>
                          setReviewContent(event.target.value)
                        }
                        placeholder="自由写下这次复盘中你想表达的内容、观点、顾虑和思考。"
                      />
                    </label>
                    <div className="span-3 review-actions">
                      <button
                        className="text-button"
                        type="button"
                        disabled={saving}
                        onClick={() => setReviewingId(null)}
                      >
                        取消
                      </button>
                      <button
                        className="primary"
                        type="submit"
                        disabled={saving || !reviewContent.trim()}
                      >
                        <Save size={15} />
                        保存复盘
                      </button>
                    </div>
                  </form>
                )}
              </article>
            );
          })}
        </div>
      </section>
    </div>
  );
}
