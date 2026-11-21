import {
  ArrowDown,
  ArrowUp,
  ArrowUpDown,
  Bell,
  Pin,
  PinOff,
  Star,
} from "lucide-react";
import type { WatchRules, WatchRow } from "../../api/watch";
import { referenceLabels } from "./rules";
import { Badge } from "../../components/Badge";
import { formatAmount } from "../../lib/format";
import {
  checks,
  distance,
  issueLabels,
  number,
  statuses,
  sortOptions,
  type WatchSort,
  type WatchDisplayRow,
} from "./presentation";

type RuleKey = "yield" | "daily" | "weekly";
const ruleMarkers: Record<RuleKey, string> = {
  yield: "①",
  daily: "②",
  weekly: "③",
};
const ruleMarker = (rule: RuleKey) => (
  <span className="watch-rule-marker" aria-hidden="true">
    {ruleMarkers[rule]}
  </span>
);

export function WatchList({
  rows,
  held,
  busy,
  rules,
  sort,
  onSort,
  onPin,
  onDetails,
  onRead,
  onRetry,
  favorites,
  canAdd,
  onAdd,
}: {
  rows: WatchDisplayRow[];
  held: Set<string>;
  busy: boolean;
  rules: WatchRules;
  sort: WatchSort;
  onSort: (sort: WatchSort) => void;
  onPin: (row: WatchRow) => void;
  onDetails: (row: WatchDisplayRow) => void;
  onRead: (row: WatchRow) => void;
  onRetry: () => void;
  favorites: Set<string>;
  canAdd: boolean;
  onAdd: (row: WatchDisplayRow) => void;
}) {
  const minYield = rules.minYieldBps / 100;
  const state = (row: WatchDisplayRow) => (
    <Badge tone={row.latest?.status === "attention" ? "success" : "neutral"}>
      {row.latest ? statuses[row.latest.status] : "等待首次检查"}
    </Badge>
  );
  const issue = (row: WatchDisplayRow) =>
    (row.refreshFailed || Boolean(row.latest?.issues.length)) && (
      <div className="watch-data-issue">
        <span>
          {row.refreshFailed
            ? "本次来源读取失败，保留上次结果与日期。"
            : row.latest?.issues.map((i) => issueLabels[i]).join("；")}
        </span>
        {row.watch && (
          <button className="text-button" disabled={busy} onClick={onRetry}>
            重新检查数据
          </button>
        )}
      </div>
    );
  const alert = (row: WatchDisplayRow) =>
    row.watch?.alert && (
      <button
        className="watch-alert-button"
        disabled={busy}
        onClick={() => row.watch && onRead(row.watch)}
      >
        <Bell size={13} />
        标为已读
      </button>
    );
  const pin = (row: WatchDisplayRow) => {
    if (!row.watch) return null;
    const pinned = row.watch.entry.pinned;
    const Icon = pinned ? PinOff : Pin;
    const label = pinned ? "取消置顶" : "置顶";
    return (
      <button
        className={`watch-pin-toggle ${pinned ? "pinned" : ""}`}
        disabled={busy}
        onClick={() => row.watch && onPin(row.watch)}
        aria-label={`${label}${row.company.name}`}
        title={label}
      >
        <Icon size={14} aria-hidden="true" />
      </button>
    );
  };
  const add = (row: WatchDisplayRow) => {
    if (row.company.symbol.endsWith(".BJ")) return null;
    const saved = favorites.has(row.company.symbol);
    return (
      <button
        className={`watch-favorite-toggle ${saved ? "saved" : ""}`}
        disabled={busy || saved || !canAdd}
        aria-label={`${saved ? "已在自选" : "加入自选"}${row.company.name}`}
        title={saved ? "已在自选" : "加入自选"}
        onClick={() => onAdd(row)}
      >
        <Star
          size={16}
          fill={saved ? "currentColor" : "none"}
          aria-hidden="true"
        />
      </button>
    );
  };
  const sortHeader = (
    label: string,
    first: WatchSort,
    second: WatchSort,
    firstDirection: "ascending" | "descending",
    rule?: RuleKey,
    absolute = false,
  ) => {
    const direction =
      sort === first
        ? firstDirection
        : sort === second
          ? firstDirection === "ascending"
            ? "descending"
            : "ascending"
          : undefined;
    const Icon =
      direction === "ascending"
        ? ArrowUp
        : direction === "descending"
          ? ArrowDown
          : ArrowUpDown;
    const next = sort === first ? second : first;
    return (
      <th
        scope="col"
        className={rule && `rule-${rule}`}
        aria-sort={direction && absolute ? "other" : direction}
      >
        <button
          className="watch-sort-header"
          onClick={() => onSort(next)}
          aria-label={`${label}：${direction ? `当前${sortOptions[sort]}，` : ""}点击按${sortOptions[next]}`}
          title={sortOptions[next]}
        >
          {rule && ruleMarker(rule)}
          {label}
          <Icon size={13} aria-hidden="true" />
        </button>
      </th>
    );
  };
  return (
    <div className="panel watch-overview table-scroll">
      <table aria-label="观察公司对比">
        <thead>
          <tr>
            <th>公司 / 行情日</th>
            {sortHeader("收盘价", "price-asc", "price-desc", "ascending")}
            {sortHeader(
              "TTM 股息率",
              "yield",
              "yield-asc",
              "descending",
              "yield",
            )}
            <th scope="col" className="rule-yield">
              {ruleMarker("yield")}
              {minYield}% 参考价
            </th>
            {sortHeader(
              `距日线${referenceLabels[rules.daily.reference]}`,
              "daily",
              "daily-far",
              "ascending",
              "daily",
              true,
            )}
            {sortHeader(
              `距周线${referenceLabels[rules.weekly.reference]}`,
              "weekly",
              "weekly-far",
              "ascending",
              "weekly",
              true,
            )}
            <th>观察状态</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr
              key={row.company.symbol}
              className={row.watch?.alert ? "watch-unread" : ""}
            >
              <td data-label="公司">
                <div className="watch-company-line">
                  <button
                    className="watch-company"
                    onClick={() => onDetails(row)}
                  >
                    <strong>{row.company.name}</strong>
                    <small>
                      {row.company.symbol} ·{" "}
                      {held.has(row.company.symbol) ? "已持有" : "未持有"}
                    </small>
                  </button>
                  {pin(row)}
                </div>
                <small className="watch-row-date">
                  {row.latest?.observedOn ?? "尚无行情"}
                </small>
              </td>
              <td data-label="收盘价" className="watch-numeric">
                {number(row.latest?.price)}
              </td>
              <td
                data-label="TTM 股息率"
                className={`watch-numeric rule-yield ${checks(row)[0] === true ? "watch-met" : ""}`}
              >
                <span className="watch-value">
                  {number(row.latest?.yieldPct, "%")}
                </span>
              </td>
              <td
                data-label={`${minYield}% 参考价`}
                className="watch-numeric rule-yield"
              >
                {row.dividendPrices.find((p) => p.yieldPct === minYield)
                  ?.price != null
                  ? formatAmount(
                      row.dividendPrices.find((p) => p.yieldPct === minYield)!
                        .price,
                    )
                  : "—"}
              </td>
              <td
                data-label={`距日线${referenceLabels[rules.daily.reference]}`}
                className={`watch-numeric rule-daily ${row.latest?.daily?.matchesRule ? "watch-met" : ""}`}
              >
                <span className="watch-value">
                  {distance(row.latest?.daily?.distancePct)}
                </span>
              </td>
              <td
                data-label={`距周线${referenceLabels[rules.weekly.reference]}`}
                className={`watch-numeric rule-weekly ${row.latest?.weekly?.matchesRule ? "watch-met" : ""}`}
              >
                <span className="watch-value">
                  {distance(row.latest?.weekly?.distancePct)}
                </span>
              </td>
              <td data-label="状态">
                <div className="watch-row-status">
                  {state(row)}
                  {alert(row)}
                  {!row.watch && add(row)}
                </div>
                {issue(row)}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
