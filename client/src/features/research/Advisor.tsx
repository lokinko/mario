import { useEffect, useRef, useState } from "react";
import {
  AlertTriangle,
  Bot,
  BrainCircuit,
  Check,
  ChevronRight,
  Database,
  Eye,
  History,
  KeyRound,
  LoaderCircle,
  LockKeyhole,
  Send,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import {
  getAnalysis,
  getAnalysisHistory,
  previewAnalysis,
  runAnalysis,
} from "../../api";
import type {
  AnalysisPreview,
  AnalysisRequest,
  AnalysisResult,
  AnalysisHistoryItem,
  ContextSelection,
  DecisionEntry,
  ModelConfig,
  StoredAnalysis,
} from "../../types";
import { View } from "../../app/navigation";
import { Toggle } from "../../components/Toggle";
import {
  AdviceCards,
  WebSearchStatus,
  EvidenceOverview,
  StoredAnalysisView,
  StoredWorkflowTrace,
  StructuredReportView,
} from "./AnalysisReport";
import { MemoryItems } from "../memory/MemoryItems";
import { useRequestGuard } from "../../lib/useRequestGuard";

export function Advisor({
  model,
  active = true,
  navigate,
  requestedAnalysisId,
  clearRequestedAnalysis,
  onCreateDecisionDraft,
}: {
  model: ModelConfig;
  active?: boolean;
  navigate: (v: View) => void;
  requestedAnalysisId: string | null;
  clearRequestedAnalysis: () => void;
  onCreateDecisionDraft: (draft: DecisionEntry) => void;
}) {
  const historyRequest = useRequestGuard();
  const analysisRequest = useRequestGuard();
  const [question, setQuestion] = useState("");
  const latestAnswerRef = useRef<HTMLElement>(null);
  const previewRef = useRef<HTMLElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const [turns, setTurns] = useState<
    { question: string; result: AnalysisResult }[]
  >([]);
  const [followUp, setFollowUp] = useState(false);
  const [deep, setDeep] = useState(true);
  const [memory, setMemory] = useState(true);
  const [reflection, setReflection] = useState(true);
  const [alternatives, setAlternatives] = useState(true);
  const [excludedMemoryIds, setExcludedMemoryIds] = useState<string[]>([]);
  const [contextSelection, setContextSelection] = useState<ContextSelection>({
    includeProfile: true,
    includeGoals: true,
    includeHoldings: true,
    includePlanning: true,
    includeRiskFindings: true,
    includeRules: true,
    includeSystemReviews: true,
    includePortfolioCheckins: true,
    includePortfolioEvents: true,
    includeEvidence: true,
  });
  const [preview, setPreview] = useState<AnalysisPreview | null>(null);
  const [previewStale, setPreviewStale] = useState(false);
  const [previewing, setPreviewing] = useState(false);
  const [running, setRunning] = useState(false);

  const [webSearch, setWebSearch] = useState(true);
  const [error, setError] = useState("");
  const [history, setHistory] = useState<AnalysisHistoryItem[]>([]);
  const [storedAnalysis, setStoredAnalysis] = useState<StoredAnalysis | null>(
    null,
  );
  const [historyBusy, setHistoryBusy] = useState("");
  const [historyError, setHistoryError] = useState("");

  useEffect(() => {
    if (!active) return;
    const queued =
      window.sessionStorage.getItem("mario.advisorQuestion") ??
      window.sessionStorage.getItem("compass.advisorQuestion");
    if (queued) {
      invalidatePreview(() => setQuestion(queued));
      setFollowUp(false);
      window.sessionStorage.removeItem("mario.advisorQuestion");
      window.sessionStorage.removeItem("compass.advisorQuestion");
    }
  }, [active]);

  useEffect(() => {
    let active = true;
    getAnalysisHistory()
      .then((items) => {
        if (active) setHistory(items);
      })
      .catch((nextError) => {
        if (active) setHistoryError(String(nextError));
      });
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    if (!requestedAnalysisId) return;
    void openStoredAnalysis(requestedAnalysisId).then((accepted) => {
      if (accepted) clearRequestedAnalysis();
    });
  }, [requestedAnalysisId]);

  useEffect(() => {
    if (preview && active)
      previewRef.current?.scrollIntoView?.({
        behavior: "smooth",
        block: "nearest",
      });
  }, [preview, active]);

  useEffect(() => {
    if (active && turns.length)
      latestAnswerRef.current?.scrollIntoView?.({
        behavior: "smooth",
        block: "start",
      });
  }, [turns.length]);

  const request = (previewRevision?: string): AnalysisRequest => ({
    webSearch,
    userMessage: question,
    question:
      followUp && turns.length
        ? `以下是上一轮问答，仅用于理解追问，模型回答不是已核实事实。\n上次问题：${turns[turns.length - 1].question}\n上次回答摘要：${turns[turns.length - 1].result.answer.slice(0, 6000)}\n\n本次问题：${question}`
        : question,
    workflow: deep ? "deep" : "quick",
    useMemory: memory,
    reflect: reflection,
    exploreAlternatives: alternatives,
    excludedMemoryIds,
    contextSelection,
    previewRevision,
  });

  const invalidatePreview = (action: () => void) => {
    if (running) return;
    analysisRequest.invalidate();
    setPreviewing(false);
    setRunning(false);
    historyRequest.invalidate();
    setHistoryBusy("");
    action();
    setExcludedMemoryIds([]);
    setPreview(null);
    setPreviewStale(false);
    setStoredAnalysis(null);
  };

  const prepare = async () => {
    if (!question.trim() || running || previewing) return;
    const isCurrent = analysisRequest.begin();
    setPreviewing(true);
    setError("");
    try {
      const nextPreview = await previewAnalysis(request());
      if (!isCurrent()) return;
      setPreview(nextPreview);
      setPreviewStale(false);
    } catch (e) {
      if (isCurrent()) setError(String(e));
    } finally {
      if (isCurrent()) setPreviewing(false);
    }
  };

  const analyze = async () => {
    const isCurrent = analysisRequest.begin();
    setRunning(true);
    setError("");
    try {
      if (!preview) throw new Error("请先预览将发送的数据");
      if (previewStale) throw new Error("记忆选择已变化，请重新预览后再确认");
      const nextResult = await runAnalysis(request(preview.contextRevision));
      if (!isCurrent()) return;
      setTurns((current) => [...current, { question, result: nextResult }]);
      setQuestion("");
      setFollowUp(true);
      setStoredAnalysis(null);
      setPreview(null);
      getAnalysisHistory()
        .then((items) => {
          if (isCurrent()) setHistory(items);
        })
        .catch(() => undefined);
    } catch (e) {
      if (isCurrent()) setError(String(e));
    } finally {
      if (isCurrent()) setRunning(false);
    }
  };

  const toggleContext = (key: keyof ContextSelection) =>
    invalidatePreview(() =>
      setContextSelection((current) => ({ ...current, [key]: !current[key] })),
    );
  const toggleMemoryCandidate = (id: string) => {
    if (running) return;
    analysisRequest.invalidate();
    setPreviewing(false);
    setRunning(false);
    setExcludedMemoryIds((current) =>
      current.includes(id)
        ? current.filter((item) => item !== id)
        : [...current, id],
    );
    setPreviewStale(true);
  };

  const openStoredAnalysis = async (id: string) => {
    analysisRequest.invalidate();
    setPreviewing(false);
    setRunning(false);
    const isCurrent = historyRequest.begin();
    setHistoryBusy(id);
    setHistoryError("");
    setPreview(null);
    setStoredAnalysis(null);
    try {
      const item = await getAnalysis(id);
      if (!isCurrent()) return false;
      setStoredAnalysis(item);
      window.setTimeout(() => {
        if (isCurrent())
          document
            .getElementById("stored-analysis")
            ?.scrollIntoView({ behavior: "smooth", block: "start" });
      }, 50);
    } catch (nextError) {
      if (isCurrent()) setHistoryError(String(nextError));
    } finally {
      if (isCurrent()) setHistoryBusy("");
    }
    return isCurrent();
  };

  const reuseStoredQuestion = (item: StoredAnalysis) => {
    analysisRequest.invalidate();
    setPreviewing(false);
    setRunning(false);
    historyRequest.invalidate();
    setHistoryBusy("");
    setQuestion(item.question);
    setFollowUp(false);
    setStoredAnalysis(null);
    setPreview(null);
    setPreviewStale(false);
    window.scrollTo({ top: 0, behavior: "smooth" });
  };

  return (
    <div className="page narrow conversation-page">
      <header className="conversation-heading">
        <span className="conversation-eyebrow">和 mario 聊聊</span>
        <h1>
          {turns.length
            ? "把问题聊清楚，再做决定。"
            : "最近有什么投资上的困惑？"}
        </h1>
        <p>说说你的想法，或从一个小问题开始。</p>
      </header>
      {!model.hasApiKey && (
        <div className="setup-banner">
          <KeyRound size={20} />
          <div>
            <strong>连接模型，开始问答</strong>
            <p>使用你自己的模型密钥，已有资料会帮助它理解你的情况。</p>
          </div>
          <button className="secondary" onClick={() => navigate("settings")}>
            连接模型
          </button>
        </div>
      )}
      <div className="conversation-turns" aria-label="本次问答">
        {turns.map((turn) => (
          <article
            className="conversation-turn"
            key={turn.result.id}
            ref={turn === turns[turns.length - 1] ? latestAnswerRef : undefined}
          >
            <div className="user-question">
              <span>你</span>
              <p>{turn.question}</p>
            </div>
            <div className="assistant-answer">
              <span className="answer-author">
                <Sparkles size={16} /> mario
              </span>
              <WebSearchStatus trace={turn.result.workflowTrace} />
              {turn.result.workflowTrace.adviceGrounding?.some(
                (review) => review.status !== "supported",
              ) && (
                <p className="advice-gap" role="status">
                  这次核对发现证据缺口；请先查看各条建议的核对结果。
                </p>
              )}
              <p className="answer">
                {turn.result.workflowTrace.structuredReport?.verdict ??
                  turn.result.answer}
              </p>
              {turn.result.workflowTrace.structuredReport?.unknowns.length ? (
                <div className="conversation-clarification">
                  <strong>还需要了解</strong>
                  <p>
                    {turn.result.workflowTrace.structuredReport.unknowns[0]}
                  </p>
                  <button
                    className="text-button"
                    disabled={running}
                    onClick={() => {
                      invalidatePreview(() =>
                        setQuestion(
                          `关于“${turn.result.workflowTrace.structuredReport!.unknowns[0]}”，我的情况是：`,
                        ),
                      );
                      setFollowUp(turn === turns[turns.length - 1]);
                      inputRef.current?.focus();
                    }}
                  >
                    补充我的情况
                  </button>
                </div>
              ) : null}
              {turn.result.workflowTrace.structuredReport && (
                <EvidenceOverview
                  report={turn.result.workflowTrace.structuredReport}
                  evidence={turn.result.workflowTrace.evidenceCatalog ?? []}
                />
              )}
              {turn.result.workflowTrace.structuredReport && (
                <AdviceCards
                  onAsk={(prompt) => {
                    if (running) return;
                    invalidatePreview(() => setQuestion(prompt));
                    setFollowUp(turn === turns[turns.length - 1]);
                    inputRef.current?.focus();
                  }}
                  report={turn.result.workflowTrace.structuredReport}
                  personalContext={turn.result.workflowTrace.personalContext}
                  grounding={turn.result.workflowTrace.adviceGrounding}
                  evidence={turn.result.workflowTrace.evidenceCatalog ?? []}
                  analysisId={turn.result.id}
                  onCreateDecisionDraft={onCreateDecisionDraft}
                />
              )}
              {turn.result.workflowTrace.structuredReport && (
                <details className="answer-details">
                  <summary>查看完整分析与方案比较</summary>
                  <StructuredReportView
                    report={turn.result.workflowTrace.structuredReport}
                    personalContext={turn.result.workflowTrace.personalContext}
                    grounding={turn.result.workflowTrace.adviceGrounding}
                    evidence={turn.result.workflowTrace.evidenceCatalog ?? []}
                    analysisId={turn.result.id}
                    onCreateDecisionDraft={onCreateDecisionDraft}
                  />
                </details>
              )}
              <details className="answer-details">
                <summary>分析过程与使用的资料</summary>
                <p>
                  {turn.result.transparency.model} ·{" "}
                  {turn.result.transparency.modelCalls} 次调用 ·{" "}
                  {(turn.result.transparency.totalLatencyMs / 1000).toFixed(1)}{" "}
                  秒
                </p>
                <StoredWorkflowTrace trace={turn.result.workflowTrace} />
              </details>
              <p className="disclaimer">{turn.result.disclaimer}</p>
            </div>
          </article>
        ))}
      </div>
      {storedAnalysis && (
        <StoredAnalysisView
          item={storedAnalysis}
          onClose={() => setStoredAnalysis(null)}
          onReuse={() => reuseStoredQuestion(storedAnalysis)}
          onCreateDecisionDraft={onCreateDecisionDraft}
        />
      )}
      <section className="panel advisor-panel conversation-composer">
        <label className="question-box">
          <span>{turns.length ? "继续聊聊" : "这次希望解决什么问题？"}</span>
          <textarea
            disabled={running}
            ref={inputRef}
            value={question}
            placeholder={
              turns.length
                ? "补充一点情况，或继续追问…"
                : "例如：我想开始投资，但不知道该先考虑什么…"
            }
            onChange={(e) =>
              invalidatePreview(() => setQuestion(e.target.value))
            }
            onKeyDown={(e) => {
              if (
                e.key === "Enter" &&
                (e.metaKey || e.ctrlKey) &&
                !e.nativeEvent.isComposing
              ) {
                e.preventDefault();
                void prepare();
              }
            }}
          />
        </label>
        {!turns.length && (
          <div className="research-tasks" aria-label="从一个问题开始">
            {[
              "我现在适合开始投资吗？",
              "我的持仓有哪些需要注意的风险？",
              "帮我理清最近的一次投资决定",
            ].map((task) => (
              <button
                key={task}
                className="secondary"
                onClick={() => {
                  invalidatePreview(() => setQuestion(task));
                  inputRef.current?.focus();
                }}
              >
                {task}
              </button>
            ))}
          </div>
        )}
        <div className="composer-footer">
          <span>
            {turns.length && followUp
              ? "会参考上一轮问答摘要"
              : "发送前可确认使用的资料"}
          </span>
          {turns.length > 0 && (
            <button
              className="text-button"
              disabled={running}
              onClick={() => {
                invalidatePreview(() => setQuestion(""));
                setFollowUp(!followUp);
                inputRef.current?.focus();
              }}
            >
              {followUp ? "换个话题" : "接着上轮聊"}
            </button>
          )}
          <button
            className="primary"
            onClick={prepare}
            disabled={previewing || running || !question.trim()}
          >
            {previewing ? (
              <LoaderCircle size={17} className="spin" />
            ) : (
              <Send size={17} />
            )}
            {previewing ? "准备中…" : "提问"}
          </button>
        </div>
      </section>
      {error && (
        <div className="error-box" role="alert">
          <AlertTriangle size={18} />
          {error}
        </div>
      )}
      {historyError && (
        <div className="error-box" role="alert">
          <AlertTriangle size={18} />
          {historyError}
        </div>
      )}
      {preview && (
        <section
          ref={previewRef}
          className="panel preview-panel"
          aria-label="发送确认"
        >
          <div className="panel-title">
            <div>
              <h2>确认这次使用的资料</h2>
            </div>
            <div className="preview-size">
              {(preview.payloadBytes / 1024).toFixed(1)} KB
            </div>
          </div>
          <div className="preview-provider">
            <Bot size={16} />
            <span>
              <strong>{preview.model}</strong>
              {preview.provider} ·{" "}
              {preview.workflow === "deep" ? "深度工作流" : "快速工作流"}
            </span>
          </div>
          {previewStale && (
            <div className="preview-stale">
              <AlertTriangle size={15} />
              <div>
                <strong>记忆授权已变化</strong>
                <span>下方显示的是新选择，但需要重新确认。</span>
              </div>
            </div>
          )}
          <p className="confirmation-summary">
            将把你的问题和{" "}
            {preview.groups.filter((group) => group.included).length}{" "}
            组资料发送给上述模型。
            {webSearch
              ? "确认后将启用供应商网页搜索，搜索查询可能交给其搜索服务，且可能产生额外费用。"
              : "本次不启用网页搜索，仅使用已选资料。"}
            {followUp ? "本次也会附上上一轮问题与回答摘要。" : ""}
          </p>
          <details className="payload-details">
            <summary>查看本次发送的资料</summary>
            <div className="context-group-list">
              {preview.groups.map((group) => (
                <div
                  className={group.included ? "included" : "omitted"}
                  key={group.key}
                >
                  <i>{group.included ? <Check size={12} /> : "—"}</i>
                  <div>
                    <strong>{group.label}</strong>
                    <small>{group.description}</small>
                  </div>
                  <span>
                    {group.included
                      ? `${group.recordCount} 项 · ${group.sensitivity}`
                      : "留在本机"}
                  </span>
                </div>
              ))}
            </div>
            <div className="local-only-note">
              <LockKeyhole size={16} />
              <div>
                <strong>始终留在本机</strong>
                <p>{preview.localOnly.join("；")}</p>
              </div>
            </div>
            <details className="payload-details">
              <summary>查看实际本地数据载荷</summary>
              <pre>{JSON.stringify(preview.payload, null, 2)}</pre>
            </details>
            {preview.evidenceCandidates.length > 0 && (
              <details className="payload-details">
                <summary>
                  查看本次可引用证据（{preview.evidenceCandidates.length} 条）
                </summary>
                <pre>{JSON.stringify(preview.evidenceCandidates, null, 2)}</pre>
              </details>
            )}
            {preview.memoryCandidates.length > 0 && (
              <details className="payload-details memory-disclosure">
                <summary>
                  逐条选择候选记忆（授权{" "}
                  {
                    preview.memoryCandidates.filter(
                      (item) => !excludedMemoryIds.includes(item.id),
                    ).length
                  }
                  /{preview.memoryCandidates.length} 条）
                </summary>
                <MemoryItems
                  items={preview.memoryCandidates}
                  excludedIds={excludedMemoryIds}
                  onToggle={toggleMemoryCandidate}
                />
              </details>
            )}
            <details className="payload-details">
              <summary>查看固定投资方法论提示</summary>
              <pre>{preview.systemPolicy}</pre>
            </details>
            <p className="memory-policy">{preview.memoryPolicy}</p>
          </details>
          <div className="preview-actions">
            <button
              className="text-button"
              disabled={running}
              onClick={() => setPreview(null)}
            >
              返回修改
            </button>
            <button
              className="primary"
              onClick={previewStale ? prepare : analyze}
              disabled={
                running || previewing || (!previewStale && !model.hasApiKey)
              }
            >
              {previewing ? (
                <>
                  <LoaderCircle size={15} className="spin" />
                  正在更新预览…
                </>
              ) : previewStale ? (
                <>
                  <Eye size={15} />
                  按新选择重新预览
                </>
              ) : running ? (
                <>
                  <LoaderCircle size={15} className="spin" />
                  正在分析…
                </>
              ) : (
                <>
                  <Send size={15} />
                  确认发送
                </>
              )}
            </button>
          </div>
        </section>
      )}

      <details className="conversation-context">
        <summary>供模型参考的资料与偏好</summary>
        <p>
          你已有的资料用于理解你的情况；需要时再补充。每次发送前都可以检查。
        </p>
        <div className="external-background">
          <label className="auto-external-option">
            <input
              type="checkbox"
              checked={webSearch}
              disabled={running}
              onChange={(event) =>
                invalidatePreview(() => setWebSearch(event.target.checked))
              }
            />
            联网查找相关资料
          </label>
          <p>
            确认发送后由模型供应商搜索网页，回答会附上来源链接。关闭后仅参考已有资料。
          </p>
        </div>
        <div className="context-shortcuts">
          <button className="secondary" onClick={() => navigate("foundation")}>
            我的持仓与目标
          </button>
          <button className="secondary" onClick={() => navigate("memory")}>
            记忆与规则
          </button>
          <button className="secondary" onClick={() => navigate("evidence")}>
            参考资料
          </button>
        </div>
        <details className="workflow-advanced">
          <summary>高级分析选项</summary>
          <div className="workflow-options">
            <Toggle
              icon={<BrainCircuit size={17} />}
              title="深度编排"
              detail="构建计划并分阶段分析"
              checked={deep}
              onChange={(value) => invalidatePreview(() => setDeep(value))}
            />
            <Toggle
              icon={<Database size={17} />}
              title="本地记忆"
              detail="检索相关历史决策"
              checked={memory}
              onChange={(value) => invalidatePreview(() => setMemory(value))}
            />
            <Toggle
              icon={<ShieldCheck size={17} />}
              title="纠错反思"
              detail="独立检查遗漏和过度自信"
              checked={reflection}
              onChange={(value) =>
                invalidatePreview(() => setReflection(value))
              }
            />
            <Toggle
              icon={<Sparkles size={17} />}
              title="多方案探索"
              detail="比较至少两条可行路径"
              checked={alternatives}
              onChange={(value) =>
                invalidatePreview(() => setAlternatives(value))
              }
            />
          </div>
        </details>
        <details className="context-control">
          <summary>模型可以参考哪些资料</summary>
          <div>
            <strong>选择允许发送的本地上下文</strong>
            <span>取消选择后，该组不会进入模型提示词</span>
          </div>
          <div className="context-options">
            {(
              [
                ["includeProfile", "财务档案"],
                ["includeGoals", "目标计划"],
                ["includeHoldings", "持仓明细"],
                ["includePlanning", "规划结果"],
                ["includeRiskFindings", "风险检查"],
                ["includeRules", "个人规则"],
                ["includeSystemReviews", "周期复盘"],
                ["includePortfolioCheckins", "组合变化"],
                ["includePortfolioEvents", "组合流水"],
                ["includeEvidence", "研究证据"],
              ] as [keyof ContextSelection, string][]
            ).map(([key, label]) => (
              <button
                key={key}
                aria-pressed={contextSelection[key]}
                className={contextSelection[key] ? "selected" : ""}
                onClick={() => toggleContext(key)}
              >
                <i>{contextSelection[key] && <Check size={11} />}</i>
                {label}
              </button>
            ))}
          </div>
        </details>
      </details>
      <details className="panel analysis-history-panel">
        <summary>以前的问答 · {history.length}</summary>
        <div className="panel-title">
          <div>
            <h2>分析历史</h2>
          </div>
          <span className="history-count">最近 {history.length} 条</span>
        </div>
        <p className="analysis-history-boundary">
          历史 AI
          分析是未经结果验证的研究产物。它可以被重开、追溯或转成待确认草稿，但不会自动成为事实、规则或交易指令。
        </p>
        {history.length === 0 && !historyError && (
          <div className="empty">
            还没有保存过 AI 分析。成功完成一次分析后，完整工作流会留在本机。
          </div>
        )}
        <div className="analysis-history-list">
          {history.map((item) => (
            <button
              key={item.id}
              className={storedAnalysis?.id === item.id ? "active" : ""}
              onClick={() => void openStoredAnalysis(item.id)}
              disabled={running || Boolean(historyBusy)}
            >
              <div>
                <History size={15} />
                <span>{item.workflowVersion || "旧版分析"}</span>
              </div>
              <strong>{item.question}</strong>
              <small>
                {item.groundingStatus === "supported"
                  ? "资料范围内支持 · 模型核对"
                  : item.groundingStatus === "contradicted"
                    ? "发现证据矛盾"
                    : item.groundingStatus === "insufficient"
                      ? "证据不足"
                      : "尚未完成证据核对"}
              </small>
              <p>
                {item.verdict || "旧记录没有结构化裁决，可打开查看原回答。"}
              </p>
              <small>
                {new Date(item.createdAt).toLocaleString("zh-CN")} ·{" "}
                {item.transparency?.model || "无模型审计"}
              </small>
              <em>
                {historyBusy === item.id ? (
                  <LoaderCircle size={14} className="spin" />
                ) : (
                  <ChevronRight size={14} />
                )}
              </em>
            </button>
          ))}
        </div>
      </details>
    </div>
  );
}
