import { httpRequest } from "../lib/httpClient";

export interface WatchInput {
  symbol: string;
  name: string;
  thesis: string;
}
export interface WatchEntry extends WatchInput {
  createdAt: string;
  pinned: boolean;
  gridPlan?: WatchGridPlan;
  previousGridPlan?: {
    buyBasis: "budget" | "remainingCash";
    sellBasis: "totalBought" | "remainingShares";
    levels: { yieldBps: number; buyBps: number; sellBps: number }[];
  };
}
export interface WatchGridGenerator {
  startYieldBps: number;
  endYieldBps: number;
  stepBps: number;
  initialBuyShares: number;
  initialSellShares: number;
  buyGrowthBps: number;
  sellGrowthBps: number;
}
export interface WatchGridPlan {
  budget: string;
  generator?: WatchGridGenerator;
  levels: { yieldBps: number; buyShares: number; sellShares: number }[];
}
export interface WatchGridProjection {
  reboundYieldBps: number | null;
  levels: {
    yieldBps: number;
    price: string | null;
    buyAmount: string | null;
    sellAmount: string | null;
  }[];
  scenarios: {
    deepestYieldBps: number;
    investedAmount: string;
    budgetGap: string;
    soldShares: number;
    remainingShares: number;
    unfilledSellShares: number;
    saleProceeds: string;
    profit: string;
    budgetReturnPct: string;
    investedReturnPct: string;
  }[];
}
export type BandReference = "lower" | "middle" | "upper";
export interface BandRule {
  reference: BandReference;
  mode: "near" | "at_or_below" | "at_or_above";
  offsetBps: number;
}
export interface WatchRules {
  minYieldBps: number;
  daily: BandRule;
  weekly: BandRule;
}
export interface Band {
  lower: number;
  middle: number;
  upper: number;
  distancePct: number | null;
  reference: BandReference;
  matchesRule: boolean;
}
export type WatchStatus = "attention" | "waiting" | "incomplete";
export type WatchIssue =
  | "price_unavailable"
  | "daily_history_incomplete"
  | "weekly_history_incomplete"
  | "dividend_unavailable"
  | "non_positive_band";
export interface Observation {
  rules: WatchRules;
  yieldMatchesRule: boolean | null;
  symbol: string;
  observedOn: string;
  checkedAt: string;
  price: number | null;
  dividendPerShare: string | null;
  yieldPct: number | null;
  daily: Band | null;
  weekly: Band | null;
  status: WatchStatus;
  issues: WatchIssue[];
  sources: string[];
  dividendEvents: {
    reportDate?: string;
    exDate: string;
    cashPerTen: string;
    bonusPerTen: string;
    transferPerTen: string;
  }[];
  dividendBasis?: { asOf: string; fetchedAt: string } | null;
  dividendHistory?: {
    start: string;
    end: string;
    events: Observation["dividendEvents"];
  } | null;
}
export interface WatchMarketRow {
  latest: Observation | null;
  dividendPrices: DividendPrice[];
  checkedAt: string | null;
  refreshFailed: boolean;
}
export interface WatchRow extends WatchMarketRow {
  entry: WatchEntry;
  gridProjection?: WatchGridProjection | null;
  alert: boolean;
}
export interface CatalogCompany {
  symbol: string;
  name: string;
  sources: string[];
  sourcesCheckedOn?: string;
}
export interface WatchIndustry {
  id: string;
  name: string;
  companies: CatalogCompany[];
}
export interface CatalogRow extends WatchMarketRow {
  company: CatalogCompany;
  industryId: string;
  checkedFor: string | null;
}
export interface CatalogBoard {
  revision: number;
  canEdit: boolean;
  catalog: { version: number; verifiedOn: string; industries: WatchIndustry[] };
  rows: CatalogRow[];
  rules: WatchRules;
}
export interface DividendPrice {
  yieldPct: number;
  price: string;
  reached: boolean | null;
  changePct: number | null;
}
export interface WatchBoard {
  defaultRules: WatchRules;
  rows: WatchRow[];
  weekly: Observation[];
  weekStart: string;
  weekEnd: string;
  rules: WatchRules;
}

export const getWatchBoard = () => httpRequest<WatchBoard>("/watch");
export const getWatchCatalog = () =>
  httpRequest<CatalogBoard>("/watch/catalog");
export const resolveCatalogCompany = (query: string) =>
  httpRequest<CatalogCompany>("/watch/catalog/resolve", {
    method: "POST",
    body: JSON.stringify({ query }),
  });
export const saveWatchCatalog = (
  catalog: CatalogBoard["catalog"],
  revision: number,
) =>
  httpRequest<CatalogBoard>("/watch/catalog", {
    method: "PUT",
    body: JSON.stringify({ catalog, revision }),
  });
export const refreshWatchCatalog = (industryId: string) =>
  httpRequest<CatalogBoard>("/watch/catalog/refresh", {
    method: "POST",
    body: JSON.stringify({ industryId }),
  });
export const saveWatch = (input: WatchInput, ifAbsent = false) =>
  httpRequest<WatchEntry>("/watch", {
    method: "PUT",
    body: JSON.stringify({ ...input, ...(ifAbsent ? { ifAbsent: true } : {}) }),
  });
export const deleteWatch = (symbol: string) =>
  httpRequest<{ ok: boolean }>(`/watch/${encodeURIComponent(symbol)}`, {
    method: "DELETE",
  });
export const setWatchPinned = (symbol: string, pinned: boolean) =>
  httpRequest<WatchEntry>(`/watch/${encodeURIComponent(symbol)}/pin`, {
    method: "PUT",
    body: JSON.stringify({ pinned }),
  });
export const saveWatchGrid = (symbol: string, plan: WatchGridPlan | null) =>
  httpRequest<WatchBoard>(`/watch/${encodeURIComponent(symbol)}/grid`, {
    method: "PUT",
    body: JSON.stringify(plan),
  });
export const readWatchAlerts = (symbol: string) =>
  httpRequest<{ ok: boolean }>(`/watch/${encodeURIComponent(symbol)}/read`, {
    method: "POST",
  });
export const refreshWatch = () =>
  httpRequest<WatchBoard>("/watch/refresh", {
    method: "POST",
  });

export const saveWatchRules = (rules: WatchRules, expectedRules: WatchRules) =>
  httpRequest<WatchBoard>("/watch/rules", {
    method: "PUT",
    body: JSON.stringify({ rules, expectedRules }),
  });

export type DividendCalendarScope = "all" | "favorites" | "public";
export type DividendPlanStage =
  | "preannounced"
  | "proposed"
  | "approved"
  | "scheduled"
  | "cancelled"
  | "no_distribution"
  | "unknown";
export interface DividendPlan {
  id: string;
  symbol: string;
  reportDate: string;
  announcedOn: string;
  stage: DividendPlanStage;
  sourceProgress: string;
  profile: string;
  cashPerTen: string | null;
  bonusPerTen: string | null;
  transferPerTen: string | null;
  recordDate: string | null;
  exDate: string | null;
  paymentDate: string | null;
  issues: ("invalid_dates" | "invalid_amount" | "unknown_progress")[];
  sources: string[];
}
export interface DividendCalendarCompany {
  symbol: string;
  name: string;
  personal: boolean;
}
export interface DividendCalendarEntry extends DividendCalendarCompany {
  plan: DividendPlan;
  fetchedAt: string | null;
}
export interface DividendCalendarBoard {
  start: string;
  end: string;
  asOf: string;
  scope: DividendCalendarScope;
  entries: DividendCalendarEntry[];
  pending: DividendCalendarEntry[];
  coverage: (DividendCalendarCompany & {
    attemptedAt: string | null;
    fetchedAt: string | null;
    refreshFailed: boolean;
  })[];
}
function dividendCalendarQuery(scope: DividendCalendarScope, from: string) {
  return new URLSearchParams({ scope, ...(from ? { from } : {}) }).toString();
}
export const getDividendCalendar = (
  scope: DividendCalendarScope = "all",
  from = "",
) =>
  httpRequest<DividendCalendarBoard>(
    `/watch/dividend-calendar?${dividendCalendarQuery(scope, from)}`,
  );
export const refreshDividendCalendar = (
  scope: DividendCalendarScope,
  from: string,
  symbols?: string[],
) =>
  httpRequest<DividendCalendarBoard>(
    `/watch/dividend-calendar/refresh?${dividendCalendarQuery(scope, from)}`,
    {
      method: "POST",
      body: JSON.stringify(symbols ? { symbols } : {}),
    },
  );
