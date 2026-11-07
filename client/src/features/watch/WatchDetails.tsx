import { useState } from "react";
import type { WatchRules, WatchGridPlan, WatchRow } from "../../api/watch";
import type { Snapshot } from "../../types";
import { Drawer } from "../../components/Drawer";
import { Callout } from "../../components/Callout";
import { WatchEvidence } from "./WatchEvidence";
import { WatchGrid, WatchGridEditor } from "./WatchGrid";
import { canonical, statuses } from "./presentation";

export function WatchDetails({
  row,
  snapshot,
  rules,
  busy,
  error,
  onClose,
  onGridEdited,
  onGridClosed,
  onGridSave,
  onAsk,
  onEdit,
  onRemove,
  onRetry,
}: {
  row: WatchRow;
  snapshot: Snapshot;
  rules: WatchRules;
  busy: boolean;
  error: string;
  onClose: () => void;
  onGridEdited: () => void;
  onGridClosed: () => void;
  onGridSave: (plan: WatchGridPlan | null) => Promise<boolean>;
  onAsk: () => void;
  onEdit: () => void;
  onRemove: () => void;
  onRetry: () => void;
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
      title={editingGrid ? "股息网格" : entry.name}
      subtitle={
        editingGrid
          ? `${entry.name} · ${entry.symbol}`
          : `${entry.symbol} · ${latest ? statuses[latest.status] : "等待首次检查"}`
      }
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
            {row.refreshFailed && (
              <Callout
                tone="warning"
                action={
                  <button
                    type="button"
                    className="text-button"
                    disabled={busy}
                    onClick={onRetry}
                  >
                    重新检查数据
                  </button>
                }
              >
                {latest
                  ? `本次来源读取失败，保留 ${latest.observedOn} 的观察结果。`
                  : "本次来源读取失败，尚无可用的观察结果。"}
              </Callout>
            )}
            <WatchEvidence row={row} rules={rules}>
              <WatchGrid row={row} onEdit={() => setEditingGrid(true)} />
            </WatchEvidence>
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
