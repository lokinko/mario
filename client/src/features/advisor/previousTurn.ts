import type {
  AnalysisResult,
  PreviousTurn,
  StoredAnalysis,
} from "../../types/ai";

const clip = (text: string, limit: number) =>
  [...text].slice(0, limit).join("");

type Turn = {
  question: string;
  result: AnalysisResult | StoredAnalysis;
};

export function previousTurn(
  turns: readonly Turn[] = [],
): PreviousTurn | undefined {
  const turn = turns.at(-1);
  if (!turn) return undefined;
  // Frozen context is the only recovery source for user questions that are no
  // longer in the open conversation. Never infer questions from AI prose.
  const frozen = turns[0].result.workflowTrace?.personalContext?.previousTurn;
  const questions = [
    ...(frozen?.earlierQuestions ?? []),
    ...(frozen ? [frozen.question] : []),
    ...turns.slice(0, -1).map((item) => item.question),
  ];
  const priorQuestion = clip(turn.question.trim(), 200);
  const earlier = questions
    .map((question) => clip(question.trim(), 300))
    .filter(
      (question) =>
        question &&
        question !== clip(turn.question.trim(), 300) &&
        question !== priorQuestion,
    );
  const firstQuestion = earlier[0];
  const recentQuestions = earlier.filter(
    (question, index) =>
      question !== firstQuestion && earlier.lastIndexOf(question) === index,
  );
  const earlierQuestions = firstQuestion
    ? [firstQuestion, ...recentQuestions.slice(-4)]
    : [];
  const report = turn.result.workflowTrace?.structuredReport;
  const answer = report ? report.answer : turn.result.answer;
  const suggestions = report
    ? report.suggestions.map((item) => item.action)
    : [];
  const pendingQuestion = report
    ? clip(report.question?.trim() ?? "", 200)
    : "";
  return {
    question: priorQuestion,
    answer: clip(answer, 300),
    suggestions: suggestions.slice(0, 3).map((action) => clip(action, 100)),
    ...(earlierQuestions.length ? { earlierQuestions } : {}),
    ...(pendingQuestion ? { pendingQuestion } : {}),
  };
}
