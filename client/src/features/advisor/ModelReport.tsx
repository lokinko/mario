import { Drawer } from "../../components/Drawer";
import { useEffect, useId, useRef, useState } from "react";
import { Check, Copy } from "lucide-react";
import type { AnalysisClaim, AnalysisWorkflowTrace } from "../../types/ai";
import { CouncilSummary } from "./CouncilSummary";
import { reportSources, SourceDetail } from "./ReportSources";

function reportCopyText(answer: string, trace?: AnalysisWorkflowTrace): string {
  const report = trace?.structuredReport;
  if (!report) return answer;
  const lines = [report.answer];
  if (report.suggestions.length) {
    lines.push(
      report.suggestions
        .map(
          (item, index) =>
            `${index + 1}. ${item.action}\n   ${item.why}${item.unlessWhen ? `（前提：${item.unlessWhen}）` : ""}`,
        )
        .join("\n"),
    );
  }
  const modules = (trace?.modules ?? []).filter((item) => item.analysis);
  if (modules.length > 0) {
    lines.push(
      "分模块立场：\n" +
        modules
          .map((item) => `- ${item.label}：${item.analysis!.stance}`)
          .join("\n"),
    );
  }
  if (report.question) lines.push(`追问：${report.question}`);
  return lines.filter(Boolean).join("\n\n");
}

export function ModelReport({
  answer,
  trace,
}: {
  answer: string;
  trace?: AnalysisWorkflowTrace;
}) {
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const copyTimer = useRef<ReturnType<typeof window.setTimeout> | undefined>(
    undefined,
  );
  useEffect(() => () => window.clearTimeout(copyTimer.current), []);
  const regionId = useId();
  const lastTrigger = useRef<HTMLButtonElement | null>(null);
  const report = trace?.structuredReport;
  const copyReport = async () => {
    const text = reportCopyText(answer, trace);
    if (!text) return;
    try {
      await navigator.clipboard?.writeText(text);
      setCopied(true);
      window.clearTimeout(copyTimer.current);
      copyTimer.current = window.setTimeout(() => setCopied(false), 2000);
    } catch {
      /* Ignore clipboard permission errors in restricted contexts. */
    }
  };
  if (!report) return <div className="answer model-report">{answer}</div>;

  const { sources, personalFacts, localByFact } = reportSources(trace);
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
  const claims = (items: AnalysisClaim[]) => (
    <ul>
      {items.map((claim, index) => (
        <li key={index}>
          {claim.statement}
          {[
            ...new Set([
              ...claim.evidenceIds,
              ...(localByFact.get(index) ?? []),
            ]),
          ].map(citation)}
        </li>
      ))}
    </ul>
  );

  const route = trace?.route;

  return (
    <div className="model-report" aria-label="模型分析报告">
      {route && (
        <div className="report-meta" aria-label="分析场景与模式">
          <span className="report-meta-tag">{route.domainLabel}</span>
          <span className="report-meta-mode">
            {route.executionMode === "fast" ? "精简直答" : "分模块策略汇总"}
          </span>
          {!!route.companies?.length && (
            <span className="report-meta-companies">
              {route.companies.join(" · ")}
            </span>
          )}
        </div>
      )}

      <p className="report-answer">{report.answer}</p>

      {report.suggestions.length > 0 && (
        <ol className="report-suggestions" aria-label="建议">
          {report.suggestions.map((suggestion, index) => (
            <li key={index}>
              <p className="suggestion-action">{suggestion.action}</p>
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
      )}

      {trace && <CouncilSummary trace={trace} />}

      {report.question && (
        <p className="report-followup">追问：{report.question}</p>
      )}

      <div className="report-footer">
        <details className="report-citations" aria-label="本次回答引用">
          <summary>依据与局限</summary>
          {trace?.webSearch?.warnings.map((warning, index) => (
            <p key={index}>{warning}</p>
          ))}
          {claims(report.facts)}
          {personalFacts.length > 0 && (
            <ul>
              {personalFacts.map((item) => (
                <li key={item.id}>
                  <strong>{item.title}</strong>：{item.detail}
                </li>
              ))}
            </ul>
          )}
          {sources.length > 0 && (
            <ul>
              {sources.map((source) => (
                <li key={source.id}>
                  {citation(source.id)} {source.title || "原始资料"}
                </li>
              ))}
            </ul>
          )}
          {sources.length === 0 && personalFacts.length === 0 && (
            <p>本次回答未提供可核对的引用来源。</p>
          )}
        </details>
        <button
          type="button"
          className="text-button report-copy-button"
          onClick={() => void copyReport()}
        >
          {copied ? <Check size={14} /> : <Copy size={14} />}
          {copied ? "已复制" : "复制回答"}
        </button>
      </div>
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
