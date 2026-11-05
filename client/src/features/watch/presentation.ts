import type { WatchIssue, WatchStatus, WatchRow } from "../../api/watch";
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
  non_positive_band: "下轨不为正，无法使用比例区间",
};
export const number = (value: number | null | undefined, suffix = "") =>
  value == null ? "—" : `${formatNumber(value, 2, true)}${suffix}`;
export const distance = (value: number | null | undefined) =>
  value == null ? "—" : `${formatSigned(value)}%`;
export const canonical = (value: string) =>
  value.trim().toUpperCase().replace(/\.SH$/, ".SS");

export function checks(row: WatchRow, minYield: number) {
  return [
    row.latest?.yieldPct == null
      ? null
      : (row.dividendPrices.find((p) => p.yieldPct === minYield)?.reached ??
        (row.latest.yieldPct === 0 ? false : null)),
    row.latest?.daily?.distancePct == null ? null : row.latest.daily.nearLower,
    row.latest?.weekly?.distancePct == null
      ? null
      : row.latest.weekly.nearLower,
  ];
}

export type WatchFilter =
  "all" | "attention" | "yield" | "bands" | "alerts" | "incomplete";
export type WatchSort = "default" | "yield" | "daily" | "weekly" | "name";
export function selectRows(
  rows: WatchRow[],
  query: string,
  filter: WatchFilter,
  sort: WatchSort,
  held: Set<string>,
  ownership: string,
  minYield: number,
) {
  const term = query.trim().toLocaleLowerCase();
  const selected = rows.filter((row) => {
    const values = checks(row, minYield);
    return (
      (!term ||
        `${row.entry.name} ${row.entry.symbol} ${row.entry.thesis}`
          .toLocaleLowerCase()
          .includes(term)) &&
      (ownership === "all" ||
        held.has(row.entry.symbol) === (ownership === "held")) &&
      (filter === "all" ||
        (filter === "attention" && row.latest?.status === "attention") ||
        (filter === "yield" && values[0] === true) ||
        (filter === "bands" && values[1] === true && values[2] === true) ||
        (filter === "alerts" && row.alert) ||
        (filter === "incomplete" &&
          (!row.latest ||
            row.latest.status === "incomplete" ||
            row.refreshFailed)))
    );
  });
  const rank = (r: WatchRow) =>
    r.alert ? 0 : r.latest?.status === "attention" ? 1 : 2;
  const metric = (r: WatchRow) =>
    sort === "yield"
      ? r.latest?.yieldPct
      : Math.abs(
          (sort === "daily" ? r.latest?.daily : r.latest?.weekly)
            ?.distancePct ?? NaN,
        );
  return selected.sort((a, b) => {
    if (sort === "name")
      return a.entry.name.localeCompare(b.entry.name, "zh-CN");
    if (sort === "default")
      return rank(a) - rank(b) || a.entry.symbol.localeCompare(b.entry.symbol);
    const av = metric(a),
      bv = metric(b);
    const knownA = av != null && Number.isFinite(av),
      knownB = bv != null && Number.isFinite(bv);
    if (!knownA || !knownB) return Number(knownB) - Number(knownA);
    return (
      (sort === "yield" ? bv! - av! : av! - bv!) ||
      a.entry.symbol.localeCompare(b.entry.symbol)
    );
  });
}
