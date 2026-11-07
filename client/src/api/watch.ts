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
}
export interface WatchGridPlan {
  buyBasis: "budget" | "remainingCash";
  sellBasis: "totalBought" | "remainingShares";
  levels: { yieldBps: number; buyBps: number; sellBps: number }[];
}
export interface WatchGridProjection {
  exitPrice: string | null;
  levels: { yieldBps: number; price: string | null }[];
  scenarios: {
    deepestYieldBps: number;
    investedPct: string;
    exitSharesPct: string;
    budgetReturnPct: string;
    investedReturnPct: string;
  }[];
}
export interface Band {
  lower: number;
  middle: number;
  upper: number;
  distancePct: number | null;
  reference: "lower" | "middle";
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
  minYieldPct: number;
  bandDistancePct: number;
}
export interface DividendPrice {
  yieldPct: number;
  price: string;
  reached: boolean | null;
  changePct: number | null;
}
export interface WatchBoard {
  rows: WatchRow[];
  weekly: Observation[];
  weekStart: string;
  weekEnd: string;
  minYieldPct: number;
  bandDistancePct: number;
}

export const getWatchBoard = () => httpRequest<WatchBoard>("/watch");
export const getWatchCatalog = () =>
  httpRequest<CatalogBoard>("/watch/catalog");
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
