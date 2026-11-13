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
export interface UsageDashboard {
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
