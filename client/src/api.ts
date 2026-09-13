import {
  beforeDataWrite,
  markDataWrite,
  isDataWrite,
  DATA_SAVED,
  CLOUD_CHANGED,
} from "./lib/syncEvents";
import { invoke } from "@tauri-apps/api/core";
import { webToken } from "./lib/webSession";
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
  FxRateQuote,
  Goal,
  Holding,
  SecurityPriceConfig,
  SecurityPriceQuote,
  InvestmentRule,
  InvestmentRuleInput,
  InvestmentRuleRevision,
  ModelConfig,
  ModelConnectionTest,
  MemoryCandidate,
  MemoryPreferenceInput,
  PortfolioCheckInInput,
  PortfolioCheckInRecord,
  PortfolioEventInput,
  PortfolioEventImportPreview,
  PortfolioEventImportResult,
  PortfolioEventRecord,
  PortfolioEventReversalInput,
  ResearchEvidence,
  ResearchEvidenceInput,
  ReminderSettings,
  ReviewReminderSummary,
  RuleEffectivenessSummary,
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
    localServiceConfig =
      "__TAURI_INTERNALS__" in window
        ? invoke<LocalServiceConfig>("local_service_config").catch((error) => {
            localServiceConfig = undefined;
            throw error;
          })
        : Promise.resolve({
            baseUrl:
              import.meta.env.VITE_API_URL ??
              (import.meta.env.DEV ? "http://127.0.0.1:4217/api" : "/api"),
          });
  }
  return localServiceConfig;
}

async function httpRequest<T>(path: string, init?: RequestInit): Promise<T> {
  const dataWrite = isDataWrite(path, init?.method);
  const release = dataWrite ? markDataWrite() : () => {};
  try {
    if (dataWrite) await beforeDataWrite();
    const service = await getLocalServiceConfig();
    const authToken = service.authToken ?? webToken();
    const result = await requestJson<T>(`${service.baseUrl}${path}`, {
      ...init,
      timeoutMs:
        path === "/analysis" ||
        path === "/model-config/test" ||
        path === "/model-config/codex"
          ? 300000
          : path.startsWith("/cloud/")
            ? 120000
            : 15000,
      headers: {
        "Content-Type": "application/json",
        ...(authToken ? { Authorization: `Bearer ${authToken}` } : {}),
        ...(init?.headers ?? {}),
      },
    });
    if (dataWrite) window.dispatchEvent(new Event(DATA_SAVED));
    if (
      path.startsWith("/cloud/") &&
      init?.method &&
      path !== "/cloud/sync/auto"
    )
      window.dispatchEvent(new Event(CLOUD_CHANGED));
    return result;
  } finally {
    release();
  }
}

export async function getSnapshot(): Promise<Snapshot> {
  return httpRequest<Snapshot>("/snapshot");
}

export async function getFxRate(
  fromCurrency: string,
  toCurrency: string,
  onDate: string,
): Promise<FxRateQuote> {
  const query = new URLSearchParams({ fromCurrency, toCurrency, onDate });
  return httpRequest<FxRateQuote>(`/market-data/fx-rate?${query.toString()}`);
}

export async function getSecurityPrice(
  symbol: string,
  onDate: string,
): Promise<SecurityPriceQuote> {
  const query = new URLSearchParams({ symbol, onDate });
  return httpRequest<SecurityPriceQuote>(
    `/market-data/security-price?${query.toString()}`,
  );
}

export async function getSecurityPriceConfig(): Promise<SecurityPriceConfig> {
  return httpRequest<SecurityPriceConfig>("/market-data/security/config");
}

export async function saveSecurityPriceConfig(
  apiKey?: string,
): Promise<SecurityPriceConfig> {
  return httpRequest<SecurityPriceConfig>("/market-data/security/config", {
    method: "PUT",
    body: JSON.stringify({ apiKey }),
  });
}

export async function deleteSecurityPriceKey(): Promise<SecurityPriceConfig> {
  return httpRequest<SecurityPriceConfig>("/market-data/security/key", {
    method: "DELETE",
  });
}

export async function applyVerifiedHoldingValuation(
  id: string,
  symbol: string,
  quantity: number,
  onDate: string,
): Promise<Snapshot> {
  return httpRequest<Snapshot>(
    `/holdings/${encodeURIComponent(id)}/verified-valuation`,
    {
      method: "PUT",
      body: JSON.stringify({ symbol, quantity, onDate }),
    },
  );
}

export async function saveProfile(
  profile: FinancialProfile,
): Promise<Snapshot> {
  return httpRequest<Snapshot>("/profile", {
    method: "PUT",
    body: JSON.stringify(profile),
  });
}

export async function saveHolding(
  holding: Omit<Holding, "id">,
): Promise<Snapshot> {
  return httpRequest<Snapshot>("/holdings", {
    method: "POST",
    body: JSON.stringify(holding),
  });
}

export async function updateHolding(
  id: string,
  holding: Omit<Holding, "id">,
): Promise<Snapshot> {
  return httpRequest<Snapshot>(`/holdings/${encodeURIComponent(id)}`, {
    method: "PUT",
    body: JSON.stringify(holding),
  });
}

export async function deleteHolding(id: string): Promise<Snapshot> {
  return httpRequest<Snapshot>(`/holdings/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}

export async function getPortfolioCheckins(): Promise<
  PortfolioCheckInRecord[]
> {
  return httpRequest<PortfolioCheckInRecord[]>("/portfolio-checkins");
}

export async function savePortfolioCheckin(
  input: PortfolioCheckInInput,
): Promise<PortfolioCheckInRecord> {
  return httpRequest<PortfolioCheckInRecord>("/portfolio-checkins", {
    method: "POST",
    body: JSON.stringify(input),
  });
}

export async function getPortfolioEvents(): Promise<PortfolioEventRecord[]> {
  return httpRequest<PortfolioEventRecord[]>("/portfolio-events");
}

export async function getMemories(): Promise<MemoryCandidate[]> {
  return httpRequest<MemoryCandidate[]>("/memories");
}

export async function saveMemoryPreference(
  id: string,
  input: MemoryPreferenceInput,
): Promise<MemoryCandidate> {
  return httpRequest<MemoryCandidate>(
    `/memories/${encodeURIComponent(id)}/preference`,
    {
      method: "PUT",
      body: JSON.stringify(input),
    },
  );
}

export async function savePortfolioEvent(
  input: PortfolioEventInput,
): Promise<PortfolioEventRecord> {
  return httpRequest<PortfolioEventRecord>("/portfolio-events", {
    method: "POST",
    body: JSON.stringify(input),
  });
}

export async function reversePortfolioEvent(
  id: string,
  input: PortfolioEventReversalInput,
): Promise<PortfolioEventRecord> {
  return httpRequest<PortfolioEventRecord>(
    `/portfolio-events/${encodeURIComponent(id)}/reverse`,
    {
      method: "POST",
      body: JSON.stringify(input),
    },
  );
}

export async function previewPortfolioEventImport(
  csvText: string,
): Promise<PortfolioEventImportPreview> {
  return httpRequest<PortfolioEventImportPreview>(
    "/portfolio-events/import/preview",
    {
      method: "POST",
      body: JSON.stringify({ csvText }),
    },
  );
}

export async function commitPortfolioEventImport(
  csvText: string,
  previewRevision: string,
): Promise<PortfolioEventImportResult> {
  return httpRequest<PortfolioEventImportResult>(
    "/portfolio-events/import/commit",
    {
      method: "POST",
      body: JSON.stringify({ csvText, previewRevision }),
    },
  );
}

export async function saveGoal(goal: Omit<Goal, "id">): Promise<Snapshot> {
  return httpRequest<Snapshot>("/goals", {
    method: "POST",
    body: JSON.stringify(goal),
  });
}

export async function updateGoal(
  id: string,
  goal: Omit<Goal, "id">,
): Promise<Snapshot> {
  return httpRequest<Snapshot>(`/goals/${encodeURIComponent(id)}`, {
    method: "PUT",
    body: JSON.stringify(goal),
  });
}

export async function deleteGoal(id: string): Promise<Snapshot> {
  return httpRequest<Snapshot>(`/goals/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}

export async function getModelConfig(): Promise<ModelConfig> {
  return httpRequest<ModelConfig>("/model-config");
}

export async function saveModelConfig(
  config: Omit<ModelConfig, "hasApiKey"> & { apiKey?: string },
): Promise<ModelConfig> {
  return httpRequest<ModelConfig>("/model-config", {
    method: "PUT",
    body: JSON.stringify(config),
  });
}

export async function testModelConnection(): Promise<ModelConnectionTest> {
  return httpRequest<ModelConnectionTest>("/model-config/test", {
    method: "POST",
  });
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

export async function saveCloudConfig(
  config: CloudConfig,
): Promise<CloudStatus> {
  return httpRequest<CloudStatus>("/cloud/config", {
    method: "PUT",
    body: JSON.stringify(config),
  });
}

export async function signUpCloud(
  email: string,
  password: string,
): Promise<AccountResult> {
  return httpRequest<AccountResult>("/cloud/signup", {
    method: "POST",
    body: JSON.stringify({ email, password }),
  });
}

export async function signInCloud(
  email: string,
  password: string,
): Promise<AccountResult> {
  return httpRequest<AccountResult>("/cloud/login", {
    method: "POST",
    body: JSON.stringify({ email, password }),
  });
}

export async function resendCloudConfirmation(
  email: string,
): Promise<AccountResult> {
  return httpRequest<AccountResult>("/cloud/confirmation/resend", {
    method: "POST",
    body: JSON.stringify({ email }),
  });
}

export async function requestCloudPasswordReset(
  email: string,
): Promise<{ message: string }> {
  return httpRequest("/cloud/password/recover", {
    method: "POST",
    body: JSON.stringify({ email }),
  });
}

export async function verifyCloudPasswordReset(
  email: string,
  proof: string,
): Promise<{ recoveryId: string }> {
  return httpRequest("/cloud/password/verify", {
    method: "POST",
    body: JSON.stringify({ email, proof }),
  });
}

export async function resetCloudPassword(
  recoveryId: string,
  password: string,
): Promise<{ message: string }> {
  return httpRequest("/cloud/password/reset", {
    method: "POST",
    body: JSON.stringify({ recoveryId, password }),
  });
}

export async function signOutCloud(): Promise<CloudStatus> {
  return httpRequest<CloudStatus>("/cloud/session", { method: "DELETE" });
}

export async function pushCloudSync(): Promise<SyncResult> {
  return httpRequest<SyncResult>("/cloud/sync/push", { method: "POST" });
}

export async function pullCloudSync(
  confirmReplace: boolean,
): Promise<SyncResult> {
  return httpRequest<SyncResult>("/cloud/sync/pull", {
    method: "POST",
    body: JSON.stringify({ confirmReplace }),
  });
}

export async function exportCloudRecoveryKey(): Promise<RecoveryKeyResult> {
  return httpRequest<RecoveryKeyResult>("/cloud/recovery-key");
}

export async function importCloudRecoveryKey(
  recoveryKey: string,
  confirmReplace: boolean,
): Promise<void> {
  await httpRequest<void>("/cloud/recovery-key", {
    method: "PUT",
    body: JSON.stringify({ recoveryKey, confirmReplace }),
  });
}

export async function runAnalysis(
  request: AnalysisRequest,
): Promise<AnalysisResult> {
  return httpRequest<AnalysisResult>("/analysis", {
    method: "POST",
    body: JSON.stringify(request),
  });
}

export async function getAnalysisHistory(): Promise<AnalysisHistoryItem[]> {
  return httpRequest<AnalysisHistoryItem[]>("/analyses");
}

export async function getAnalysis(id: string): Promise<StoredAnalysis> {
  return httpRequest<StoredAnalysis>(`/analyses/${encodeURIComponent(id)}`);
}

export async function previewAnalysis(
  request: AnalysisRequest,
): Promise<AnalysisPreview> {
  return httpRequest<AnalysisPreview>("/analysis/preview", {
    method: "POST",
    body: JSON.stringify(request),
  });
}

export async function saveDecision(entry: DecisionEntry): Promise<void> {
  await httpRequest<void>("/decisions", {
    method: "POST",
    body: JSON.stringify(entry),
  });
}

export async function getDecisions(): Promise<DecisionRecord[]> {
  return httpRequest<DecisionRecord[]>("/decisions");
}

export async function saveDecisionReview(
  id: string,
  review: DecisionReview,
): Promise<void> {
  await httpRequest<void>(`/decisions/${encodeURIComponent(id)}/review`, {
    method: "PUT",
    body: JSON.stringify(review),
  });
}

export async function getReminderSettings(): Promise<ReminderSettings> {
  return httpRequest<ReminderSettings>("/reminder-settings");
}

export async function saveReminderSettings(
  enabled: boolean,
): Promise<ReminderSettings> {
  return httpRequest<ReminderSettings>("/reminder-settings", {
    method: "PUT",
    body: JSON.stringify({ enabled }),
  });
}

export async function getReviewReminders(): Promise<ReviewReminderSummary> {
  return httpRequest<ReviewReminderSummary>("/review-reminders");
}

export async function acknowledgeReviewReminder(
  fingerprint: string,
): Promise<ReviewReminderSummary> {
  return httpRequest<ReviewReminderSummary>("/review-reminders/acknowledge", {
    method: "POST",
    body: JSON.stringify({ fingerprint }),
  });
}

export async function getInvestmentRules(): Promise<InvestmentRule[]> {
  return httpRequest<InvestmentRule[]>("/investment-rules");
}

export async function getRuleEffectiveness(): Promise<RuleEffectivenessSummary> {
  return httpRequest<RuleEffectivenessSummary>("/rule-effectiveness");
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
  review: SystemReviewInput,
): Promise<SystemReviewRecord> {
  return httpRequest<SystemReviewRecord>("/system-reviews", {
    method: "POST",
    body: JSON.stringify(review),
  });
}

export async function getResearchEvidence(): Promise<ResearchEvidence[]> {
  return httpRequest<ResearchEvidence[]>("/research-evidence");
}

export async function saveResearchEvidence(
  evidence: ResearchEvidenceInput,
): Promise<ResearchEvidence> {
  return httpRequest<ResearchEvidence>("/research-evidence", {
    method: "POST",
    body: JSON.stringify(evidence),
  });
}

export async function setResearchEvidenceStatus(
  id: string,
  active: boolean,
): Promise<ResearchEvidence> {
  return httpRequest<ResearchEvidence>(
    `/research-evidence/${encodeURIComponent(id)}/status`,
    { method: "PUT", body: JSON.stringify({ active }) },
  );
}
import { requestJson } from "./lib/transport";

export async function readCodexCredentials(): Promise<ModelConfig> {
  return httpRequest<ModelConfig>("/model-config/codex", { method: "POST" });
}

export async function autoCloudSync(): Promise<
  import("./types").AutoSyncResult
> {
  return httpRequest("/cloud/sync/auto", { method: "POST" });
}
export async function saveAutoSyncSettings(
  enabled: boolean,
): Promise<CloudStatus> {
  return httpRequest("/cloud/sync/settings", {
    method: "PUT",
    body: JSON.stringify({ enabled }),
  });
}

export function ensureDailyAssets(timezone: string) {
  return httpRequest<import("./types").DailyHistory>("/daily-assets/ensure", {
    method: "POST",
    body: JSON.stringify({ timezone }),
  });
}
export function getDailyAssets(
  query: { from?: string; to?: string; before?: string; limit?: number } = {},
) {
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(query))
    if (value != null) params.set(key, String(value));
  return httpRequest<import("./types").DailyHistory>(`/daily-assets?${params}`);
}
export function compareDailyAssets(from: string, to: string) {
  return httpRequest<import("./types").DailyComparison>(
    `/daily-assets/compare?${new URLSearchParams({ from, to })}`,
  );
}
// Serialize quick edits so late responses cannot replace a newer portfolio in React.
let amountQueue: Promise<unknown> = Promise.resolve();
export function updateHoldingAmount(
  id: string,
  amount: number,
  expectedRevision: string,
  requestId: string,
) {
  const result = amountQueue
    .catch(() => undefined)
    .then(() =>
      httpRequest<Snapshot>(`/holdings/${encodeURIComponent(id)}/amount`, {
        method: "PUT",
        body: JSON.stringify({ amount, expectedRevision, requestId }),
      }),
    );
  amountQueue = result;
  return result;
}
