import { httpRequest } from "../lib/httpClient";
import type {
  InvestmentRule,
  InvestmentRuleInput,
  InvestmentRuleRevision,
  SystemReviewDraft,
  SystemReviewRecord,
} from "../types";
export async function getInvestmentRules(): Promise<InvestmentRule[]> {
  return httpRequest<InvestmentRule[]>("/investment-rules");
}

export async function saveInvestmentRule(
  rule: InvestmentRuleInput,
): Promise<InvestmentRule> {
  return httpRequest<InvestmentRule>("/investment-rules", {
    method: "POST",
    body: JSON.stringify(rule),
  });
}

export async function updateInvestmentRule(
  id: string,
  rule: InvestmentRuleInput,
): Promise<InvestmentRule> {
  return httpRequest<InvestmentRule>(
    `/investment-rules/${encodeURIComponent(id)}`,
    { method: "PUT", body: JSON.stringify(rule) },
  );
}

export async function getInvestmentRuleHistory(
  id: string,
): Promise<InvestmentRuleRevision[]> {
  return httpRequest<InvestmentRuleRevision[]>(
    `/investment-rules/${encodeURIComponent(id)}/history`,
  );
}

export async function getSystemReviews(): Promise<SystemReviewRecord[]> {
  return httpRequest<SystemReviewRecord[]>("/system-reviews");
}

export async function saveSystemReview(
  review: SystemReviewDraft,
): Promise<SystemReviewRecord> {
  return httpRequest<SystemReviewRecord>("/system-reviews", {
    method: "POST",
    body: JSON.stringify(review),
  });
}
