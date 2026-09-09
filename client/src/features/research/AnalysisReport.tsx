import {
  AlertTriangle,
  ArrowRight,
  BrainCircuit,
  Check,
  ChevronRight,
  FilePenLine,
  History,
  Sparkles,
} from "lucide-react";
import type {
  AnalysisClaim,
  AnalysisAction,
  AnalysisEvidenceReference,
  AnalysisWorkflowTrace,
  DecisionEntry,
  StructuredAnalysis,
  StoredAnalysis,
} from "../../types";
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
  analysisId,
  onCreateDecisionDraft,
}: {
  report: StructuredAnalysis;
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
      thesis: `${report.verdict}\n\n拟采取行动：${action.action}\n理由：${action.rationale}`,
      counterThesis: counterPoints.join("\n"),
      invalidation,
    });
  };
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
      <section className="report-section">
        <div className="report-section-title">
          <ArrowRight size={14} />
          <strong>下一步行动</strong>
          <small>选择后仍需人工补全与确认</small>
        </div>
        <div className="report-action-list">
          {report.actions.map((action, index) => (
            <article key={`${action.action}-${index}`}>
              <i>{index + 1}</i>
              <div>
                <strong>{action.action}</strong>
                <p>{action.rationale}</p>
                <small>复盘：{action.reviewTrigger}</small>
                <button
                  className="decision-draft-button"
                  onClick={() => createDecisionDraft(action, index)}
                >
                  <FilePenLine size={12} />
                  转为决策草稿
                </button>
              </div>
              <em
                className={action.reversible ? "reversible" : "confirm-first"}
              >
                {action.reversible ? "可逆" : "需单独确认"}
              </em>
            </article>
          ))}
        </div>
      </section>
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
                  title={`${source.publisher} · ${source.asOfDate}`}
                >
                  {source.title} · {source.sourceTier} · {source.asOfDate}
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
