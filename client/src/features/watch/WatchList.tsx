import {
  ArrowDown,
  ArrowUp,
  ArrowUpDown,
  Bell,
  MessageCircle,
  Pin,
  PinOff,
} from "lucide-react";
import type { WatchRow } from "../../api/watch";
import { Badge } from "../../components/Badge";
import { formatAmount } from "../../lib/format";
import { BandTrack, RuleChecks, YieldGrid } from "./WatchSignals";
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

export function WatchList({
  rows,
  view,
  held,
  busy,
  minYield,
  tolerance,
  sort,
  onSort,
  onPin,
  onDetails,
  onAsk,
  onEdit,
  onRead,
  onRetry,
  favorites,
  canAdd,
  onAdd,
}: {
  rows: WatchDisplayRow[];
  view: "compact" | "cards";
  held: Set<string>;
  busy: boolean;
  minYield: number;
  tolerance: number;
  sort: WatchSort;
  onSort: (sort: WatchSort) => void;
  onPin: (row: WatchRow) => void;
  onDetails: (row: WatchDisplayRow) => void;
  onAsk: (row: WatchRow) => void;
  onEdit: (row: WatchRow) => void;
  onRead: (row: WatchRow) => void;
  onRetry: () => void;
  favorites: Set<string>;
  canAdd: boolean;
  onAdd: (row: WatchDisplayRow) => void;
}) {
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
        <button className="text-button" disabled={busy} onClick={onRetry}>
          重新检查数据
        </button>
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
  const pin = (row: WatchDisplayRow, iconOnly = false) => {
    if (!row.watch) return null;
    const pinned = row.watch?.entry.pinned;
    const Icon = pinned ? PinOff : Pin;
    const label = pinned ? "取消置顶" : "置顶";
    return (
      <button
        className={
          iconOnly
            ? `watch-pin-toggle ${pinned ? "pinned" : ""}`
            : "text-button"
        }
        disabled={busy}
        onClick={() => row.watch && onPin(row.watch)}
        aria-label={`${label}${row.company.name}`}
        title={iconOnly ? label : undefined}
      >
        <Icon size={iconOnly ? 14 : 15} aria-hidden="true" />
        {!iconOnly && label}
      </button>
    );
  };
  const add = (row: WatchDisplayRow) => (
    <button
      className="text-button"
      disabled={busy || favorites.has(row.company.symbol) || !canAdd}
      aria-label={`${favorites.has(row.company.symbol) ? "已在自选" : "加入自选"}${row.company.name}`}
      onClick={() => onAdd(row)}
    >
      {favorites.has(row.company.symbol) ? "已在自选" : "加入自选"}
    </button>
  );
  const sortHeader = (
    label: string,
    first: WatchSort,
    second: WatchSort,
    firstDirection: "ascending" | "descending",
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
      <th scope="col" aria-sort={direction && absolute ? "other" : direction}>
        <button
          className="watch-sort-header"
          onClick={() => onSort(next)}
          aria-label={`${label}：${direction ? `当前${sortOptions[sort]}，` : ""}点击按${sortOptions[next]}`}
          title={sortOptions[next]}
        >
          {label}
          <Icon size={13} aria-hidden="true" />
        </button>
      </th>
    );
  };
  if (view === "compact")
    return (
      <div className="panel watch-overview table-scroll">
        <table aria-label="观察公司对比">
          <thead>
            <tr>
              <th>公司 / 行情日</th>
              {sortHeader("收盘价", "price-asc", "price-desc", "ascending")}
              {sortHeader("TTM 股息率", "yield", "yield-asc", "descending")}
              <th>5% 参考价</th>
              {sortHeader(
                "距日线下轨",
                "daily",
                "daily-far",
                "ascending",
                true,
              )}
              {sortHeader(
                "距周线中轨",
                "weekly",
                "weekly-far",
                "ascending",
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
                    {pin(row, true)}
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
                  className={`watch-numeric ${checks(row, minYield)[0] === true ? "watch-met" : ""}`}
                >
                  {number(row.latest?.yieldPct, "%")}
                </td>
                <td data-label="5% 参考价" className="watch-numeric">
                  {row.dividendPrices.find((p) => p.yieldPct === 5)?.price !=
                  null
                    ? formatAmount(
                        row.dividendPrices.find((p) => p.yieldPct === 5)!.price,
                      )
                    : "—"}
                </td>
                <td
                  data-label="距日线下轨"
                  className={`watch-numeric ${row.latest?.daily?.matchesRule ? "watch-met" : ""}`}
                >
                  {distance(row.latest?.daily?.distancePct)}
                </td>
                <td
                  data-label="距周线中轨"
                  className={`watch-numeric ${row.latest?.weekly?.matchesRule ? "watch-met" : ""}`}
                >
                  {distance(row.latest?.weekly?.distancePct)}
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
  return (
    <div className="watch-list">
      {rows.map((row) => (
        <article
          className={`panel watch-card ${row.watch?.alert ? "watch-unread" : ""}`}
          key={row.company.symbol}
        >
          <div className="watch-heading">
            <div>
              <h3>
                {row.company.name}
                <small>{row.company.symbol}</small>
              </h3>
              {row.watch?.entry.pinned && (
                <span className="watch-pinned">已置顶</span>
              )}
              <span>
                {held.has(row.company.symbol) ? "已关联原账本持仓" : "尚未持有"}
              </span>
            </div>
            {state(row)}
          </div>
          <div className="watch-card-alert">{alert(row)}</div>
          <div className="watch-metrics">
            <div>
              <span>收盘价</span>
              <strong>{number(row.latest?.price)}</strong>
            </div>
            <div>
              <span>TTM 税前股息率</span>
              <strong>{number(row.latest?.yieldPct, "%")}</strong>
            </div>
            <div>
              <span>TTM 每股分红</span>
              <strong>{row.latest?.dividendPerShare ?? "—"}</strong>
            </div>
          </div>
          <RuleChecks row={row} minYield={minYield} />
          <div className="watch-bands">
            <BandTrack
              label="日线"
              band={row.latest?.daily}
              price={row.latest?.price}
              tolerance={tolerance}
            />
            <BandTrack
              label="周线"
              band={row.latest?.weekly}
              price={row.latest?.price}
              tolerance={tolerance}
            />
          </div>
          <YieldGrid row={row} />
          <p className="watch-meta">
            行情日期 {row.latest?.observedOn ?? "—"} · 分红覆盖至{" "}
            {row.latest?.dividendBasis?.asOf ?? "—"}
          </p>
          {issue(row)}
          {row.watch?.entry.thesis && (
            <p className="watch-thesis">{row.watch?.entry.thesis}</p>
          )}
          <div className="watch-actions">
            {pin(row)}
            {!row.watch && add(row)}
            {row.watch && (
              <button
                className="primary"
                onClick={() => row.watch && onAsk(row.watch)}
              >
                <MessageCircle size={15} />
                结合我的交易讨论
              </button>
            )}
            <button className="text-button" onClick={() => onDetails(row)}>
              {row.watch ? "查看依据与交易" : "查看依据"}
            </button>
            {row.watch && (
              <button
                className="text-button"
                disabled={busy}
                onClick={() => row.watch && onEdit(row.watch)}
              >
                编辑
              </button>
            )}
          </div>
        </article>
      ))}
    </div>
  );
}
