import { Drawer } from "../../components/Drawer";
import { useId, useRef, useState } from "react";
import type { AnalysisClaim, AnalysisWorkflowTrace } from "../../types";
import { reportSources, SourceDetail } from "./ReportSources";

export function ModelReport({
  answer,
  trace,
}: {
  answer: string;
  trace?: AnalysisWorkflowTrace;
}) {
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const regionId = useId();
  const lastTrigger = useRef<HTMLButtonElement | null>(null);
  const report = trace?.structuredReport;
  if (!report) return <div className="answer model-report">{answer}</div>;

  const { sources, localByFact, localByInference } = reportSources(trace);
  const sourceById = new Map(
    sources.map((source, index) => [source.id, { source, number: index + 1 }]),
  );
  const selected = selectedId ? sourceById.get(selectedId)?.source : undefined;
  const citation = (id: string) => {
    const item = sourceById.get(id);
    return item ? (
      <button
        type="button"
        className="report-source"
        key={id}
        title={`${item.source.title || "原始资料"}${item.source.asOfDate ? ` · ${item.source.asOfDate}` : ""}`}
        aria-label={`查看引用 ${item.number}：${item.source.title || "原始资料"}`}
        aria-expanded={selectedId === id}
        aria-controls={regionId}
        onClick={(event) => {
          lastTrigger.current = event.currentTarget;
          setSelectedId(id);
        }}
      >
        [{item.number}]
      </button>
    ) : null;
  };
  const claims = (items: AnalysisClaim[], facts = false) => (
    <ul>
      {items.map((claim, index) => (
        <li key={index}>
          {claim.statement}
          {[
            ...new Set([
              ...claim.evidenceIds,
              ...((facts ? localByFact : localByInference).get(index) ?? []),
            ]),
          ].map(citation)}
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
      {"suggestions" in report ? (
        <>
          <p>{report.answer}</p>
          <ol aria-label="建议">
            {report.suggestions.map((suggestion, index) => (
              <li key={index}>
                <p>{suggestion.action}</p>
                <p className="muted">
                  {suggestion.why}
                  {[
                    ...new Set(
                      suggestion.factIndices.flatMap((factIndex) => [
                        ...(report.facts[factIndex]?.evidenceIds ?? []),
                        ...(localByFact.get(factIndex) ?? []),
                      ]),
                    ),
                  ].map(citation)}
                </p>
                {suggestion.unlessWhen && (
                  <p className="muted">前提：{suggestion.unlessWhen}</p>
                )}
              </li>
            ))}
          </ol>
          {report.question && <p>追问：{report.question}</p>}
        </>
      ) : (
        <>
          <section>
            <h2>当前判断</h2>
            <p>{report.verdict}</p>
          </section>
          <section>
            <h2>已知事实</h2>
            {claims(report.facts, true)}
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
        </>
      )}
      <details className="report-citations" aria-label="本次回答引用">
        <summary>依据与局限</summary>
        {trace?.webSearch?.warnings.map((warning, index) => (
          <p key={index}>{warning}</p>
        ))}
        {claims(report.facts, true)}
        <ul>
          {sources.map((source) => (
            <li key={source.id}>
              {citation(source.id)} {source.title || "原始资料"}
            </li>
          ))}
        </ul>
        {sources.length === 0 && <p>本次回答未提供可核对的引用来源。</p>}
      </details>
      {selected && (
        <Drawer
          title="引用原始信息"
          onClose={() => {
            setSelectedId(null);
            lastTrigger.current?.focus();
          }}
        >
          <SourceDetail
            key={selected.id}
            source={selected}
            regionId={regionId}
            close={() => {
              setSelectedId(null);
              lastTrigger.current?.focus();
            }}
          />
        </Drawer>
      )}
    </div>
  );
}
