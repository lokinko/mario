import { httpRequest } from "../lib/httpClient";

export interface WatchInput {
  symbol: string;
  name: string;
  thesis: string;
}
export interface WatchEntry extends WatchInput {
  createdAt: string;
}
export interface Band {
  lower: number;
  middle: number;
  upper: number;
  distancePct: number | null;
  nearLower: boolean;
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
    exDate: string;
    cashPerTen: string;
    bonusPerTen: string;
    transferPerTen: string;
  }[];
}
export interface WatchRow {
  entry: WatchEntry;
  latest: Observation | null;
  alert: boolean;
  checkedAt: string | null;
  refreshFailed: boolean;
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
export const saveWatch = (input: WatchInput) =>
  httpRequest<WatchEntry>("/watch", {
    method: "PUT",
    body: JSON.stringify(input),
  });
export const deleteWatch = (symbol: string) =>
  httpRequest<{ ok: boolean }>(`/watch/${encodeURIComponent(symbol)}`, {
    method: "DELETE",
  });
export const readWatchAlerts = (symbol: string) =>
  httpRequest<{ ok: boolean }>(`/watch/${encodeURIComponent(symbol)}/read`, {
    method: "POST",
  });
export const refreshWatch = () =>
  httpRequest<WatchBoard>("/watch/refresh", {
    method: "POST",
  });
