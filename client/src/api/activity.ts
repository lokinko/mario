import { httpRequest } from "../lib/httpClient";

export type ActivityPage =
  | "watch"
  | "calendar"
  | "facts"
  | "evidence"
  | "review"
  | "advisor"
  | "methodology"
  | "settings";
export interface PriceCollectionDay {
  date: string;
  expected: number;
  successful: number;
  failed: number;
  pending: number;
  lagging: number;
  updatedAt: number;
}
export interface PriceCollectionDashboard {
  generatedAt: number;
  years: number;
  targetThrough: string;
  universe: {
    asOf: string;
    fetchedAt: number;
    sourceUrl: string;
    members: { symbol: string; name: string }[];
  } | null;
  universeAttemptedAt: number | null;
  universeError: string;
  summary: PriceCollectionDay;
  rows: {
    symbol: string;
    name: string;
    status: string;
    issueCodes: string[];
    error: string;
    attemptedAt: number | null;
    succeededAt: number | null;
    firstOn: string | null;
    latestOn: string | null;
    bars: number;
    provider: string;
  }[];
  daily: PriceCollectionDay[];
}
export interface UsageDashboard {
  marketPrices: PriceCollectionDashboard;
  generatedAt: number;
  trackingSince: number;
  days: number;
  totalUsers: number;
  dailyActive: number;
  weeklyActive: number;
  monthlyActive: number;
  daily: { date: string; activeUsers: number; activeMinutes: number }[];
  users: {
    userId: string;
    email: string;
    lastSeen: number | null;
    activeDays: number;
    activeMinutes: number;
  }[];
  pages: { page: ActivityPage; activeUsers: number; activeMinutes: number }[];
}
export const recordActivity = (page: ActivityPage, signal: AbortSignal) =>
  httpRequest("/activity", {
    method: "POST",
    body: JSON.stringify({ page }),
    signal,
  });
export const getUsageDashboard = (days: number) =>
  httpRequest<UsageDashboard>(`/dashboard?days=${days}`);
