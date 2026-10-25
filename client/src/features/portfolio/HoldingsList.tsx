import { EmptyState } from "../../components/EmptyState";
import { Search } from "lucide-react";
import type { ReactNode } from "react";
import type { Holding, Snapshot } from "../../types";
import { isClosedHolding } from "./holdingMetrics";

export type HoldingSort =
  "value-desc" | "value-asc" | "pnl-desc" | "pnl-asc" | "name";
export type HoldingFilters = {
  search: string;
  sort: HoldingSort;
  showClosed: boolean;
};
export const defaultHoldingFilters: HoldingFilters = {
  search: "",
  sort: "value-desc",
  showClosed: false,
};

const columns = [
  "资产 / 币种",
  "持仓数",
  "现价 / 成本价",
  "当前市值",
  "浮动盈亏",
  "已结算收益",
  "操作",
];

export function HoldingsList({
  snapshot,
  holdings,
  filters,
  onFiltersChange,
  renderRow,
  onRecordTrade,
}: {
  snapshot: Snapshot;
  holdings: Holding[];
  filters: HoldingFilters;
  onFiltersChange: (filters: HoldingFilters) => void;
  renderRow: (holding: Holding) => ReactNode;
  onRecordTrade: () => void;
}) {
  const { search, sort, showClosed } = filters;
  const update = (changes: Partial<HoldingFilters>) =>
    onFiltersChange({ ...filters, ...changes });
  const currency = snapshot.profile.baseCurrency;
  const closed = (item: Holding) =>
    isClosedHolding(
      item,
      Boolean(
        snapshot.holdingTradeSummaries?.[item.id] ||
        snapshot.stockOptionTotals?.[item.id],
      ),
    ) && !snapshot.stockOptionTotals?.[item.id]?.openContracts;
  const closedCount = holdings.filter(closed).length;
  const query = search.trim().toLocaleLowerCase();
  const visible = holdings.filter(
    (item) =>
      (showClosed || !closed(item)) &&
      `${item.name} ${item.symbol} ${item.assetClass} ${item.currency}`
        .toLocaleLowerCase()
        .includes(query),
  );
  const groups = Array.from(new Set(visible.map((item) => item.currency)))
    .sort((a, b) =>
      a === currency ? -1 : b === currency ? 1 : a.localeCompare(b),
    )
    .map((code) => ({
      currency: code,
      holdings: visible.filter((item) => item.currency === code),
    }));
  if (holdings.length === 0)
    return (
      <div
        className="facts-assets investment-assets"
        role="table"
        aria-label="持仓列表"
        aria-colcount={7}
      >
        <div role="row">
          <div role="cell" aria-colspan={7}>
            <EmptyState
              title="还没有持仓"
              action={
                <button className="primary" onClick={onRecordTrade}>
                  记录第一笔交易
                </button>
              }
            >
              从第一笔真实成交开始记录。
            </EmptyState>
          </div>
        </div>
      </div>
    );
  return (
    <>
      <p className="sr-only" id="holdings-sort-help">
        {sort === "name"
          ? "按币种分组，组内按名称排列 · 已清仓默认收起"
          : sort.startsWith("pnl")
            ? `按币种分组，组内按原币浮动盈亏${sort === "pnl-asc" ? "从低到高" : "从高到低"}排列 · 已清仓默认收起`
            : `按币种分组，组内按折合 ${currency} 市值${sort === "value-asc" ? "从低到高" : "从高到低"}排列 · 已清仓默认收起，展开后按最近交易排列`}
        {sort !== "name" &&
          holdings.some(
            (item) =>
              !closed(item) &&
              item.currency !== currency &&
              !(item.fxRateToBase != null && item.fxRateToBase > 0),
          ) &&
          " · 缺少汇率的资产排在未清仓资产末尾"}
      </p>
      <div className="holdings-toolbar" data-presentation="true">
        <div className="position-filters">
          <span>按币种分组</span>
        </div>
        <div className="holdings-tools">
          <label className="holdings-search">
            <Search size={16} aria-hidden="true" />
            <input
              type="search"
              aria-label="搜索持仓"
              placeholder="搜索名称、代码或币种"
              value={search}
              onChange={(event) => update({ search: event.target.value })}
            />
          </label>
          <select
            aria-label="持仓排序"
            aria-describedby="holdings-sort-help"
            value={sort}
            onChange={(event) =>
              update({ sort: event.target.value as HoldingSort })
            }
          >
            <option value="value-desc">市值从高到低</option>
            <option value="value-asc">市值从低到高</option>
            <option value="pnl-desc">浮动盈亏从高到低</option>
            <option value="pnl-asc">浮动盈亏从低到高</option>
            <option value="name">名称排序</option>
          </select>
        </div>
      </div>
      <div
        className="facts-assets investment-assets"
        role="table"
        aria-label="持仓列表"
        aria-colcount={7}
      >
        <div role="row" className="holdings-list-header">
          {columns.map((label) => (
            <span
              role="columnheader"
              key={label}
              aria-sort={
                label === "资产 / 币种" && sort === "name"
                  ? "ascending"
                  : (label === "当前市值" && sort.startsWith("value")) ||
                      (label === "浮动盈亏" && sort.startsWith("pnl"))
                    ? sort.endsWith("asc")
                      ? "ascending"
                      : "descending"
                    : undefined
              }
            >
              {["资产 / 币种", "当前市值", "浮动盈亏"].includes(label) ? (
                <button
                  className="column-sort"
                  data-presentation="true"
                  onClick={() =>
                    update({
                      sort:
                        label === "资产 / 币种"
                          ? "name"
                          : label === "当前市值"
                            ? sort === "value-desc"
                              ? "value-asc"
                              : "value-desc"
                            : sort === "pnl-desc"
                              ? "pnl-asc"
                              : "pnl-desc",
                    })
                  }
                >
                  {label}
                  {label === "资产 / 币种" && sort === "name"
                    ? " ▲"
                    : (label === "当前市值" && sort.startsWith("value")) ||
                        (label === "浮动盈亏" && sort.startsWith("pnl"))
                      ? sort.endsWith("asc")
                        ? " ▲"
                        : " ▼"
                      : ""}
                </button>
              ) : (
                label
              )}
            </span>
          ))}
        </div>
        {groups.map((group) => (
          <div
            role="rowgroup"
            aria-label={group.currency + " 持仓"}
            key={group.currency}
          >
            <div role="row" className="holdings-currency-header">
              <div role="cell" aria-colspan={7}>
                <strong>{group.currency}</strong>
                <span>{group.holdings.length} 项持仓</span>
              </div>
            </div>
            {group.holdings.map(renderRow)}
          </div>
        ))}
        {visible.length === 0 && (
          <div className="empty-state" role="row">
            <div role="cell" aria-colspan={7}>
              <p>
                {query
                  ? "没有找到匹配的持仓。"
                  : "暂无未清仓持仓，可点击上方展开已清仓持仓。"}
              </p>
              {query && (
                <button
                  className="text-button"
                  onClick={() => update({ search: "" })}
                >
                  清除筛选
                </button>
              )}
            </div>
          </div>
        )}
      </div>
      {closedCount > 0 && (
        <button
          className="text-button closed-toggle"
          data-presentation="true"
          aria-expanded={showClosed}
          onClick={() => update({ showClosed: !showClosed })}
        >
          {showClosed ? "收起已清仓" : "已清仓"} {closedCount} 项 ›
        </button>
      )}
    </>
  );
}
