import { useEffect, useState } from "react";
import { StockOptions } from "./StockOptions";
import type { Holding, Snapshot } from "../../types";
import { formatMoney, formatSigned } from "../../lib/format";
import { HoldingTrades } from "./HoldingTrades";
import { isClosedHolding } from "./holdingMetrics";

export function HoldingDetail({
  snapshot,
  holding,
  busy,
  editingTrade,
  onEditingTradeChange,
  onEdit,
  onDelete,
  onUpdate,
  flash,
}: {
  snapshot: Snapshot;
  holding: Holding;
  busy: boolean;
  editingTrade: boolean;
  onEditingTradeChange: (editing: boolean) => void;
  onEdit: () => void;
  onDelete: () => void;
  onUpdate: (snapshot: Snapshot) => void;
  flash: (message: string) => void;
}) {
  const [stockEditing, setStockEditing] = useState(false);
  const [optionEditing, setOptionEditing] = useState(false);
  useEffect(() => {
    onEditingTradeChange(stockEditing || optionEditing);
    return () => onEditingTradeChange(false);
  }, [stockEditing, optionEditing, onEditingTradeChange]);
  const tradeSummary = snapshot.holdingTradeSummaries?.[holding.id];
  const valuations =
    snapshot.holdingValuations?.filter((v) => v.holdingId === holding.id) ?? [];
  const usable = snapshot.accounting?.holdings[holding.id]?.usable !== false;
  const pendingTrade = editingTrade ? "请先保存或取消当前交易" : undefined;
  return (
    <>
      <dl className="detail-grid">
        <div>
          <dt>当前市值</dt>
          <dd>
            <span className="num" data-money>
              {!usable
                ? "—"
                : formatMoney(
                    holding.exactValues?.marketValue ?? holding.marketValue,
                    holding.currency,
                  )}
            </span>
          </dd>
        </div>
        <div>
          <dt>持仓总成本</dt>
          <dd>
            <span className="num" data-money>
              {usable &&
              (holding.costBasis > 0 ||
                isClosedHolding(holding, Boolean(tradeSummary)))
                ? formatMoney(
                    holding.exactValues?.costBasis ?? holding.costBasis,
                    holding.currency,
                  )
                : "—"}
            </span>
          </dd>
        </div>
        <div>
          <dt>已结算盈亏</dt>
          <dd>
            <span className="num" data-money>
              {!usable
                ? "—"
                : tradeSummary
                  ? formatSigned(tradeSummary.settledProfit, (value) =>
                      formatMoney(value, holding.currency),
                    )
                  : "未录入交易"}
            </span>
          </dd>
        </div>
      </dl>
      {valuations.map((v) => (
        <p className="section-intro" key={v.holdingId}>
          收盘日 {v.observedOn} · {v.disclaimer}
        </p>
      ))}
      {holding.assetClass === "股票" && (
        <StockOptions
          key={holding.id}
          snapshot={snapshot}
          stock={holding}
          disabled={busy || stockEditing}
          onEditingChange={setOptionEditing}
          onUpdate={onUpdate}
          flash={flash}
        />
      )}
      {holding.assetClass !== "现金" && (
        <>
          <h3>交易明细</h3>
          <fieldset
            disabled={busy || optionEditing}
            className="holding-trades-controls"
          >
            <HoldingTrades
              key={holding.id}
              snapshot={snapshot}
              holding={holding}
              hideSummary
              onEditingChange={setStockEditing}
              onUpdate={onUpdate}
              flash={flash}
            />
          </fieldset>
        </>
      )}
      <div className="detail-actions holding-detail-footer">
        <button
          className="primary"
          disabled={busy || editingTrade}
          title={pendingTrade}
          onClick={onEdit}
        >
          编辑资产
        </button>
        <button
          type="button"
          className="secondary danger-text"
          disabled={busy || editingTrade}
          title={pendingTrade}
          onClick={onDelete}
        >
          删除持仓
        </button>
      </div>
    </>
  );
}
