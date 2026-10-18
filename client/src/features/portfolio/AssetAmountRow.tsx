import type {
  Holding,
  AccountingHolding,
  StockOptionTotals,
} from "../../types";
import { formatMoney, formatNumber } from "../../lib/format";
import { isClosedHolding, unrealizedProfit } from "./holdingMetrics";
export function AssetAmountRow({
  holding,
  realizedPnl,
  onOpen,
  onEdit,
  onDelete,
  disabled = false,
  canDelete = true,
  exact,
  optionTotals,
}: {
  holding: Holding;
  realizedPnl?: number | string;
  onOpen?: () => void;
  onEdit: () => void;
  onDelete: () => void;
  disabled?: boolean;
  canDelete?: boolean;
  exact?: AccountingHolding;
  optionTotals?: StockOptionTotals;
}) {
  const investment = holding.assetClass !== "现金";
  const closed =
    isClosedHolding(holding, realizedPnl != null || Boolean(optionTotals)) &&
    !optionTotals?.openContracts;
  const pnl = optionTotals
    ? optionTotals.unrealizedProfit == null
      ? null
      : Number(optionTotals.unrealizedProfit)
    : unrealizedProfit(holding, realizedPnl != null);
  const settledProfit = exact ? exact.settledProfit : realizedPnl;
  return (
    <div
      className={"asset-list-row" + (investment ? "" : " cash-list-row")}
      role="row"
    >
      <div role="cell" className="asset-identity">
        <button
          className="text-button asset-name"
          disabled={disabled}
          onClick={onOpen ?? onEdit}
          aria-label={"查看 " + holding.name}
        >
          <strong>{holding.name}</strong>
        </button>
        <small>
          {holding.symbol && holding.symbol + " · "}
          {holding.currency} · {holding.assetClass}
          {optionTotals ? " · 含关联期权" : ""}
          {closed ? " · 已清仓" : ""}
        </small>
      </div>
      {investment && (
        <>
          <div role="cell" data-label="数量">
            {holding.quantity == null
              ? "—"
              : formatNumber(exact?.quantity ?? holding.quantity)}
          </div>
          <div role="cell" data-label="现价 / 成本价">
            <span>
              {holding.currentPrice == null
                ? "—"
                : formatNumber(exact?.currentPrice ?? holding.currentPrice)}
            </span>
            <small>
              成本{" "}
              {holding.costPrice == null
                ? "—"
                : formatNumber(exact?.costPrice ?? holding.costPrice)}
            </small>
          </div>
        </>
      )}
      <div
        role="cell"
        data-label={investment ? "市值" : "余额"}
        className="asset-value"
      >
        <strong>
          {exact && !exact.usable
            ? "待核对"
            : formatMoney(
                exact?.marketValue ?? holding.marketValue,
                holding.currency,
              )}
        </strong>
        {!investment && <small>{holding.valuationDate}</small>}
        {investment &&
          exact?.priceStatus &&
          ["unconfirmed", "provisional"].includes(exact.priceStatus) && (
            <small>参考估值，价格待核对</small>
          )}
      </div>
      {investment && (
        <>
          <div
            role="cell"
            data-label="浮动盈亏"
            className={pnl == null ? "" : pnl >= 0 ? "gain" : "loss"}
          >
            {exact && !exact.usable
              ? "待核对"
              : pnl == null
                ? "—"
                : (pnl > 0 ? "+" : "") +
                  formatMoney(exact?.unrealizedProfit ?? pnl, holding.currency)}
            <small>
              {!optionTotals && pnl != null && holding.costBasis > 0
                ? (pnl > 0 ? "+" : "") +
                  formatNumber((pnl / holding.costBasis) * 100) +
                  "%"
                : optionTotals
                  ? "股票与期权合计"
                  : closed
                    ? "已清仓"
                    : "成本待补"}
            </small>
          </div>
          <div
            role="cell"
            data-label="已结算收益"
            className={
              settledProfit == null || (exact && !exact.usable)
                ? ""
                : Number(settledProfit) >= 0
                  ? "gain"
                  : "loss"
            }
          >
            {exact && !exact.usable
              ? "待核对"
              : settledProfit == null
                ? "—"
                : (Number(settledProfit) > 0 ? "+" : "") +
                  formatMoney(settledProfit, holding.currency)}
          </div>
        </>
      )}
      <div role="cell" className="row-actions">
        <button
          className="text-button"
          disabled={disabled}
          aria-label={"编辑 " + holding.name}
          onClick={onEdit}
        >
          编辑
        </button>
        {canDelete && (
          <details className="row-menu">
            <summary aria-label={holding.name + "更多操作"}>···</summary>
            <button
              className="text-button danger-text"
              disabled={disabled}
              aria-label={"删除 " + holding.name}
              onClick={onDelete}
            >
              删除
            </button>
          </details>
        )}
      </div>
    </div>
  );
}
