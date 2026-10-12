import {
  beforeDataWrite,
  markDataWrite,
  isDataWrite,
  DATA_SAVED,
} from "./lib/syncEvents";
import {
  serviceUrl,
  dataRevision,
  acceptRevision,
  isLocalDev,
} from "./lib/service";
import { HttpError } from "./lib/transport";
import { webToken } from "./lib/webSession";
import type {
  AnalysisRequest,
  AnalysisSettings,
  AnalysisPreview,
  AnalysisResult,
  AnalysisHistoryItem,
  FinancialProfile,
  FxRateQuote,
  FxBook,
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
  PortfolioEventInput,
  PortfolioEventImportPreview,
  PortfolioEventImportResult,
  PortfolioEventRecord,
  PortfolioEventReversalInput,
  ResearchEvidence,
  ResearchEvidenceInput,
  ReviewReminderSummary,
  Snapshot,
  SystemReviewDraft,
  SystemReviewRecord,
  StoredAnalysis,
} from "./types";

async function httpRequest<T>(
  path: string,
  init?: RequestInit,
  account = { token: webToken(), baseUrl: serviceUrl() },
): Promise<T> {
  const dataWrite = isDataWrite(path, init?.method);
  const release = dataWrite ? markDataWrite() : () => {};
  const { token: authToken, baseUrl } = account;
  const ensureCurrentAccount = () => {
    if (webToken() !== authToken || serviceUrl() !== baseUrl)
      throw new Error("账号已切换，请在当前账号中重新操作。");
  };
  try {
    if (dataWrite) await beforeDataWrite();
    ensureCurrentAccount();
    const baseRevision = dataRevision();
    const result = await requestJson<T>(`${baseUrl}${path}`, {
      ...init,
      timeoutMs:
        path === "/analysis" ||
        path === "/model-config/test" ||
        path === "/model-config/codex" ||
        path === "/portfolio-events/import/preview" ||
        path === "/portfolio-events/import/commit"
          ? 300000
          : path === "/snapshot" ||
              path.startsWith("/fx-settings") ||
              path.startsWith("/market-data/fx-rate") ||
              path === "/portfolio-events"
            ? 30000
            : path.startsWith("/research-evidence") || path.startsWith("/data/")
              ? 60000
              : 15000,
      onResponse: (response) => {
        if (webToken() !== authToken || serviceUrl() !== baseUrl) return;
        if (
          path === "/snapshot" ||
          path === "/fx-settings" ||
          (dataWrite && path !== "/analysis")
        ) {
          acceptRevision(
            response.headers.get("x-data-revision") ?? baseRevision,
          );
        }
      },
      headers: {
        "Content-Type": "application/json",
        ...(baseRevision ? { "If-Match": baseRevision } : {}),
        ...(authToken ? { Authorization: `Bearer ${authToken}` } : {}),
        ...(init?.headers ?? {}),
      },
    });
    ensureCurrentAccount();
    if (dataWrite) window.dispatchEvent(new Event(DATA_SAVED));

    return result;
  } catch (error) {
    if (error instanceof HttpError && error.status === 401) {
      const currentToken = webToken();
      if (!currentToken || currentToken === authToken)
        window.dispatchEvent(new Event("mario:session-expired"));
    }
    throw error;
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

export function refreshSecurityPrices(): Promise<{
  snapshot: Snapshot;
  updatedCount: number;
  errors: string[];
}> {
  return httpRequest("/market-data/security/refresh", { method: "POST" });
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

export function saveHoldingTrade(
  input: import("./types").HoldingTradeInput,
): Promise<Snapshot> {
  return httpRequest<Snapshot>("/holding-trades", {
    method: "POST",
    body: JSON.stringify(input),
  });
}

export function updateHoldingTrade(
  id: string,
  input: import("./types").HoldingTradeUpdateInput,
): Promise<Snapshot> {
  return httpRequest<Snapshot>(`/holding-trades/${encodeURIComponent(id)}`, {
    method: "PUT",
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

export async function getAnalysisSettings(): Promise<AnalysisSettings> {
  return httpRequest<AnalysisSettings>("/analysis-settings");
}

export async function saveAnalysisSettings(
  settings: AnalysisSettings,
): Promise<AnalysisSettings> {
  return httpRequest<AnalysisSettings>("/analysis-settings", {
    method: "PUT",
    body: JSON.stringify(settings),
  });
}

export async function getReviewReminders(): Promise<ReviewReminderSummary> {
  return httpRequest<ReviewReminderSummary>("/review-reminders");
}

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

export async function getResearchAttachment(
  evidenceId: string,
  attachmentId: string,
): Promise<{ name: string; dataBase64: string }> {
  return httpRequest(
    `/research-evidence/${encodeURIComponent(evidenceId)}/attachments/${encodeURIComponent(attachmentId)}`,
  );
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

export async function checkServerVersion(): Promise<
  import("./types").AutoSyncResult
> {
  if (isLocalDev()) return { state: "disabled" };
  const baseline = dataRevision();
  const result = await httpRequest<{ revision: string }>("/sync/version");
  return {
    state: "synced",
    sync: { localUpdated: baseline !== result.revision },
  };
}
export function exportData() {
  return httpRequest<unknown>("/data/export");
}
export function importData(data: unknown) {
  return httpRequest("/data/import", {
    method: "POST",
    body: JSON.stringify(data),
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
  const account = { token: webToken(), baseUrl: serviceUrl() };
  const result = amountQueue
    .catch(() => undefined)
    .then(() =>
      httpRequest<Snapshot>(
        `/holdings/${encodeURIComponent(id)}/amount`,
        {
          method: "PUT",
          body: JSON.stringify({ amount, expectedRevision, requestId }),
        },
        account,
      ),
    );
  amountQueue = result;
  return result;
}

export function getFxSettings(): Promise<FxBook> {
  return httpRequest<FxBook>("/fx-settings");
}
export function refreshFxSettings(): Promise<FxBook> {
  return httpRequest<FxBook>("/fx-settings/refresh", { method: "POST" });
}
export function setFxOverride(
  currency: string,
  rate: number | null,
): Promise<FxBook> {
  return httpRequest<FxBook>("/fx-settings", {
    method: "PUT",
    body: JSON.stringify({ currency, rate }),
  });
}
