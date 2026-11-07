import { useState } from "react";
import type { WatchGridPlan, WatchRow } from "../../api/watch";
import type { Snapshot } from "../../types";
import { Drawer } from "../../components/Drawer";
import { Callout } from "../../components/Callout";
import { BandTrack, RuleChecks, YieldGrid } from "./WatchSignals";
import { WatchDividends } from "./WatchDividends";
import { WatchGrid, WatchGridEditor } from "./WatchGrid";
import { canonical, number, statuses } from "./presentation";

export function WatchDetails({
  row,
  snapshot,
  minYield,
  tolerance,
  busy,
  error,
  onClose,
  onGridEdited,
  onGridClosed,
  onGridSave,
  onAsk,
  onEdit,
  onRemove,
}: {
  row: WatchRow;
  snapshot: Snapshot;
  minYield: number;
  tolerance: number;
  busy: boolean;
  error: string;
  onClose: () => void;
  onGridEdited: () => void;
  onGridClosed: () => void;
  onGridSave: (plan: WatchGridPlan | null) => Promise<boolean>;
  onAsk: () => void;
  onEdit: () => void;
  onRemove: () => void;
}) {
  const { entry, latest } = row;
  const [editingGrid, setEditingGrid] = useState(false);
  const closeGrid = () => {
    setEditingGrid(false);
    onGridClosed();
  };
  const related = snapshot.holdings.filter(
    (h) => canonical(h.symbol) === entry.symbol,
  );
  const ids = new Set(related.map((h) => h.id));
  const trades = (snapshot.holdingTrades ?? [])
    .filter((t) => ids.has(t.holdingId))
    .sort(
      (a, b) =>
        b.occurredOn.localeCompare(a.occurredOn) || b.sequence - a.sequence,
    )
    .slice(0, 5);
  return (
    <Drawer
      title={editingGrid ? `${entry.name} · 股息网格` : entry.name}
      subtitle={`${entry.symbol} · ${latest ? statuses[latest.status] : "等待首次检查"}`}
      onClose={() => {
        if (editingGrid) onGridClosed();
        onClose();
      }}
      busy={busy}
      size="wide"
    >
      <div className="watch-detail-body">
        {error && <Callout tone="danger">{error}</Callout>}
        {editingGrid ? (
          <WatchGridEditor
            plan={entry.gridPlan}
            busy={busy}
            onEdited={onGridEdited}
            onCancel={closeGrid}
            onSave={async (plan) => {
              if (!(await onGridSave(plan))) return false;
              closeGrid();
              return true;
            }}
          />
        ) : (
          <>
            <div className="watch-metrics">
              <div>
                <span>收盘价 · {latest?.observedOn ?? "—"}</span>
                <strong>{number(latest?.price)}</strong>
              </div>
              <div>
                <span>TTM 税前股息率</span>
                <strong>{number(latest?.yieldPct, "%")}</strong>
              </div>
              <div>
                <span>TTM 每股分红 / 元</span>
                <strong>{latest?.dividendPerShare ?? "—"}</strong>
              </div>
            </div>
            <RuleChecks row={row} minYield={minYield} />
            <div className="watch-bands">
              <BandTrack
                label="日线"
                band={latest?.daily}
                price={latest?.price}
                tolerance={tolerance}
              />
              <BandTrack
                label="周线"
                band={latest?.weekly}
                price={latest?.price}
                tolerance={tolerance}
              />
            </div>
            <YieldGrid row={row} />
            <WatchGrid row={row} onEdit={() => setEditingGrid(true)} />
            <WatchDividends latest={latest} />
            {entry.thesis && (
              <section className="watch-detail-section">
                <h3>关注理由</h3>
                <p className="watch-thesis">{entry.thesis}</p>
              </section>
            )}
            <section className="watch-detail-section">
              <h3>已有持仓与最近交易</h3>
              {related.map((h) => (
                <p key={h.id}>
                  {h.name} · 数量{" "}
                  {snapshot.accounting?.holdings[h.id]?.quantity ??
                    h.quantity ??
                    "—"}{" "}
                  · 成本 {snapshot.accounting?.holdings[h.id]?.costBasis ?? "—"}{" "}
                  {h.currency}
                </p>
              ))}
              {!trades.length ? (
                <p>没有关联的已录入交易。</p>
              ) : (
                <ul className="watch-trades">
                  {trades.map((t) => (
                    <li key={t.id}>
                      {t.occurredOn} · {t.account} ·{" "}
                      {
                        {
                          buy: "买入",
                          sell: "卖出",
                          dividend: "分红",
                          expire: "到期",
                        }[t.kind]
                      }{" "}
                      · 成交金额 {t.grossAmount} {t.currency}
                      {t.kind === "buy" || t.kind === "sell"
                        ? ` · 原始成交价 ${t.price}`
                        : ""}
                    </li>
                  ))}
                </ul>
              )}
            </section>
            <div className="watch-actions">
              <button className="primary" onClick={onAsk}>
                结合我的交易讨论
              </button>
              <button className="secondary" disabled={busy} onClick={onEdit}>
                编辑观察资料
              </button>
              <button
                className="text-button danger"
                disabled={busy}
                onClick={onRemove}
              >
                移出观察名单
              </button>
            </div>
            <p className="watch-meta">
              最近检查{" "}
              {row.checkedAt
                ? new Date(row.checkedAt).toLocaleString("zh-CN", {
                    timeZone: "Asia/Shanghai",
                  })
                : "—"}
            </p>
          </>
        )}
      </div>
    </Drawer>
  );
}
