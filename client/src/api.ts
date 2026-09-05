import type {
  AnalysisRequest,
  AnalysisPreview,
  AnalysisResult,
  DecisionEntry,
  DecisionRecord,
  DecisionReview,
  FinancialProfile,
  Goal,
  Holding,
  InvestmentRule,
  InvestmentRuleInput,
  InvestmentRuleRevision,
  ModelConfig,
  ModelConnectionTest,
  Snapshot,
  SystemReviewInput,
  SystemReviewRecord,
} from "./types";

const API_BASE = import.meta.env.VITE_API_URL ?? "http://127.0.0.1:4217/api";

async function httpRequest<T>(path: string, init?: RequestInit): Promise<T> {
  let response: Response | undefined;
  let lastError: unknown;
  const attempts = !init?.method || init.method === "GET" ? 10 : 1;
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    try {
      response = await fetch(`${API_BASE}${path}`, {
        ...init,
        headers: { "Content-Type": "application/json", ...(init?.headers ?? {}) },
      });
      break;
    } catch (error) {
      lastError = error;
      await new Promise((resolve) => window.setTimeout(resolve, 150));
    }
  }
  if (!response) throw lastError ?? new Error("无法连接本地服务");
  if (!response.ok) {
    const body = await response.text();
    let message = body;
    try {
      const parsed = JSON.parse(body) as { error?: string };
      if (parsed.error) message = parsed.error;
    } catch {
      // Keep the original non-JSON response for diagnostics.
    }
    throw new Error(message || `本地服务返回 ${response.status}`);
  }
  if (response.status === 204) return undefined as T;
  return response.json() as Promise<T>;
}

export async function getSnapshot(): Promise<Snapshot> {
  return httpRequest<Snapshot>("/snapshot");
}

export async function saveProfile(profile: FinancialProfile): Promise<Snapshot> {
  return httpRequest<Snapshot>("/profile", { method: "PUT", body: JSON.stringify(profile) });
}

export async function saveHolding(holding: Omit<Holding, "id">): Promise<Snapshot> {
  return httpRequest<Snapshot>("/holdings", { method: "POST", body: JSON.stringify(holding) });
}

export async function updateHolding(id: string, holding: Omit<Holding, "id">): Promise<Snapshot> {
  return httpRequest<Snapshot>(`/holdings/${encodeURIComponent(id)}`, { method: "PUT", body: JSON.stringify(holding) });
}

export async function deleteHolding(id: string): Promise<Snapshot> {
  return httpRequest<Snapshot>(`/holdings/${encodeURIComponent(id)}`, { method: "DELETE" });
}

export async function saveGoal(goal: Omit<Goal, "id">): Promise<Snapshot> {
  return httpRequest<Snapshot>("/goals", { method: "POST", body: JSON.stringify(goal) });
}

export async function updateGoal(id: string, goal: Omit<Goal, "id">): Promise<Snapshot> {
  return httpRequest<Snapshot>(`/goals/${encodeURIComponent(id)}`, { method: "PUT", body: JSON.stringify(goal) });
}

export async function deleteGoal(id: string): Promise<Snapshot> {
  return httpRequest<Snapshot>(`/goals/${encodeURIComponent(id)}`, { method: "DELETE" });
}

export async function getModelConfig(): Promise<ModelConfig> {
  return httpRequest<ModelConfig>("/model-config");
}

export async function saveModelConfig(config: Omit<ModelConfig, "hasApiKey"> & { apiKey?: string }): Promise<ModelConfig> {
  return httpRequest<ModelConfig>("/model-config", { method: "PUT", body: JSON.stringify(config) });
}

export async function testModelConnection(): Promise<ModelConnectionTest> {
  return httpRequest<ModelConnectionTest>("/model-config/test", { method: "POST" });
}

export async function deleteModelKey(): Promise<ModelConfig> {
  return httpRequest<ModelConfig>("/model-key", { method: "DELETE" });
}

export async function runAnalysis(request: AnalysisRequest): Promise<AnalysisResult> {
  return httpRequest<AnalysisResult>("/analysis", { method: "POST", body: JSON.stringify(request) });
}

export async function previewAnalysis(request: AnalysisRequest): Promise<AnalysisPreview> {
  return httpRequest<AnalysisPreview>("/analysis/preview", { method: "POST", body: JSON.stringify(request) });
}

export async function saveDecision(entry: DecisionEntry): Promise<void> {
  await httpRequest<void>("/decisions", { method: "POST", body: JSON.stringify(entry) });
}

export async function getDecisions(): Promise<DecisionRecord[]> {
  return httpRequest<DecisionRecord[]>("/decisions");
}

export async function saveDecisionReview(id: string, review: DecisionReview): Promise<void> {
  await httpRequest<void>(`/decisions/${encodeURIComponent(id)}/review`, { method: "PUT", body: JSON.stringify(review) });
}

export async function getInvestmentRules(): Promise<InvestmentRule[]> {
  return httpRequest<InvestmentRule[]>("/investment-rules");
}

export async function saveInvestmentRule(rule: InvestmentRuleInput): Promise<InvestmentRule> {
  return httpRequest<InvestmentRule>("/investment-rules", { method: "POST", body: JSON.stringify(rule) });
}

export async function updateInvestmentRule(id: string, rule: InvestmentRuleInput): Promise<InvestmentRule> {
  return httpRequest<InvestmentRule>(`/investment-rules/${encodeURIComponent(id)}`, { method: "PUT", body: JSON.stringify(rule) });
}

export async function getInvestmentRuleHistory(id: string): Promise<InvestmentRuleRevision[]> {
  return httpRequest<InvestmentRuleRevision[]>(`/investment-rules/${encodeURIComponent(id)}/history`);
}

export async function getSystemReviews(): Promise<SystemReviewRecord[]> {
  return httpRequest<SystemReviewRecord[]>("/system-reviews");
}

export async function saveSystemReview(review: SystemReviewInput): Promise<SystemReviewRecord> {
  return httpRequest<SystemReviewRecord>("/system-reviews", { method: "POST", body: JSON.stringify(review) });
}
