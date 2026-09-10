import {
  AlertTriangle,
  BrainCircuit,
  Check,
  ChevronRight,
  FilePenLine,
  History,
  Sparkles,
} from "lucide-react";
import type {
  AdviceGrounding,
  AnalysisClaim,
  AnalysisAction,
  AnalysisEvidenceReference,
  AnalysisWorkflowTrace,
  DecisionEntry,
  StructuredAnalysis,
  StoredAnalysis,
} from "../../types";
import { formatMoney } from "../../lib/format";
import { emptyDecision } from "../decisions/DecisionJournal";
import { MemoryItems } from "../memory/MemoryItems";

export function StoredAnalysisView({
  item,
  onClose,
  onReuse,
  onCreateDecisionDraft,
}: {
  item: StoredAnalysis;
  onClose: () => void;
  onReuse: () => void;
  onCreateDecisionDraft: (draft: DecisionEntry) => void;
}) {
  const trace = item.workflowTrace;
  const audit = item.transparency;
  return (
    <section className="panel stored-analysis" id="stored-analysis">
      <div className="panel-title stored-analysis-title">
        <div>
          <span>冻结于 {new Date(item.createdAt).toLocaleString("zh-CN")}</span>
          <h2>{item.question}</h2>
        </div>
        <div>
          <button className="text-button" onClick={onClose}>
            关闭
          </button>
          <button className="secondary" onClick={onReuse}>
            用当前数据重新分析
          </button>
        </div>
      </div>
      <div className="historical-analysis-warning">
        <History size={17} />
        <div>
          <strong>历史 AI 分析 · 未经结果验证</strong>
          <p>
            这是当时保存的原始研究产物，不会按当前持仓或新证据自动更新。重新分析会重新生成发送前预览。
          </p>
        </div>
      </div>
      {trace && <WebSearchStatus trace={trace} />}
      {audit && (
        <div className="analysis-audit stored-analysis-audit">
          <div>
            <strong>{audit.model}</strong>
            <span>{audit.provider}</span>
          </div>
          <div>
            <strong>{audit.modelCalls || "—"} 次</strong>
            <span>模型调用</span>
          </div>
          <div>
            <strong>
              {audit.totalLatencyMs
                ? `${(audit.totalLatencyMs / 1000).toFixed(1)} 秒`
                : "—"}
            </strong>
            <span>模型总耗时</span>
          </div>
          <div>
            <strong>{audit.memoryItemsUsed} 条</strong>
            <span>采用记忆</span>
          </div>
          <div>
            <strong>{audit.evidenceItemsUsed} 条</strong>
            <span>带来源证据</span>
          </div>
          <div>
            <strong>
              {audit.structuredOutputValidated
                ? audit.outputRepairs > 0
                  ? `修复 ${audit.outputRepairs} 次`
                  : "直接通过"
                : "旧版/未校验"}
            </strong>
            <span>输出契约</span>
          </div>
        </div>
      )}
      {trace && <StoredWorkflowTrace trace={trace} />}
      <div className="final-answer-label">
        <Sparkles size={15} />
        <div>
          <span>当时的综合裁决</span>
          <strong>请结合当前事实重新判断，不把旧回答当作实时建议</strong>
        </div>
      </div>
      {trace?.outputValidation && (
        <details className="payload-details">
          <summary>
            输出检查：
            {trace.outputValidation.status === "repaired"
              ? "修复后通过"
              : "首次通过"}
          </summary>
          <p className="memory-policy">
            这里只证明输出符合当时的字段与引用契约，不证明事实、推理或未来结果正确。
          </p>
          {trace.outputValidation.errors.length > 0 && (
            <pre>{trace.outputValidation.errors.join("\n")}</pre>
          )}
        </details>
      )}
      {trace?.structuredReport ? (
        <StructuredReportView
          report={trace.structuredReport}
          personalContext={trace.personalContext}
          grounding={trace.adviceGrounding}
          evidence={trace.evidenceCatalog ?? []}
          analysisId={item.id}
          onCreateDecisionDraft={onCreateDecisionDraft}
        />
      ) : (
        <div className="answer">{item.answer}</div>
      )}
      <p className="disclaimer">
        历史记录仅用于复盘当时的研究过程，不构成投资建议或收益保证。
      </p>
    </section>
  );
}

export function StoredWorkflowTrace({
  trace,
}: {
  trace: AnalysisWorkflowTrace;
}) {
  if (
    !trace.researchPlan &&
    trace.alternatives.length === 0 &&
    !trace.critique &&
    trace.calls.length === 0
  )
    return null;
  return (
    <div className="workflow-trace">
      <div className="workflow-trace-title">
        <div>
          <span>可审计工作流 · {trace.version || "旧版"}</span>
          <strong>当时实际保存的研究阶段</strong>
        </div>
        <small>以下是模型被明确要求输出的工作产物，不是隐藏思维过程。</small>
      </div>
      {trace.researchPlan && (
        <details>
          <summary>
            <span>01</span>
            <div>
              <strong>研究计划</strong>
              <small>假设、未知与检索线索</small>
            </div>
            <ChevronRight size={15} />
          </summary>
          <div className="trace-content">{trace.researchPlan}</div>
        </details>
      )}
      {trace.memoryItems.length > 0 && (
        <details>
          <summary>
            <span>M</span>
            <div>
              <strong>实际采用的长期记忆</strong>
              <small>{trace.memoryItems.length} 条 · 保留当时命中原因</small>
            </div>
            <ChevronRight size={15} />
          </summary>
          <MemoryItems items={trace.memoryItems} compact />
        </details>
      )}
      {trace.alternatives.map((alternative, index) => (
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
      {trace.critique && (
        <details>
          <summary>
            <span>
              {String(trace.alternatives.length + 2).padStart(2, "0")}
            </span>
            <div>
              <strong>独立风险审查</strong>
              <small>证据漏洞、极端风险与过度自信</small>
            </div>
            <ChevronRight size={15} />
          </summary>
          <div className="trace-content">{trace.critique}</div>
        </details>
      )}
      {trace.calls.length > 0 && (
        <details className="call-trace">
          <summary>
            <span>Σ</span>
            <div>
              <strong>模型调用记录</strong>
              <small>{trace.calls.length} 个独立阶段</small>
            </div>
            <ChevronRight size={15} />
          </summary>
          <div className="call-list">
            {trace.calls.map((call) => (
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
      )}
    </div>
  );
}

export function StructuredReportView({
  report,
  evidence,
  personalContext,
  grounding,
  analysisId,
  onCreateDecisionDraft,
}: {
  report: StructuredAnalysis;
  personalContext?: AnalysisWorkflowTrace["personalContext"];
  grounding?: AdviceGrounding[];
  evidence: AnalysisEvidenceReference[];
  analysisId: string;
  onCreateDecisionDraft: (draft: DecisionEntry) => void;
}) {
  const evidenceById = new Map(evidence.map((item) => [item.id, item]));
  return (
    <div className="structured-report">
      <section className="report-verdict">
        <span>当前最重要判断</span>
        <p>{report.verdict}</p>
      </section>
      <div className="report-claim-grid">
        <section>
          <div className="report-section-title">
            <Check size={14} />
            <strong>已知事实</strong>
            <small>用户数据或已授权证据</small>
          </div>
          <ClaimList items={report.facts} evidenceById={evidenceById} />
        </section>
        <section>
          <div className="report-section-title">
            <BrainCircuit size={14} />
            <strong>合理推断</strong>
            <small>不与事实混写</small>
          </div>
          {report.inferences.length > 0 ? (
            <ClaimList items={report.inferences} evidenceById={evidenceById} />
          ) : (
            <p className="report-empty">本次没有需要单列的推断。</p>
          )}
        </section>
      </div>
      <section className="report-section unknown-section">
        <div className="report-section-title">
          <AlertTriangle size={14} />
          <strong>仍待核实</strong>
          <small>模型不得补写为事实</small>
        </div>
        <ul>
          {report.unknowns.map((item, index) => (
            <li key={`${item}-${index}`}>{item}</li>
          ))}
        </ul>
      </section>
      <section className="report-section">
        <div className="report-section-title">
          <BrainCircuit size={14} />
          <strong>方案与取舍</strong>
          <small>{report.options.length} 条可行路径</small>
        </div>
        <div className="report-option-grid">
          {report.options.map((option, index) => (
            <article key={`${option.name}-${index}`}>
              <span>方案 {String(index + 1).padStart(2, "0")}</span>
              <h3>{option.name}</h3>
              <p>
                <b>适用条件</b>
                {option.suitableWhen}
              </p>
              <p>
                <b>机会成本</b>
                {option.tradeoffs.join("；")}
              </p>
              <p>
                <b>主要风险</b>
                {option.risks.join("；")}
              </p>
            </article>
          ))}
        </div>
      </section>
      <AdviceCards
        report={report}
        personalContext={personalContext}
        grounding={grounding}
        evidence={evidence}
        analysisId={analysisId}
        onCreateDecisionDraft={onCreateDecisionDraft}
      />
      <section className="report-section trigger-section">
        <div className="report-section-title">
          <History size={14} />
          <strong>复盘与证伪条件</strong>
          <small>未来用结果校准判断</small>
        </div>
        <ul>
          {report.reviewTriggers.map((item, index) => (
            <li key={`${item}-${index}`}>{item}</li>
          ))}
        </ul>
      </section>
    </div>
  );
}

export function ClaimList({
  items,
  evidenceById,
}: {
  items: AnalysisClaim[];
  evidenceById: Map<string, AnalysisEvidenceReference>;
}) {
  return (
    <div className="report-claim-list">
      {items.map((claim, index) => (
        <article key={`${claim.statement}-${index}`}>
          <p>{claim.statement}</p>
          <footer>
            <span>
              {claim.basis === "research_evidence" ? "研究证据" : "用户数据"}
            </span>
            {claim.evidenceIds.map((id) => {
              const source = evidenceById.get(id);
              return source ? (
                <a
                  key={id}
                  href={source.sourceUrl}
                  target="_blank"
                  rel="noreferrer"
                  title={`${source.publisher} · ${source.asOfDate || "资料日期未提供"}`}
                >
                  {source.title} · {source.sourceTier} ·{" "}
                  {source.asOfDate || "资料日期未提供"}
                </a>
              ) : (
                <em key={id}>证据 {id}</em>
              );
            })}
          </footer>
        </article>
      ))}
    </div>
  );
}

export function AdviceCards({
  onAsk,
  report,
  evidence,
  personalContext,
  grounding,
  analysisId,
  onCreateDecisionDraft,
}: {
  onAsk?: (question: string) => void;
  report: StructuredAnalysis;
  personalContext?: AnalysisWorkflowTrace["personalContext"];
  grounding?: AdviceGrounding[];
  evidence: AnalysisEvidenceReference[];
  analysisId: string;
  onCreateDecisionDraft: (draft: DecisionEntry) => void;
}) {
  const evidenceById = new Map(evidence.map((item) => [item.id, item]));
  const createDecisionDraft = (action: AnalysisAction, index: number) => {
    const counterPoints = [
      ...report.options.flatMap((option) => option.risks),
      ...report.unknowns,
    ].filter(
      (item, itemIndex, all) => item.trim() && all.indexOf(item) === itemIndex,
    );
    const invalidation = [action.reviewTrigger, ...report.reviewTriggers]
      .filter(
        (item, itemIndex, all) =>
          item.trim() && all.indexOf(item) === itemIndex,
      )
      .join("\n");
    onCreateDecisionDraft({
      ...emptyDecision,
      sourceAnalysisId: analysisId,
      sourceActionIndex: index,
      thesis: `${report.verdict}\n\n拟采取行动：${action.action}\n理由：${action.rationale}\n\n依据：${(action.supportingFactIndices ?? []).flatMap((i) => (report.facts[i] ? [report.facts[i].statement] : [])).join("；")}\n证据局限：${(action.evidenceLimits ?? []).join("；")}`,
      counterThesis: counterPoints.join("\n"),
      invalidation,
    });
  };

  return (
    <section className="advice-cards" aria-label="建议与依据">
      {report.actions.map((action, index) => {
        const review = grounding?.find((item) => item.actionIndex === index);
        const facts = (action.supportingFactIndices ?? []).flatMap((i) =>
          report.facts[i] ? [report.facts[i]] : [],
        );
        const sourceIds = [
          ...new Set(facts.flatMap((fact) => fact.evidenceIds)),
        ];
        const personalFacts = facts.filter(
          (fact) => fact.basis === "user_data",
        );
        return (
          <article
            className="evidence-advice"
            key={`${action.action}-${index}`}
          >
            <header>
              <span>建议 {index + 1}</span>
              <h3>{action.action}</h3>
            </header>
            <div
              className={`advice-grounding-status ${review?.status ?? "unavailable"}`}
            >
              <strong>
                {review?.status === "supported"
                  ? "资料范围内支持 · 模型核对"
                  : review?.status === "contradicted"
                    ? "发现证据矛盾"
                    : review?.status === "insufficient"
                      ? "证据不足"
                      : "尚未完成证据核对"}
              </strong>
              <p>
                {review?.reason ??
                  "旧记录没有逐条核对结果，请用当前资料重新分析。"}
              </p>
              {review && review.checks.length > 0 && (
                <details>
                  <summary>核对用到的原值与引文</summary>
                  <ul>
                    {review.checks.map((check, i) => (
                      <li key={i}>
                        {report.facts[check.factIndex]?.statement} —{" "}
                        {check.sourceKind === "local"
                          ? check.sourceRef === "/currentUserMessage"
                            ? "本轮陈述"
                            : "个人资料原值"
                          : "来源摘录"}
                        ：<q>{check.quote}</q>
                      </li>
                    ))}
                  </ul>
                </details>
              )}
              <small>模型核对不等于独立事实认证，也不验证未来结果。</small>
              {onAsk && review?.status !== "supported" && (
                <button
                  className="text-button"
                  onClick={() =>
                    onAsk(
                      `请先补齐这条建议的依据：“${action.action}”。核对问题：${review?.reason ?? "尚未核对"}。请指出最关键的缺失资料，并在证据不足时暂缓结论。`,
                    )
                  }
                >
                  继续核实这条建议
                </button>
              )}
            </div>
            <p className="advice-rationale">{action.rationale}</p>
            {facts.length ? (
              <div className="advice-grounding">
                <strong>为什么适合你的情况</strong>
                <ul>
                  {facts.map((fact, i) => (
                    <li key={i}>
                      <span>
                        {fact.basis === "user_data" ? "个人资料" : "外部信息"}
                      </span>
                      {fact.statement}
                    </li>
                  ))}
                </ul>
                <small>
                  {personalFacts.length} 项个人资料依据 · {sourceIds.length}{" "}
                  条外部引用
                </small>
              </div>
            ) : (
              <p className="advice-gap">
                这份旧回答没有保存逐条建议的证据关联，需重新分析后核对依据。
              </p>
            )}
            {personalFacts.length > 0 && (
              <PersonalContextEvidence context={personalContext} />
            )}
            {sourceIds.map((id) => {
              const source = evidenceById.get(id);
              return source ? (
                <SourceEvidence key={id} source={source} />
              ) : (
                <p className="advice-gap" key={id}>
                  引用 {id} 的来源记录缺失，暂时无法核实。
                </p>
              );
            })}
            {!sourceIds.length && (
              <p className="advice-gap">
                未关联外部来源；这条建议不能据此说明当前市场或产品状况。
              </p>
            )}
            {(action.evidenceLimits ?? []).length > 0 && (
              <div className="advice-limits">
                <strong>适用边界与待核实</strong>
                <ul>
                  {action.evidenceLimits!.map((limit, i) => (
                    <li key={i}>{limit}</li>
                  ))}
                </ul>
              </div>
            )}
            <footer>
              <span>重新评估：{action.reviewTrigger}</span>
              <button
                className="decision-draft-button"
                disabled={review?.status !== "supported"}
                title={
                  review?.status !== "supported"
                    ? "补充或核对证据后重新分析"
                    : undefined
                }
                onClick={() => createDecisionDraft(action, index)}
              >
                <FilePenLine size={12} />
                确认行动草稿
              </button>
            </footer>
          </article>
        );
      })}
    </section>
  );
}

export function SourceEvidence({
  source,
}: {
  source: AnalysisEvidenceReference;
}) {
  // Historic/imported data may predate URL validation. Never make unsafe schemes clickable.
  let safeUrl: string | null = null;
  try {
    const url = new URL(source.sourceUrl);
    if (
      ["https:", "http:"].includes(url.protocol) &&
      !url.username &&
      !url.password
    )
      safeUrl = url.href;
  } catch {
    /* Render the title as text when the source URL cannot be opened safely. */
  }
  return (
    <details className="source-evidence">
      <summary>
        <span>
          {source.publisher} · {source.asOfDate || "资料日期未提供"}
        </span>
        <strong>
          {safeUrl ? (
            <a
              href={safeUrl}
              target="_blank"
              rel="noreferrer"
              onClick={(event) => event.stopPropagation()}
            >
              {source.title} ↗
            </a>
          ) : (
            source.title
          )}
        </strong>
        <small>
          {source.sourceTier}
          {source.stance ? ` · ${source.stance}` : ""}
        </small>
      </summary>
      {source.claim && (
        <p>
          <b>
            {source.evidenceType === "native_web_excerpt"
              ? "原生引用片段"
              : source.evidenceType === "native_web_summary"
                ? "模型摘要（待核对原文）"
                : "来源摘要"}
          </b>
          {source.claim}
        </p>
      )}
      {source.notes && (
        <p>
          <b>口径与局限</b>
          {source.notes}
        </p>
      )}
      <p className="source-boundary">
        资料日期不代表实时行情。摘要与原文的一致性仍需核对。
      </p>
      {safeUrl ? (
        <a href={safeUrl} target="_blank" rel="noreferrer">
          打开原始来源 ↗
        </a>
      ) : (
        <span>来源链接不可用</span>
      )}
    </details>
  );
}

export function PersonalContextEvidence({
  context,
}: {
  context?: AnalysisWorkflowTrace["personalContext"];
}) {
  if (!context)
    return (
      <p className="advice-gap">
        旧记录未保存个人资料快照，不能用当前持仓替代当时依据。
      </p>
    );
  const holdings = context.portfolio?.holdings ?? [];
  const profile = context.financialProfile?.profile;
  return (
    <details className="source-evidence personal-evidence">
      <summary>
        <strong>核对这次授权的持仓与目标</strong>
        <small>分析时的资料快照 · 用户提供的数据</small>
      </summary>
      {context.currentUserMessage && (
        <p>
          <b>本轮补充（尚未写入档案）</b>：{context.currentUserMessage}
        </p>
      )}
      {profile && (
        <p>
          风险偏好：{profile.riskLevel} · 可承受回撤：{profile.maxDrawdownPct}%
          · 投资期限：{profile.horizonYears} 年
        </p>
      )}
      {holdings.length > 0 ? (
        <ul>
          {holdings.map((holding) => (
            <li key={holding.id}>
              <strong>{holding.name}</strong>{" "}
              {formatMoney(holding.marketValue, holding.currency)} · 估值日{" "}
              {holding.valuationDate || "未填写"}
              <br />
              <small>
                用户录入市值
                {context.portfolio?.holdingValuations?.some(
                  (item) => item.holdingId === holding.id,
                )
                  ? "；已附当时的价格来源记录"
                  : "；未附价格核验记录"}
              </small>
            </li>
          ))}
        </ul>
      ) : (
        <p>本次没有授权持仓明细或尚未录入持仓。</p>
      )}
      {(context.portfolio?.holdingValuations ?? []).map((quote) => (
        <SourceEvidence
          key={quote.holdingId}
          source={{
            id: quote.holdingId,
            title: `${quote.symbol} · ${quote.observedOn} 收盘价`,
            publisher: quote.providerName,
            sourceUrl: quote.sourceUrl,
            sourceTier: "价格数据服务",
            asOfDate: quote.observedOn,
            claim: `单价 ${quote.unitPrice} ${quote.currency} × 数量 ${quote.quantity} = ${quote.marketValue} ${quote.currency}；${quote.priceBasis}`,
            notes: quote.disclaimer,
            capturedAt: quote.capturedAt,
          }}
        />
      ))}
      {(context.goals ?? []).map((goal) => (
        <p key={goal.id}>
          目标：{goal.name} · 计划日期 {goal.targetDate}
        </p>
      ))}
      <p className="source-boundary">
        这些是当时提供的资料，不代表实时资产；模型对资料的概括仍需与你的实际情况核对。
      </p>
    </details>
  );
}

export function EvidenceOverview({
  report,
  evidence,
}: {
  report: StructuredAnalysis;
  evidence: AnalysisEvidenceReference[];
}) {
  const cited = new Set(
    report.facts.concat(report.inferences).flatMap((fact) => fact.evidenceIds),
  );
  const themes = [
    ...new Set(evidence.map((item) => item.assetName).filter(Boolean)),
  ];
  return (
    <details className="source-evidence evidence-overview">
      <summary>
        <strong>本次外部信息覆盖</strong>
        <small>
          {evidence.length} 条授权资料 ·{" "}
          {evidence.filter((item) => cited.has(item.id)).length} 条被回答引用
        </small>
      </summary>
      {themes.length > 0 && <p>涉及主题：{themes.join("、")}</p>}
      <p className="source-boundary">
        汇总本次授权的已保存资料，包括公开宏观指标；尚不覆盖全市场行情与最新公司披露。
      </p>
      {evidence.length ? (
        evidence.map((source) => (
          <div key={source.id}>
            <small>
              {cited.has(source.id) ? "回答已引用" : "候选资料，回答未引用"}
            </small>
            <SourceEvidence source={source} />
          </div>
        ))
      ) : (
        <p>
          当前没有可引用的外部资料，需要补充来源后再形成涉及市场环境的判断。
        </p>
      )}
    </details>
  );
}

export function WebSearchStatus({ trace }: { trace: AnalysisWorkflowTrace }) {
  if (!trace.webSearch) return null;
  return (
    <p className="source-boundary" role="status">
      {trace.webSearch.status === "completed"
        ? "已通过供应商网页搜索取得来源。"
        : "网页搜索资料不完整。"}
      {trace.webSearch.warnings.join("；")}
    </p>
  );
}
