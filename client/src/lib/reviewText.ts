import type { DecisionReview, SystemReviewRecord } from "../types";

export function decisionReviewText(review: DecisionReview): string {
  if (review.content != null) return review.content;
  return [
    review.outcomeSummary,
    review.lessons && `经验与思考：${review.lessons}`,
    review.thesisStatus && `原始逻辑结果：${review.thesisStatus}`,
    review.actualReturnPct != null && `实际收益：${review.actualReturnPct}%`,
    review.processRating != null && `过程评分：${review.processRating}/5`,
  ]
    .filter((part) => typeof part === "string" && part.length > 0)
    .join("\n\n");
}

export function systemReviewText(review: SystemReviewRecord): string {
  if (review.content != null) return review.content;
  return [
    review.periodLabel && `复盘周期：${review.periodLabel}`,
    review.processSummary,
    review.ruleViolations && `规则违反：${review.ruleViolations}`,
    review.lessons && `经验与思考：${review.lessons}`,
    review.nextActions && `下一步：${review.nextActions}`,
    review.adherenceScore != null && `纪律评分：${review.adherenceScore}/5`,
    review.nextReviewDate && `下次复盘日：${review.nextReviewDate}`,
  ]
    .filter((part) => typeof part === "string" && part.length > 0)
    .join("\n\n");
}
