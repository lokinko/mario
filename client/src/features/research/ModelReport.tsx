import type { AnalysisClaim, AnalysisWorkflowTrace } from "../../types";

export function ModelReport({
  answer,
  trace,
}: {
  answer: string;
  trace?: AnalysisWorkflowTrace;
}) {
  const report = trace?.structuredReport;
  if (!report) return <div className="answer model-report">{answer}</div>;

  const sources = new Map(
    (trace?.evidenceCatalog ?? []).map((source) => [source.id, source]),
  );
  const claims = (items: AnalysisClaim[]) => (
    <ul>
      {items.map((claim, index) => (
        <li key={index}>
          {claim.statement}
          {claim.evidenceIds.map((id) => {
            const source = sources.get(id);
            return source ? (
              <a
                className="report-source"
                key={id}
                href={source.sourceUrl}
                target="_blank"
                rel="noreferrer"
              >
                {source.title}
              </a>
            ) : null;
          })}
        </li>
      ))}
    </ul>
  );
  const list = (items: string[]) => (
    <ul>
      {items.map((item, index) => (
        <li key={index}>{item}</li>
      ))}
    </ul>
  );

  return (
    <div className="model-report" aria-label="模型 report">
      <section>
        <h2>当前判断</h2>
        <p>{report.verdict}</p>
      </section>
      <section>
        <h2>已知事实</h2>
        {claims(report.facts)}
      </section>
      {report.inferences.length > 0 && (
        <section>
          <h2>合理推断</h2>
          {claims(report.inferences)}
        </section>
      )}
      <section>
        <h2>仍待核实</h2>
        {list(report.unknowns)}
      </section>
      <section>
        <h2>方案比较</h2>
        {report.options.map((option, index) => (
          <div key={index}>
            <h3>{option.name}</h3>
            <p>适用条件：{option.suitableWhen}</p>
            <p>取舍：{option.tradeoffs.join("；")}</p>
            <p>风险：{option.risks.join("；")}</p>
          </div>
        ))}
      </section>
      <section>
        <h2>下一步行动</h2>
        {report.actions.map((action, index) => (
          <div key={index}>
            <h3>{action.action}</h3>
            <p>{action.rationale}</p>
            {!!action.supportingFactIndices?.length && (
              <p>
                依据：
                {action.supportingFactIndices
                  .map((factIndex) => report.facts[factIndex]?.statement)
                  .filter(Boolean)
                  .join("；")}
              </p>
            )}
            <p>{action.reversible ? "可逆" : "需单独确认"}</p>
            {!!action.evidenceLimits?.length && (
              <p>证据局限：{action.evidenceLimits.join("；")}</p>
            )}
            <p>复盘条件：{action.reviewTrigger}</p>
          </div>
        ))}
      </section>
      <section>
        <h2>复盘与证伪条件</h2>
        {list(report.reviewTriggers)}
      </section>
    </div>
  );
}
