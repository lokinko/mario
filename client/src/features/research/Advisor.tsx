import { useEffect, useState } from "react";
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
import { PageHeader } from "../../components/PageHeader";
import { View } from "../../app/navigation";
import { Toggle } from "../../components/Toggle";
import { StoredAnalysisView, StructuredReportView } from "./AnalysisReport";
import { MemoryItems } from "../memory/MemoryItems";
import { useRequestGuard } from "../../lib/useRequestGuard";

export function Advisor({
  model,
  navigate,
  requestedAnalysisId,
  clearRequestedAnalysis,
  onCreateDecisionDraft,
}: {
  model: ModelConfig;
  navigate: (v: View) => void;
  requestedAnalysisId: string | null;
  clearRequestedAnalysis: () => void;
  onCreateDecisionDraft: (draft: DecisionEntry) => void;
}) {
  const historyRequest = useRequestGuard();
  const [question, setQuestion] = useState(
    "请基于我的财务目标和当前组合，指出最需要优先处理的风险，并给出不依赖市场预测的改进方案。",
  );
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
  const [result, setResult] = useState<AnalysisResult | null>(null);
  const [error, setError] = useState("");
  const [history, setHistory] = useState<AnalysisHistoryItem[]>([]);
  const [storedAnalysis, setStoredAnalysis] = useState<StoredAnalysis | null>(
    null,
  );
  const [historyBusy, setHistoryBusy] = useState("");
  const [historyError, setHistoryError] = useState("");

  useEffect(() => {
    const queued =
      window.sessionStorage.getItem("mario.advisorQuestion") ??
      window.sessionStorage.getItem("compass.advisorQuestion");
    if (queued) {
      setQuestion(queued);
      window.sessionStorage.removeItem("mario.advisorQuestion");
      window.sessionStorage.removeItem("compass.advisorQuestion");
    }
  }, []);

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

  const request = (previewRevision?: string): AnalysisRequest => ({
    question,
    workflow: deep ? "deep" : "quick",
    useMemory: memory,
    reflect: reflection,
    exploreAlternatives: alternatives,
    excludedMemoryIds,
    contextSelection,
    previewRevision,
  });

  const invalidatePreview = (action: () => void) => {
    historyRequest.invalidate();
    setHistoryBusy("");
    action();
    setExcludedMemoryIds([]);
    setPreview(null);
    setPreviewStale(false);
    setResult(null);
    setStoredAnalysis(null);
  };

  const prepare = async () => {
    setPreviewing(true);
    setError("");
    setResult(null);
    try {
      setPreview(await previewAnalysis(request()));
      setPreviewStale(false);
    } catch (e) {
      setError(String(e));
    } finally {
      setPreviewing(false);
    }
  };

  const analyze = async () => {
    setRunning(true);
    setError("");
    setResult(null);
    try {
      if (!preview) throw new Error("请先预览将发送的数据");
      if (previewStale) throw new Error("记忆选择已变化，请重新预览后再确认");
      const nextResult = await runAnalysis(request(preview.contextRevision));
      setResult(nextResult);
      setStoredAnalysis(null);
      setPreview(null);
      getAnalysisHistory()
        .then(setHistory)
        .catch(() => undefined);
    } catch (e) {
      setError(String(e));
    } finally {
      setRunning(false);
    }
  };

  const toggleContext = (key: keyof ContextSelection) =>
    invalidatePreview(() =>
      setContextSelection((current) => ({ ...current, [key]: !current[key] })),
    );
  const toggleMemoryCandidate = (id: string) => {
    setExcludedMemoryIds((current) =>
      current.includes(id)
        ? current.filter((item) => item !== id)
        : [...current, id],
    );
    setPreviewStale(true);
    setResult(null);
  };

  const openStoredAnalysis = async (id: string) => {
    const isCurrent = historyRequest.begin();
    setHistoryBusy(id);
    setHistoryError("");
    setResult(null);
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
    historyRequest.invalidate();
    setHistoryBusy("");
    setQuestion(item.question);
    setStoredAnalysis(null);
    setResult(null);
    setPreview(null);
    setPreviewStale(false);
    window.scrollTo({ top: 0, behavior: "smooth" });
  };

  return (
    <div className="page narrow">
      <PageHeader
        eyebrow="AI 原生分析"
        title="研究室，而不是荐股机"
        description="规则引擎先处理确定性风险，大模型负责理解、比较、反驳与解释。"
        action={
          <div className={`model-pill ${model.hasApiKey ? "ready" : ""}`}>
            <Bot size={15} />
            {model.hasApiKey ? model.model : "尚未配置模型"}
          </div>
        }
      />
      {!model.hasApiKey && (
        <div className="setup-banner">
          <KeyRound size={20} />
          <div>
            <strong>配置自己的模型密钥</strong>
            <p>密钥保存到系统钥匙串，不写入投资数据库。</p>
          </div>
          <button className="secondary" onClick={() => navigate("settings")}>
            立即配置
          </button>
        </div>
      )}
      <section className="panel advisor-panel">
        <label className="question-box">
          <span>这次希望解决什么问题？</span>
          <textarea
            value={question}
            onChange={(e) =>
              invalidatePreview(() => setQuestion(e.target.value))
            }
          />
        </label>
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
            onChange={(value) => invalidatePreview(() => setReflection(value))}
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
        <div className="context-control">
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
                className={contextSelection[key] ? "selected" : ""}
                onClick={() => toggleContext(key)}
              >
                <i>{contextSelection[key] && <Check size={11} />}</i>
                {label}
              </button>
            ))}
          </div>
        </div>
        <button
          className="primary analyze-button"
          onClick={prepare}
          disabled={previewing || running || !question.trim()}
        >
          {previewing ? (
            <>
              <LoaderCircle size={17} className="spin" />
              正在生成本地预览…
            </>
          ) : (
            <>
              <Eye size={17} />
              预览将发送的数据
            </>
          )}
        </button>
      </section>

      {error && (
        <div className="error-box">
          <AlertTriangle size={18} />
          {error}
        </div>
      )}
      {historyError && (
        <div className="error-box">
          <AlertTriangle size={18} />
          {historyError}
        </div>
      )}
      {storedAnalysis && (
        <StoredAnalysisView
          item={storedAnalysis}
          onClose={() => setStoredAnalysis(null)}
          onReuse={() => reuseStoredQuestion(storedAnalysis)}
          onCreateDecisionDraft={onCreateDecisionDraft}
        />
      )}
      {preview && (
        <section className="panel preview-panel">
          <div className="panel-title">
            <div>
              <span>发送前确认</span>
              <h2>模型将看到这些内容</h2>
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
                <span>
                  下方显示的是新选择，但旧指纹已经失效。重新预览后才能开始分析。
                </span>
              </div>
            </div>
          )}
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
          <div className="preview-actions">
            <button className="text-button" onClick={() => setPreview(null)}>
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
                  确认并开始分析
                </>
              )}
            </button>
          </div>
        </section>
      )}
      {result && (
        <section className="panel result-panel">
          <div className="result-meta">
            {result.stages.map((stage) => (
              <span key={stage}>
                <Check size={13} />
                {stage}
              </span>
            ))}
          </div>
          <div className="analysis-audit">
            <div>
              <strong>{result.transparency.model}</strong>
              <span>{result.transparency.provider}</span>
            </div>
            <div>
              <strong>{result.transparency.modelCalls} 次</strong>
              <span>模型调用</span>
            </div>
            <div>
              <strong>
                {(result.transparency.totalLatencyMs / 1000).toFixed(1)} 秒
              </strong>
              <span>模型总耗时</span>
            </div>
            <div>
              <strong>
                {result.transparency.inputTokens == null
                  ? "未返回"
                  : result.transparency.inputTokens.toLocaleString()}
              </strong>
              <span>输入 tokens</span>
            </div>
            <div>
              <strong>{result.transparency.contextGroups.length} 组</strong>
              <span>上下文</span>
            </div>
            <div>
              <strong>{result.transparency.memoryItemsUsed} 条</strong>
              <span>采用记忆</span>
            </div>
            <div>
              <strong>
                {result.transparency.reviewedMemoryItemsUsed} /{" "}
                {result.transparency.conflictingMemoryItemsUsed}
              </strong>
              <span>已复盘 / 反证</span>
            </div>
            <div>
              <strong>{result.transparency.evidenceItemsUsed} 条</strong>
              <span>带来源证据</span>
            </div>
            <div>
              <strong>
                {result.transparency.citationsRequired
                  ? "外部事实须引用"
                  : "无可引用证据"}
              </strong>
              <span>引用约束</span>
            </div>
            <div>
              <strong>
                {result.transparency.structuredOutputValidated
                  ? result.transparency.outputRepairs > 0
                    ? `修复 ${result.transparency.outputRepairs} 次`
                    : "直接通过"
                  : "未校验"}
              </strong>
              <span>输出契约</span>
            </div>
            <div>
              <strong>
                {result.transparency.apiKeySent ? "异常" : "未进入提示词"}
              </strong>
              <span>API Key</span>
            </div>
          </div>
          {(result.workflowTrace.researchPlan ||
            result.workflowTrace.alternatives.length > 0 ||
            result.workflowTrace.critique) && (
            <div className="workflow-trace">
              <div className="workflow-trace-title">
                <div>
                  <span>可审计工作流 · {result.workflowTrace.version}</span>
                  <strong>查看模型如何比较、反驳再裁决</strong>
                </div>
                <small>
                  {result.workflowTrace.outputValidation
                    ? `最终输出 ${result.workflowTrace.outputValidation.status === "repaired" ? "经 1 次自动修复后" : "首次"}通过机器校验。`
                    : "以下是显式要求模型输出的研究产物，不是隐藏思维过程。"}
                </small>
              </div>
              {result.workflowTrace.researchPlan && (
                <details>
                  <summary>
                    <span>01</span>
                    <div>
                      <strong>研究计划</strong>
                      <small>假设、未知与检索线索</small>
                    </div>
                    <ChevronRight size={15} />
                  </summary>
                  <div className="trace-content">
                    {result.workflowTrace.researchPlan}
                  </div>
                </details>
              )}
              {result.workflowTrace.memoryItems.length > 0 && (
                <details>
                  <summary>
                    <span>M</span>
                    <div>
                      <strong>实际采用的长期记忆</strong>
                      <small>
                        {result.workflowTrace.memoryItems.length} 条 ·
                        显示命中原因与冲突信号
                      </small>
                    </div>
                    <ChevronRight size={15} />
                  </summary>
                  <MemoryItems
                    items={result.workflowTrace.memoryItems}
                    compact
                  />
                </details>
              )}
              {result.workflowTrace.alternatives.map((alternative, index) => (
                <details key={alternative.id}>
                  <summary>
                    <span>{String(index + 2).padStart(2, "0")}</span>
                    <div>
                      <strong>{alternative.label}</strong>
                      <small>{alternative.lens}</small>
                    </div>
                    <ChevronRight size={15} />
                  </summary>
                  <div className="trace-content">{alternative.content}</div>
                </details>
              ))}
              {result.workflowTrace.critique && (
                <details>
                  <summary>
                    <span>
                      {String(
                        result.workflowTrace.alternatives.length + 2,
                      ).padStart(2, "0")}
                    </span>
                    <div>
                      <strong>独立风险审查</strong>
                      <small>寻找证据漏洞、极端风险与过度自信</small>
                    </div>
                    <ChevronRight size={15} />
                  </summary>
                  <div className="trace-content">
                    {result.workflowTrace.critique}
                  </div>
                </details>
              )}
              <details className="call-trace">
                <summary>
                  <span>Σ</span>
                  <div>
                    <strong>模型调用记录</strong>
                    <small>
                      {result.workflowTrace.calls.length} 个独立阶段
                    </small>
                  </div>
                  <ChevronRight size={15} />
                </summary>
                <div className="call-list">
                  {result.workflowTrace.calls.map((call) => (
                    <div key={call.stage}>
                      <strong>{call.label}</strong>
                      <span>{(call.latencyMs / 1000).toFixed(2)} 秒</span>
                      <span>
                        {call.inputTokens == null
                          ? "token 未返回"
                          : `${call.inputTokens.toLocaleString()} 入 / ${(call.outputTokens ?? 0).toLocaleString()} 出`}
                      </span>
                    </div>
                  ))}
                </div>
              </details>
            </div>
          )}
          <div className="final-answer-label">
            <Sparkles size={15} />
            <div>
              <span>最终综合裁决</span>
              <strong>吸收方案与反方审查后的行动建议</strong>
            </div>
          </div>
          {result.workflowTrace.outputValidation && (
            <details className="payload-details">
              <summary>
                输出检查：
                {result.workflowTrace.outputValidation.status === "repaired"
                  ? "修复后通过"
                  : "首次通过"}
              </summary>
              <p className="memory-policy">
                已检查字段完整性和引用 ID
                是否属于本次授权证据；不代表事实准确性或推理有效性已经得到验证。
              </p>
              {result.workflowTrace.outputValidation.errors.length > 0 && (
                <pre>
                  {result.workflowTrace.outputValidation.errors.join("\n")}
                </pre>
              )}
            </details>
          )}
          {result.workflowTrace.structuredReport ? (
            <StructuredReportView
              report={result.workflowTrace.structuredReport}
              evidence={result.workflowTrace.evidenceCatalog ?? []}
              analysisId={result.id}
              onCreateDecisionDraft={onCreateDecisionDraft}
            />
          ) : (
            <div className="answer">{result.answer}</div>
          )}
          <p className="disclaimer">{result.disclaimer}</p>
        </section>
      )}
      <section className="panel analysis-history-panel">
        <div className="panel-title">
          <div>
            <span>本地分析档案</span>
            <h2>回看当时的问题，而不是依赖记忆改写</h2>
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
              disabled={Boolean(historyBusy)}
            >
              <div>
                <History size={15} />
                <span>{item.workflowVersion || "旧版分析"}</span>
              </div>
              <strong>{item.question}</strong>
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
      </section>
    </div>
  );
}
