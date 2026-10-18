import type { Holding, Snapshot } from "../../types";
import { formatMoney } from "../../lib/format";
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
  const tradeSummary = snapshot.holdingTradeSummaries?.[holding.id];
  const valuations =
    snapshot.holdingValuations?.filter((v) => v.holdingId === holding.id) ?? [];
  const pendingTrade = editingTrade ? "请先保存或取消当前交易" : undefined;
  return (
    <>
      <div className="detail-actions">
        <button
          className="secondary"
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
      <dl className="detail-grid">
        <div>
          <dt>当前市值</dt>
          <dd>
            {formatMoney(
              holding.exactValues?.marketValue ?? holding.marketValue,
              holding.currency,
            )}
          </dd>
        </div>
        <div>
          <dt>持仓总成本</dt>
          <dd>
            {holding.costBasis > 0 ||
            isClosedHolding(holding, Boolean(tradeSummary))
              ? formatMoney(
                  holding.exactValues?.costBasis ?? holding.costBasis,
                  holding.currency,
                )
              : "—"}
          </dd>
        </div>
        <div>
          <dt>已结算盈亏</dt>
          <dd>
            {tradeSummary
              ? formatMoney(
                  Number(tradeSummary.settledProfit),
                  holding.currency,
                )
              : "未录入交易"}
          </dd>
        </div>
        <div>
          <dt>估值日期</dt>
          <dd>{holding.valuationDate || "待补"}</dd>
        </div>
        <div>
          <dt>证券代码 / 币种</dt>
          <dd>
            {holding.symbol || "—"} / {holding.currency}
          </dd>
        </div>
        <div>
          <dt>行情来源</dt>
          <dd>{valuations[0]?.providerName || "手动记录"}</dd>
        </div>
      </dl>
      {valuations.map((v) => (
        <p className="section-intro" key={v.holdingId}>
          收盘日 {v.observedOn} · {v.disclaimer}
        </p>
      ))}
      {holding.assetClass !== "现金" && (
        <>
          <h3>交易明细</h3>
          {snapshot.clearedTradeHoldings?.includes(holding.id) && (
            <p className="warning-text">交易明细需重新录入</p>
          )}
          <fieldset disabled={busy} className="holding-trades-controls">
            <HoldingTrades
              key={holding.id}
              snapshot={snapshot}
              holding={holding}
              onEditingChange={onEditingTradeChange}
              onUpdate={onUpdate}
              flash={flash}
            />
          </fieldset>
        </>
      )}
    </>
  );
}
