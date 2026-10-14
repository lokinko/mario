import { httpRequest } from "../lib/httpClient";
import type {
  FinancialProfile,
  FxBook,
  Goal,
  Holding,
  SecurityPriceConfig,
  SecurityPriceQuote,
  PortfolioEventInput,
  PortfolioEventImportPreview,
  PortfolioEventImportResult,
  PortfolioEventRecord,
  PortfolioEventReversalInput,
  Snapshot,
} from "../types";
import { serviceUrl } from "../lib/service";
import { webToken } from "../lib/webSession";
// Serialize quick edits so late responses cannot replace a newer portfolio in React.
let amountQueue: Promise<unknown> = Promise.resolve();
export async function getSnapshot(): Promise<Snapshot> {
  return httpRequest<Snapshot>("/snapshot");
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
  input: import("../types").HoldingTradeInput,
): Promise<Snapshot> {
  return httpRequest<Snapshot>("/holding-trades", {
    method: "POST",
    body: JSON.stringify(input),
  });
}

export function updateHoldingTrade(
  id: string,
  input: import("../types").HoldingTradeUpdateInput,
): Promise<Snapshot> {
  return httpRequest<Snapshot>(`/holding-trades/${encodeURIComponent(id)}`, {
    method: "PUT",
    body: JSON.stringify(input),
  });
}

export function deleteHoldingTrade(
  id: string,
  input: import("../types").HoldingTradeDeleteInput,
): Promise<Snapshot> {
  return httpRequest<Snapshot>(`/holding-trades/${encodeURIComponent(id)}`, {
    method: "DELETE",
    body: JSON.stringify(input),
  });
}

export async function getPortfolioEvents(): Promise<PortfolioEventRecord[]> {
  return httpRequest<PortfolioEventRecord[]>("/portfolio-events");
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

export function ensureDailyAssets(timezone: string) {
  return httpRequest<import("../types").DailyHistory>("/daily-assets/ensure", {
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
  return httpRequest<import("../types").DailyHistory>(
    `/daily-assets?${params}`,
  );
}

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
