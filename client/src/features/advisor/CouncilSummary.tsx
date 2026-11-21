import { ExternalLink } from "../../components/ExternalLink";
import type { AnalysisWorkflowTrace, WebLead } from "../../types/ai";

const checkStatus = {
  covered: "已核对",
  not_applicable: "不适用",
  missing_source: "缺资料",
} as const;

function LeadList({ leads }: { leads: WebLead[] }) {
  if (leads.length === 0) return null;
  return (
    <div className="council-subblock">
      <h3>网页检索参考线索</h3>
      <p className="source-qualification">
        以下为公开网页检索线索，已供正文与模块分析参考（待公告复核），不计入一手原文引用
        ID。
      </p>
      <ul>
        {leads.map((lead, index) => (
          <li key={lead.url || `${lead.title}-${index}`}>
            {lead.url ? (
              <ExternalLink href={lead.url}>
                {lead.title || lead.url}
              </ExternalLink>
            ) : (
              <strong>{lead.title || "网页检索摘要"}</strong>
            )}
            ：{lead.summary}
          </li>
        ))}
      </ul>
    </div>
  );
}

/**
 * Progressive-disclosure display for v1 modular investment workflow:
 * - Default view shows each specialist module's concise one-line stance so users can scan all
 *   dimensions at a glance without wading through walls of text.
 * - Each module expands on demand for detailed reasoning, key points, triggers, and data gaps.
 * - Research checks, gaps, and web leads stay neatly folded below.
 */
export function CouncilSummary({ trace }: { trace: AnalysisWorkflowTrace }) {
  const modules = trace.modules ?? [];
  const synthesis = trace.synthesis;
  const leads = trace.webSearch?.leads ?? [];
  const brief = trace.research?.brief;
  const gaps = [
    ...(brief?.unknowns ?? []),
    ...(brief?.checks ?? [])
      .filter((check) => check.status === "missing_source")
      .map((check) => check.note),
  ];
  const hasResearchDetails =
    Boolean(trace.research?.failure) ||
    gaps.length > 0 ||
    Boolean(brief?.checks.length) ||
    leads.length > 0;

  if (modules.length === 0 && !hasResearchDetails) {
    return null;
  }

  const used =
    synthesis?.modulesUsed ?? modules.filter((m) => m.analysis).length;
  const total = synthesis?.modulesTotal ?? modules.length;
  const degraded =
    modules.length > 0 && (synthesis?.status === "degraded" || used < total);

  return (
    <div className="workflow-breakdown">
      {modules.length > 0 && (
        <section className="module-breakdown" aria-label="分模块讨论">
          <div className="module-breakdown-head">
            <h2>分模块讨论</h2>
            <span className="muted">
              {used}/{total} 个专业模块
              {degraded ? " · 部分降级" : ""}
            </span>
          </div>
          <div className="module-list">
            {modules.map((item) => (
              <article key={item.id} className="module-card">
                <div className="module-card-summary">
                  <span className="module-badge">{item.label}</span>
                  <p className="module-stance">
                    {item.analysis
                      ? item.analysis.stance
                      : `未完成（${item.failure ?? "未通过结构化校验"}）`}
                  </p>
                </div>
                {item.analysis && (
                  <details className="module-detail">
                    <summary>展开拆解</summary>
                    <p>{item.analysis.summary}</p>
                    {item.analysis.keyPoints.length > 0 && (
                      <ul>
                        {item.analysis.keyPoints.map((point, idx) => (
                          <li key={idx}>{point}</li>
                        ))}
                      </ul>
                    )}
                    {item.analysis.triggers.length > 0 && (
                      <p className="muted">
                        <strong>触发 / 证伪：</strong>
                        {item.analysis.triggers.join("；")}
                      </p>
                    )}
                    {!!item.analysis.dataGaps?.length && (
                      <p className="muted">
                        <strong>待补缺口：</strong>
                        {item.analysis.dataGaps.join("；")}
                      </p>
                    )}
                  </details>
                )}
              </article>
            ))}
          </div>
        </section>
      )}

      {hasResearchDetails && (
        <details className="council-process" aria-label="研究核查与网页线索">
          <summary>
            研究核查与网页线索
            {leads.length > 0 ? `（${leads.length} 条线索）` : ""}
          </summary>
          {trace.research?.failure && (
            <p>研究简报未通过校验，各专业模块直接依据冻结资料作答。</p>
          )}
          {gaps.length > 0 && (
            <div className="council-subblock">
              <h3>研究缺口</h3>
              <ul>
                {gaps.map((gap, index) => (
                  <li key={index}>{gap}</li>
                ))}
              </ul>
            </div>
          )}
          {!!brief?.checks.length && (
            <div className="council-subblock">
              <h3>核查项状态</h3>
              <ul>
                {brief.checks.map((check) => (
                  <li key={check.index}>
                    第 {check.index + 1} 项 · {checkStatus[check.status]}：
                    {check.note}
                  </li>
                ))}
              </ul>
            </div>
          )}
          <LeadList leads={leads} />
        </details>
      )}
    </div>
  );
}
