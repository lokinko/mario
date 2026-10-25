import type { AnalysisResult, PreviousTurn, StoredAnalysis } from "../../types";

const clip = (text: string, limit: number) =>
  [...text].slice(0, limit).join("");

export function previousTurn(turn?: {
  question: string;
  result: AnalysisResult | StoredAnalysis;
}): PreviousTurn | undefined {
  if (!turn) return undefined;
  const report = turn.result.workflowTrace?.structuredReport;
  const answer = report
    ? "suggestions" in report
      ? report.answer
      : report.verdict
    : turn.result.answer;
  const suggestions = report
    ? ("suggestions" in report ? report.suggestions : report.actions).map(
        (item) => item.action,
      )
    : [];
  return {
    question: clip(turn.question, 200),
    answer: clip(answer, 300),
    suggestions: suggestions.slice(0, 3).map((action) => clip(action, 100)),
  };
}
