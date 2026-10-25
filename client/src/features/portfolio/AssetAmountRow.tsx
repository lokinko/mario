import type {
  DataIssue,
  Holding,
  AccountingHolding,
  StockOptionTotals,
} from "../../types";
import {
  formatAmount,
  formatNumber,
  formatPercent,
  formatSigned,
} from "../../lib/format";
import { Badge } from "../../components/Badge";
import { Menu } from "../../components/Menu";
import { isClosedHolding, unrealizedProfit } from "./holdingMetrics";
export function AssetAmountRow({
  holding,
  realizedPnl,
  onOpen,
  onEdit,
  onDelete,
  onTrade,
  confirmedOn,
  disabled = false,
  canDelete = true,
  exact,
  optionTotals,
  issues = [],
  onRepair,
  source,
  observedOn,
}: {
  holding: Holding;
  issues?: DataIssue[];
  onRepair?: (issue: DataIssue) => void;
  source?: string;
  observedOn?: string;
  realizedPnl?: number | string;
  onOpen?: () => void;
  onEdit: () => void;
  onDelete: () => void;
  onTrade?: () => void;
  confirmedOn?: string;
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
    ? optionTotals.unrealizedProfit
    : exact
      ? exact.unrealizedProfit
      : unrealizedProfit(holding, realizedPnl != null);
  const settled = exact ? exact.settledProfit : realizedPnl;
  const color = (value: number | string | null | undefined) =>
    value == null || Number(value) === 0
      ? ""
      : Number(value) > 0
        ? "gain"
        : "loss";
  const money = (value: number | string) =>
    formatAmount(value, holding.currency);
  return (
    <div
      className={`asset-list-row${investment ? "" : " cash-list-row"}${closed ? " closed-holding" : ""}`}
      role="row"
      onClick={(event) => {
        if (
          !disabled &&
          !(event.target as HTMLElement).closest("button, .menu")
        )
          (onOpen ?? onEdit)();
      }}
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
          {closed && <Badge>已清仓</Badge>}
        </small>
        {investment && (
          <small>
            {observedOn || holding.valuationDate || "日期未记录"} ·{" "}
            {source || "手工估值"}
          </small>
        )}
        {issues.map((issue) => (
          <div
            className="holding-data-issue"
            key={`${issue.code}-${issue.holdingIds.join()}`}
          >
            <Badge tone="warning">{issue.message}</Badge>
            <button
              className="text-button"
              disabled={disabled}
              onClick={() => onRepair?.(issue)}
            >
              {
                {
                  missingFxRate: "补汇率",
                  pendingCorporateAction: "确认拆合股",
                  quarantinedTrades: "修复交易链",
                  cashNeedsReconciliation: "核对存款",
                  invalidTargetWeights: "编辑目标权重",
                }[issue.code]
              }
            </button>
          </div>
        ))}
      </div>
      {investment && (
        <>
          <div role="cell" data-label="数量" className="num">
            <span className="desktop-quantity">
              {" "}
              {exact?.usable === false || holding.quantity == null
                ? "—"
                : formatNumber(exact?.quantity ?? holding.quantity, 4)}
            </span>
            <span className="mobile-quantity">
              数量{" "}
              {exact?.usable === false || holding.quantity == null
                ? "—"
                : formatNumber(exact?.quantity ?? holding.quantity, 4)}{" "}
              · 现价{" "}
              <span data-money>
                {exact?.usable === false || holding.currentPrice == null
                  ? "—"
                  : formatNumber(
                      exact?.currentPrice ?? holding.currentPrice,
                      4,
                    )}
              </span>
            </span>
          </div>
          <div role="cell" data-label="现价 / 成本价" className="num">
            <span data-money>
              {exact?.usable === false || holding.currentPrice == null
                ? "—"
                : formatNumber(exact?.currentPrice ?? holding.currentPrice, 4)}
            </span>
            <small>
              成本{" "}
              <span data-money>
                {exact?.usable === false || holding.costPrice == null
                  ? "—"
                  : formatNumber(exact?.costPrice ?? holding.costPrice, 4)}
              </span>
            </small>
          </div>
        </>
      )}
      <div
        role="cell"
        data-label={investment ? "市值" : "余额"}
        className="asset-value"
      >
        <strong className="num" data-money>
          {exact && !exact.usable
            ? "—"
            : money(exact?.marketValue ?? holding.marketValue)}
        </strong>
        {investment &&
          exact?.priceStatus &&
          ["unconfirmed", "provisional", "corporate_pending_price"].includes(
            exact.priceStatus,
          ) && (
            <Badge title={`参考价 · ${holding.valuationDate || "日期待补"}`}>
              参考价
            </Badge>
          )}
      </div>
      {!investment && (
        <div role="cell" className="cash-confirmed-date" data-label="核对日期">
          {confirmedOn || holding.valuationDate || "—"}
        </div>
      )}
      {investment && (
        <>
          <div
            role="cell"
            data-label="浮动盈亏"
            className={exact?.usable === false ? "" : color(pnl)}
          >
            <span className="num" data-money>
              {exact && !exact.usable
                ? "—"
                : pnl == null
                  ? "—"
                  : formatSigned(pnl, money)}
            </span>
            <small>
              {exact?.usable === false ? null : !optionTotals &&
                pnl != null &&
                holding.costBasis > 0 ? (
                <span className="pnl-percent num">
                  {formatPercent((Number(pnl) / holding.costBasis) * 100)}
                </span>
              ) : optionTotals ? (
                "股票与期权合计"
              ) : closed ? (
                "已清仓"
              ) : (
                "成本待补"
              )}
            </small>
          </div>
          <div
            role="cell"
            data-label="已结算收益"
            className={`num ${exact && !exact.usable ? "" : color(settled)}`}
          >
            <span data-money>
              {exact && !exact.usable
                ? "—"
                : settled == null
                  ? "—"
                  : formatSigned(settled, money)}
            </span>
          </div>
        </>
      )}
      <div role="cell" className="row-actions">
        <Menu
          label={holding.name + "更多操作"}
          disabled={disabled}
          items={[
            { label: "编辑 " + holding.name, action: onEdit },
            ...(onTrade ? [{ label: "记录交易", action: onTrade }] : []),
            ...(canDelete
              ? [
                  {
                    label: "删除 " + holding.name,
                    action: onDelete,
                    danger: true,
                  },
                ]
              : []),
          ]}
        />
      </div>
    </div>
  );
}
