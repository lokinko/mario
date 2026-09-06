import { invoke } from "@tauri-apps/api/core";
import type {
  AnalysisRequest,
  AnalysisPreview,
  AnalysisResult,
  AnalysisHistoryItem,
  AccountResult,
  CloudConfig,
  CloudStatus,
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
  ResearchEvidence,
  ResearchEvidenceInput,
  ReminderSettings,
  ReviewReminderSummary,
  RecoveryKeyResult,
  Snapshot,
  SystemReviewInput,
  SystemReviewRecord,
  SyncResult,
  StoredAnalysis,
} from "./types";

interface LocalServiceConfig {
  baseUrl: string;
  authToken?: string;
}

let localServiceConfig: Promise<LocalServiceConfig> | undefined;

function getLocalServiceConfig(): Promise<LocalServiceConfig> {
  if (!localServiceConfig) {
    localServiceConfig = "__TAURI_INTERNALS__" in window
      ? invoke<LocalServiceConfig>("local_service_config")
      : Promise.resolve({ baseUrl: import.meta.env.VITE_API_URL ?? "http://127.0.0.1:4217/api" });
  }
  return localServiceConfig;
}

async function httpRequest<T>(path: string, init?: RequestInit): Promise<T> {
  const service = await getLocalServiceConfig();
  let response: Response | undefined;
  let lastError: unknown;
  const attempts = !init?.method || init.method === "GET" ? 10 : 1;
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    try {
      response = await fetch(`${service.baseUrl}${path}`, {
        ...init,
        headers: {
          "Content-Type": "application/json",
          ...(service.authToken ? { Authorization: `Bearer ${service.authToken}` } : {}),
          ...(init?.headers ?? {}),
        },
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

export async function getCloudConfig(): Promise<CloudConfig | null> {
  return httpRequest<CloudConfig | null>("/cloud/config");
}

export async function getCloudStatus(): Promise<CloudStatus> {
  return httpRequest<CloudStatus>("/cloud/status");
}

export async function saveCloudConfig(config: CloudConfig): Promise<CloudStatus> {
  return httpRequest<CloudStatus>("/cloud/config", { method: "PUT", body: JSON.stringify(config) });
}

export async function signUpCloud(email: string, password: string): Promise<AccountResult> {
  return httpRequest<AccountResult>("/cloud/signup", { method: "POST", body: JSON.stringify({ email, password }) });
}

export async function signInCloud(email: string, password: string): Promise<AccountResult> {
  return httpRequest<AccountResult>("/cloud/login", { method: "POST", body: JSON.stringify({ email, password }) });
}

export async function signOutCloud(): Promise<CloudStatus> {
  return httpRequest<CloudStatus>("/cloud/session", { method: "DELETE" });
}

export async function pushCloudSync(): Promise<SyncResult> {
  return httpRequest<SyncResult>("/cloud/sync/push", { method: "POST" });
}

export async function pullCloudSync(confirmReplace: boolean): Promise<SyncResult> {
  return httpRequest<SyncResult>("/cloud/sync/pull", { method: "POST", body: JSON.stringify({ confirmReplace }) });
}

export async function exportCloudRecoveryKey(): Promise<RecoveryKeyResult> {
  return httpRequest<RecoveryKeyResult>("/cloud/recovery-key");
}

export async function importCloudRecoveryKey(recoveryKey: string, confirmReplace: boolean): Promise<void> {
  await httpRequest<void>("/cloud/recovery-key", { method: "PUT", body: JSON.stringify({ recoveryKey, confirmReplace }) });
}

export async function runAnalysis(request: AnalysisRequest): Promise<AnalysisResult> {
  return httpRequest<AnalysisResult>("/analysis", { method: "POST", body: JSON.stringify(request) });
}

export async function getAnalysisHistory(): Promise<AnalysisHistoryItem[]> {
  return httpRequest<AnalysisHistoryItem[]>("/analyses");
}

export async function getAnalysis(id: string): Promise<StoredAnalysis> {
  return httpRequest<StoredAnalysis>(`/analyses/${encodeURIComponent(id)}`);
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

export async function getReminderSettings(): Promise<ReminderSettings> {
  return httpRequest<ReminderSettings>("/reminder-settings");
}

export async function saveReminderSettings(enabled: boolean): Promise<ReminderSettings> {
  return httpRequest<ReminderSettings>("/reminder-settings", {
    method: "PUT",
    body: JSON.stringify({ enabled }),
  });
}

export async function getReviewReminders(): Promise<ReviewReminderSummary> {
  return httpRequest<ReviewReminderSummary>("/review-reminders");
}

export async function acknowledgeReviewReminder(fingerprint: string): Promise<ReviewReminderSummary> {
  return httpRequest<ReviewReminderSummary>("/review-reminders/acknowledge", {
    method: "POST",
    body: JSON.stringify({ fingerprint }),
  });
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

export async function getResearchEvidence(): Promise<ResearchEvidence[]> {
  return httpRequest<ResearchEvidence[]>("/research-evidence");
}

export async function saveResearchEvidence(evidence: ResearchEvidenceInput): Promise<ResearchEvidence> {
  return httpRequest<ResearchEvidence>("/research-evidence", { method: "POST", body: JSON.stringify(evidence) });
}

export async function setResearchEvidenceStatus(id: string, active: boolean): Promise<ResearchEvidence> {
  return httpRequest<ResearchEvidence>(`/research-evidence/${encodeURIComponent(id)}/status`, { method: "PUT", body: JSON.stringify({ active }) });
}
