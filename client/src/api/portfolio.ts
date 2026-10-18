import { financialRequest } from "./portfolio/financialRequest";
import { httpRequest } from "../lib/httpClient";
import type {
  FinancialProfile,
  FxBook,
  Holding,
  SecurityPriceConfig,
  SecurityPriceQuote,
  Snapshot,
} from "../types";
import { serviceUrl } from "../lib/service";
import { webToken } from "../lib/webSession";
// Serialize quick edits so late responses cannot replace a newer portfolio in React.
let amountQueue: Promise<unknown> = Promise.resolve();
export async function getSnapshot(): Promise<Snapshot> {
  return httpRequest<Snapshot>("/snapshot");
}

export function updateOptionPrice(
  id: string,
  input: {
    requestId: string;
    expectedRevision: string;
    price: string;
    observedOn: string;
  },
): Promise<Snapshot> {
  return financialRequest(`/holdings/${encodeURIComponent(id)}/option-price`, {
    method: "PUT",
    body: JSON.stringify(input),
  });
}

export function getFinancialHistory(id: string): Promise<{
  records: {
    id: string;
    kind: string;
    before: string;
    after: string;
    cashDelta: string;
    operationId: string;
    reason: string;
    createdAt: string;
  }[];
  quarantine: { payload: string; reason: string; createdAt: string } | null;
}> {
  return httpRequest(`/holdings/${encodeURIComponent(id)}/financial-history`);
}
type RepairInput = {
  expectedRevision: string;
  trades: import("../types").HoldingTrade[];
  reason: string;
};
export function previewTradeRepair(
  id: string,
  input: RepairInput,
): Promise<import("../types").HoldingTradeSummary> {
  return httpRequest(
    `/holdings/${encodeURIComponent(id)}/trade-repair/preview`,
    { method: "POST", body: JSON.stringify(input) },
  );
}
export function repairTrades(
  id: string,
  input: RepairInput,
): Promise<Snapshot> {
  return financialRequest(`/holdings/${encodeURIComponent(id)}/trade-repair`, {
    method: "POST",
    body: JSON.stringify(input),
  });
}

export function confirmCorporateAction(
  id: string,
  expectedRevision: string,
  reason: string,
  mode: "confirm" | "reject" | "revert" = "confirm",
): Promise<Snapshot> {
  return financialRequest(
    `/corporate-actions/${encodeURIComponent(id)}/${mode}`,
    {
      method: "POST",
      body: JSON.stringify({ expectedRevision, reason }),
    },
  );
}

export function declareCorporateAction(
  id: string,
  input: {
    requestId: string;
    expectedRevision: string;
    split: { effectiveOn: string; numerator: string; denominator: string };
    reason: string;
  },
): Promise<Snapshot> {
  return financialRequest(
    `/holdings/${encodeURIComponent(id)}/corporate-actions`,
    {
      method: "POST",
      body: JSON.stringify(input),
    },
  );
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

export async function deleteHolding(
  id: string,
  expectedRevision: string,
): Promise<Snapshot> {
  return financialRequest(`/holdings/${encodeURIComponent(id)}`, {
    method: "DELETE",
    body: JSON.stringify({ expectedRevision }),
  });
}

export function saveHoldingTrade(
  input: import("../types").HoldingTradeInput,
): Promise<Snapshot> {
  return financialRequest("/holding-trades", {
    method: "POST",
    body: JSON.stringify(input),
  });
}

export function updateHoldingTrade(
  id: string,
  input: import("../types").HoldingTradeUpdateInput,
): Promise<Snapshot> {
  return financialRequest(`/holding-trades/${encodeURIComponent(id)}`, {
    method: "PUT",
    body: JSON.stringify(input),
  });
}

export function deleteHoldingTrade(
  id: string,
  input: import("../types").HoldingTradeDeleteInput,
): Promise<Snapshot> {
  return financialRequest(`/holding-trades/${encodeURIComponent(id)}`, {
    method: "DELETE",
    body: JSON.stringify(input),
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
  amount: number | string,
  expectedRevision: string,
  requestId: string,
) {
  const account = { token: webToken(), baseUrl: serviceUrl() };
  const result = amountQueue
    .catch(() => undefined)
    .then(() =>
      financialRequest(
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
  rate: number | string | null,
): Promise<FxBook> {
  return httpRequest<FxBook>("/fx-settings", {
    method: "PUT",
    body: JSON.stringify({
      currency,
      rate: rate == null ? null : Number(rate),
      ...(typeof rate === "string" ? { exactRate: rate } : {}),
    }),
  });
}
