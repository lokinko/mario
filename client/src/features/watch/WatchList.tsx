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
}: {
  rows: WatchRow[];
  view: "compact" | "cards";
  held: Set<string>;
  busy: boolean;
  minYield: number;
  tolerance: number;
  sort: WatchSort;
  onSort: (sort: WatchSort) => void;
  onPin: (row: WatchRow) => void;
  onDetails: (row: WatchRow) => void;
  onAsk: (row: WatchRow) => void;
  onEdit: (row: WatchRow) => void;
  onRead: (row: WatchRow) => void;
  onRetry: () => void;
}) {
  const state = (row: WatchRow) => (
    <Badge tone={row.latest?.status === "attention" ? "success" : "neutral"}>
      {row.latest ? statuses[row.latest.status] : "等待首次检查"}
    </Badge>
  );
  const issue = (row: WatchRow) =>
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
  const alert = (row: WatchRow) =>
    row.alert && (
      <button
        className="watch-alert-button"
        disabled={busy}
        onClick={() => onRead(row)}
      >
        <Bell size={13} />
        标为已读
      </button>
    );
  const pin = (row: WatchRow) => {
    const pinned = row.entry.pinned;
    const Icon = pinned ? PinOff : Pin;
    const label = pinned ? "取消置顶" : "置顶";
    return (
      <button
        className="text-button"
        disabled={busy}
        onClick={() => onPin(row)}
        aria-label={`${label}${row.entry.name}`}
      >
        <Icon size={15} aria-hidden="true" />
        {label}
      </button>
    );
  };
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
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr
                key={row.entry.symbol}
                className={row.alert ? "watch-unread" : ""}
              >
                <td data-label="公司">
                  <button
                    className="watch-company"
                    onClick={() => onDetails(row)}
                  >
                    <strong>{row.entry.name}</strong>
                    <small>
                      {row.entry.symbol} ·{" "}
                      {held.has(row.entry.symbol) ? "已持有" : "未持有"}
                    </small>
                  </button>
                  {row.entry.pinned && (
                    <span className="watch-pinned">已置顶</span>
                  )}
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
                  </div>
                  {issue(row)}
                </td>
                <td data-label="操作">
                  <div className="watch-row-actions">
                    {pin(row)}
                    <button
                      className="text-button"
                      onClick={() => onDetails(row)}
                    >
                      查看依据与交易
                    </button>
                    <button
                      className="text-button"
                      onClick={() => onAsk(row)}
                      aria-label="结合我的交易讨论"
                    >
                      <MessageCircle size={15} />
                      讨论
                    </button>
                  </div>
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
          className={`panel watch-card ${row.alert ? "watch-unread" : ""}`}
          key={row.entry.symbol}
        >
          <div className="watch-heading">
            <div>
              <h3>
                {row.entry.name}
                <small>{row.entry.symbol}</small>
              </h3>
              {row.entry.pinned && <span className="watch-pinned">已置顶</span>}
              <span>
                {held.has(row.entry.symbol) ? "已关联原账本持仓" : "尚未持有"}
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
          {row.entry.thesis && (
            <p className="watch-thesis">{row.entry.thesis}</p>
          )}
          <div className="watch-actions">
            {pin(row)}
            <button className="primary" onClick={() => onAsk(row)}>
              <MessageCircle size={15} />
              结合我的交易讨论
            </button>
            <button className="text-button" onClick={() => onDetails(row)}>
              查看依据与交易
            </button>
            <button
              className="text-button"
              disabled={busy}
              onClick={() => onEdit(row)}
            >
              编辑
            </button>
          </div>
        </article>
      ))}
    </div>
  );
}
