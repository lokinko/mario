import type {
  WatchIssue,
  WatchStatus,
  WatchRow,
  WatchMarketRow,
} from "../../api/watch";
import { formatNumber, formatSigned } from "../../lib/format";

export const statuses: Record<WatchStatus, string> = {
  attention: "进入关注区间",
  waiting: "继续等待",
  incomplete: "待补齐数据",
};
export const issueLabels: Record<WatchIssue, string> = {
  price_unavailable: "该交易日收盘价缺失",
  daily_history_incomplete: "日线历史不足或存在缺口",
  weekly_history_incomplete: "周线历史不足或存在缺口",
  dividend_unavailable: "已实施分红数据暂不可用",
  non_positive_band: "所选参考轨不为正，无法计算观察条件",
};
export const number = (value: number | null | undefined, suffix = "") =>
  value == null ? "—" : `${formatNumber(value, 2, true)}${suffix}`;
export const distance = (value: number | null | undefined) =>
  value == null ? "—" : `${formatSigned(value)}%`;
export const canonical = (value: string) =>
  value.trim().toUpperCase().replace(/\.SH$/, ".SS");

export interface WatchDisplayRow extends WatchMarketRow {
  company: { symbol: string; name: string };
  watch?: WatchRow;
}

export const displayWatch = (watch: WatchRow): WatchDisplayRow => ({
  latest: watch.latest,
  dividendPrices: watch.dividendPrices,
  checkedAt: watch.checkedAt,
  refreshFailed: watch.refreshFailed,
  company: watch.entry,
  watch,
});

export function checks(row: WatchMarketRow) {
  return [
    row.latest?.yieldMatchesRule ?? null,
    row.latest?.daily?.distancePct == null
      ? null
      : row.latest.daily.matchesRule,
    row.latest?.weekly?.distancePct == null
      ? null
      : row.latest.weekly.matchesRule,
  ];
}

export const sortOptions = {
  default: "变化与达标优先",
  yield: "股息率从高到低",
  "yield-asc": "股息率从低到高",
  "price-asc": "收盘价从低到高",
  "price-desc": "收盘价从高到低",
  daily: "距日线参考轨从近到远",
  "daily-far": "距日线参考轨从远到近",
  weekly: "距周线参考轨从近到远",
  "weekly-far": "距周线参考轨从远到近",
  name: "公司名称",
} as const;
export type WatchSort = keyof typeof sortOptions;

type MetricSort = Exclude<WatchSort, "default" | "name">;
const metrics: Record<
  MetricSort,
  {
    value: (row: WatchDisplayRow) => number | null | undefined;
    direction: 1 | -1;
  }
> = {
  yield: { value: (r) => r.latest?.yieldPct, direction: -1 },
  "yield-asc": { value: (r) => r.latest?.yieldPct, direction: 1 },
  "price-asc": { value: (r) => r.latest?.price, direction: 1 },
  "price-desc": { value: (r) => r.latest?.price, direction: -1 },
  daily: { value: dailyDistance, direction: 1 },
  "daily-far": { value: dailyDistance, direction: -1 },
  weekly: { value: weeklyDistance, direction: 1 },
  "weekly-far": { value: weeklyDistance, direction: -1 },
};

function dailyDistance(row: WatchDisplayRow) {
  return Math.abs(row.latest?.daily?.distancePct ?? NaN);
}

function weeklyDistance(row: WatchDisplayRow) {
  return Math.abs(row.latest?.weekly?.distancePct ?? NaN);
}

function compareMetric(
  a: number | null | undefined,
  b: number | null | undefined,
  direction: 1 | -1,
) {
  if (a == null || !Number.isFinite(a))
    return b == null || !Number.isFinite(b) ? 0 : 1;
  if (b == null || !Number.isFinite(b)) return -1;
  return direction * (a - b);
}

export function selectRows(
  rows: WatchDisplayRow[],
  query: string,
  sort: WatchSort,
  held: Set<string>,
  ownership: string,
) {
  const term = query.trim().toLocaleLowerCase();
  const selected = rows.filter(
    (row) =>
      (!term ||
        `${row.company.name} ${row.company.symbol} ${row.watch?.entry.thesis ?? ""}`
          .toLocaleLowerCase()
          .includes(term)) &&
      (ownership === "all" ||
        held.has(row.company.symbol) === (ownership === "held")),
  );
  const rank = (r: WatchDisplayRow) =>
    r.watch?.alert ? -4 : -checks(r).filter((met) => met === true).length;
  return selected.sort((a, b) => {
    const pinned =
      Number(Boolean(b.watch?.entry.pinned)) -
      Number(Boolean(a.watch?.entry.pinned));
    if (pinned) return pinned;
    const symbol = a.company.symbol.localeCompare(b.company.symbol);
    if (sort === "name")
      return a.company.name.localeCompare(b.company.name, "zh-CN") || symbol;
    if (sort === "default") return rank(a) - rank(b) || symbol;
    const { value, direction } = metrics[sort];
    return compareMetric(value(a), value(b), direction) || symbol;
  });
}
