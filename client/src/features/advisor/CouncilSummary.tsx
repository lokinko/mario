import { ExternalLink } from "../../components/ExternalLink";
import type { AnalysisWorkflowTrace, VoteTrace, WebLead } from "../../types/ai";

const outcome: Record<VoteTrace["status"], string> = {
  voted: "交叉评审后采纳",
  tied: "评审首选与评分持平，未分出高下，按方案顺序采纳",
  contested: "评审未形成共识，按评分排序采纳",
  single: "只有一个方案通过校验，未投票，直接采纳",
  unvoted: "没有有效投票，按方案顺序采纳",
};

const checkStatus = {
  covered: "已有资料",
  not_applicable: "与本题无关",
  missing_source: "缺少来源",
} as const;

function LeadList({ leads, note }: { leads: WebLead[]; note: string }) {
  if (leads.length === 0) return null;
  return (
    <>
      <h3>网页检索参考线索</h3>
      <p className="source-qualification">{note}</p>
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
    </>
  );
}

/** How the adopted answer was routed, discussed by specialist modules, or chosen in legacy votes. */
export function CouncilSummary({ trace }: { trace: AnalysisWorkflowTrace }) {
  const route = trace.route;
  const modules = trace.modules ?? [];
  const synthesis = trace.synthesis;
  const vote = trace.vote;
  const proposals = trace.proposals ?? [];
  const leads = trace.webSearch?.leads ?? [];
  const brief = trace.research?.brief;
  const gaps = [
    ...(brief?.unknowns ?? []),
    ...(brief?.checks ?? [])
      .filter((check) => check.status === "missing_source")
      .map((check) => check.note),
  ];

  // Current modular workflow (v20+): rules & strategy → specialist modules → strategist synthesis.
  if (modules.length > 0) {
    const used =
      synthesis?.modulesUsed ?? modules.filter((m) => m.analysis).length;
    const total = synthesis?.modulesTotal ?? modules.length;
    const degraded = synthesis?.status === "degraded" || used < total;
    return (
      <details className="council-process" aria-label="分模块讨论与策略汇总">
        <summary>
          分模块讨论与策略汇总：{used}/{total} 个专业模块
          {degraded ? "（部分降级）" : ""}
          {route ? ` · ${route.domainLabel}` : ""}
        </summary>
        {route && (
          <p>
            投研场景与策略：{route.domainLabel}（{route.reason}）
          </p>
        )}
        {!!route?.companies?.length && (
          <p>涉及公开标的：{route.companies.join("、")}</p>
        )}
        <ul>
          {modules.map((item) => (
            <li key={item.id}>
              <strong>
                {item.id} · {item.label}
              </strong>
              {item.analysis ? (
                <>
                  ：{item.analysis.stance}
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
                      触发 / 证伪条件：{item.analysis.triggers.join("；")}
                    </p>
                  )}
                </>
              ) : (
                <>：未完成（{item.failure ?? "未通过结构化校验"}）</>
              )}
            </li>
          ))}
        </ul>
        {trace.research?.failure && (
          <p>研究简报未通过校验，各专业模块直接依据冻结资料作答。</p>
        )}
        {gaps.length > 0 && (
          <>
            <h3>研究缺口</h3>
            <ul>
              {gaps.map((gap, index) => (
                <li key={index}>{gap}</li>
              ))}
            </ul>
          </>
        )}
        {!!brief?.checks.length && (
          <>
            <h3>核查项状态</h3>
            <ul>
              {brief.checks.map((check) => (
                <li key={check.index}>
                  第 {check.index + 1} 项 · {checkStatus[check.status]}：
                  {check.note}
                </li>
              ))}
            </ul>
          </>
        )}
        <LeadList
          leads={leads}
          note="公开网页检索线索已供正文分析参考（待公告复核），但非一手原文，不计入可引用事实 ID。"
        />
      </details>
    );
  }

  // Fast mode or routed analysis without legacy vote.
  if (!vote || proposals.length === 0) {
    if (!route) return null;
    return (
      <details className="council-process" aria-label="分析路径">
        <summary>
          分析路径：{route.domainLabel} ·{" "}
          {route.executionMode === "fast" ? "精简快速模式" : "分模块投研讨论"}
        </summary>
        <p>{route.reason}</p>
        {!!route.companies?.length && (
          <p>涉及公开标的：{route.companies.join("、")}</p>
        )}
        <LeadList
          leads={leads}
          note="公开网页检索线索仅供参考（待公告复核），不是网页原文，不计入可引用事实 ID。"
        />
      </details>
    );
  }

  // Legacy council vote trace (v18/v19 historical analyses).
  const label = (id: string) =>
    proposals.find((proposal) => proposal.id === id)?.label ?? id;
  const objections = vote.ballots.flatMap((ballot) =>
    (ballot.ballot?.assessments ?? [])
      .filter((assessment) => assessment.proposal === vote.winner)
      .map((assessment) => ({ voter: ballot.voter, ...assessment })),
  );
  return (
    <>
      {objections.length > 0 && (
        <section className="council-objections" aria-label="其他方案代理的质疑">
          <h2>其他方案代理的质疑</h2>
          <ul>
            {objections.map((item) => (
              <li key={item.voter}>
                {item.veto && <strong>否决：{item.veto}。</strong>}
                {item.objection}
                <span className="muted">（来自 {label(item.voter)}）</span>
              </li>
            ))}
          </ul>
        </section>
      )}
      <details className="council-process" aria-label="评议过程">
        <summary>
          评议过程：{outcome[vote.status]} {label(vote.winner)}
          {route ? ` · ${route.domainLabel}` : ""}
        </summary>
        {route && (
          <p>
            投研场景：{route.domainLabel}（{route.reason}）
          </p>
        )}
        <ul>
          {proposals.map((proposal) => {
            const row = vote.tally.find(
              (item) => item.proposal === proposal.id,
            );
            const detail = proposal.failure
              ? "未通过结构化校验"
              : !row || row.voters === 0
                ? "未获评审"
                : `${row.preferences}/${row.voters} 票首选，平均 ${(row.scoreTotal / row.voters).toFixed(1)} / 10 分，${row.vetoes} 票否决${row.eliminated ? "，已淘汰" : ""}`;
            return (
              <li key={proposal.id}>
                {proposal.label}
                {proposal.id === vote.winner ? "（采纳）" : ""}：{detail}
              </li>
            );
          })}
        </ul>
        {trace.research?.failure && (
          <p>研究简报未通过校验，三个方案直接依据冻结资料作答。</p>
        )}
        {gaps.length > 0 && (
          <>
            <h3>研究缺口</h3>
            <ul>
              {gaps.map((gap, index) => (
                <li key={index}>{gap}</li>
              ))}
            </ul>
          </>
        )}
        {!!brief?.checks.length && (
          <>
            <h3>核查项状态</h3>
            <ul>
              {brief.checks.map((check) => (
                <li key={check.index}>
                  第 {check.index + 1} 项 · {checkStatus[check.status]}：
                  {check.note}
                </li>
              ))}
            </ul>
          </>
        )}
        {leads.length > 0 && (
          <>
            <h3>待核实网页线索</h3>
            <p className="source-qualification">
              供应商生成的摘要，不是网页原文，未作为回答依据。
            </p>
            <ul>
              {leads.map((lead, index) => (
                <li key={lead.url || `${lead.title}-${index}`}>
                  <ExternalLink href={lead.url}>
                    {lead.title || lead.url}
                  </ExternalLink>
                  ：{lead.summary}
                </li>
              ))}
            </ul>
          </>
        )}
      </details>
    </>
  );
}
