import { httpRequest } from "../lib/httpClient";
import type {
  DailyReviewFeedQuery,
  DailyReviewFeedResponse,
  DailyReviewReadResult,
  DailyReviewRunResult,
  DailyReviewSettings,
} from "../types/ai";

export async function getDailyReviewFeed(
  query: DailyReviewFeedQuery = {},
): Promise<DailyReviewFeedResponse> {
  const params = new URLSearchParams();
  if (query.day) params.set("day", query.day);
  if (query.kind) params.set("kind", query.kind);
  if (query.symbol) params.set("symbol", query.symbol);
  if (query.unreadOnly) params.set("unreadOnly", "true");
  if (query.before) params.set("before", query.before);
  if (query.limit) params.set("limit", String(query.limit));
  const qs = params.toString();
  return httpRequest<DailyReviewFeedResponse>(
    qs ? `/daily-review?${qs}` : "/daily-review",
  );
}

export async function saveDailyReviewSettings(
  settings: DailyReviewSettings,
): Promise<DailyReviewSettings> {
  return httpRequest<DailyReviewSettings>("/daily-review/settings", {
    method: "PUT",
    body: JSON.stringify(settings),
  });
}

export async function runDailyReviewNow(
  reviewDay?: string,
): Promise<DailyReviewRunResult> {
  return httpRequest<DailyReviewRunResult>("/daily-review/run", {
    method: "POST",
    body: JSON.stringify(reviewDay ? { reviewDay } : {}),
  });
}

export async function markDailyReviewRead(input: {
  id?: string;
  all?: boolean;
}): Promise<DailyReviewReadResult> {
  return httpRequest<DailyReviewReadResult>("/daily-review/read", {
    method: "POST",
    body: JSON.stringify(input),
  });
}
