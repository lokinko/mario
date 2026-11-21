import { Money } from "../../components/AmountPrivacy";
import { useEffect, useState } from "react";
import { StockOptions } from "./StockOptions";
import type { Holding, Snapshot } from "../../types/user";
import { formatMoney, formatSigned } from "../../lib/format";
import { HoldingTrades } from "./HoldingTrades";
import { isClosedHolding } from "./holdingMetrics";
import { Menu } from "../../components/Menu";

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
  const [declaringSplit, setDeclaringSplit] = useState(false);
  const [addingOption, setAddingOption] = useState(false);
  useEffect(() => {
    onEditingTradeChange(stockEditing || optionEditing);
    return () => onEditingTradeChange(false);
  }, [stockEditing, optionEditing, onEditingTradeChange]);
  const tradeSummary = snapshot.holdingTradeSummaries?.[holding.id];
  const usable = snapshot.accounting?.holdings[holding.id]?.usable !== false;
  const pendingTrade = editingTrade ? "请先保存或取消当前交易" : undefined;
  const trades = (snapshot.holdingTrades ?? []).filter(
    (t) => t.holdingId === holding.id,
  );
  const isOption = Boolean(snapshot.optionContracts?.[holding.id]);
  const canDeclareSplit =
    !isOption &&
    trades.length > 0 &&
    !snapshot.clearedTradeHoldings?.includes(holding.id);
  const hasOptions = Object.values(snapshot.optionContracts ?? {}).some(
    (contract) => contract.underlyingId === holding.id,
  );
  const moreItems = [
    ...(canDeclareSplit
      ? [
          {
            label: "登记拆合股",
            action: () => setDeclaringSplit(true),
          },
        ]
      : []),
    ...(holding.assetClass === "股票" && !hasOptions
      ? [
          {
            label: "添加期权合约",
            action: () => setAddingOption(true),
          },
        ]
      : []),
    {
      label: "删除持仓",
      action: onDelete,
      danger: true,
    },
  ];
  return (
    <>
      <dl className="detail-grid holding-detail-summary">
        <div>
          <dt>当前市值</dt>
          <dd>
            <Money className="num">
              {!usable
                ? "—"
                : formatMoney(
                    holding.exactValues?.marketValue ?? holding.marketValue,
                    holding.currency,
                  )}
            </Money>
          </dd>
        </div>
        <div>
          <dt>持仓总成本</dt>
          <dd>
            <Money className="num">
              {usable &&
              (holding.costBasis > 0 ||
                isClosedHolding(holding, Boolean(tradeSummary)))
                ? formatMoney(
                    holding.exactValues?.costBasis ?? holding.costBasis,
                    holding.currency,
                  )
                : "—"}
            </Money>
          </dd>
        </div>
        <div>
          <dt>已结算盈亏</dt>
          <dd>
            <Money className="num">
              {!usable
                ? "—"
                : tradeSummary
                  ? formatSigned(tradeSummary.settledProfit, (value) =>
                      formatMoney(value, holding.currency),
                    )
                  : "未录入交易"}
            </Money>
          </dd>
        </div>
      </dl>
      {holding.assetClass === "股票" && (
        <StockOptions
          key={holding.id}
          snapshot={snapshot}
          stock={holding}
          disabled={busy || stockEditing}
          openAdd={addingOption}
          onAddClosed={() => setAddingOption(false)}
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
              declaringSplit={declaringSplit}
              onDeclareSplitDone={() => setDeclaringSplit(false)}
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
        <Menu
          label="更多持仓操作"
          disabled={busy || editingTrade}
          items={moreItems}
        />
      </div>
    </>
  );
}
