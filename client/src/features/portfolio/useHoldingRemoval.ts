import { isPendingFinancialRequest } from "../../api/portfolio/financialRequest";
import { useRef, useState } from "react";
import { deleteHolding } from "../../api/portfolio";
import type { Holding, Snapshot } from "../../types";

type RemovalRequest = { id: string; expectedRevision: string };

// Confirms whole-position removal and keeps an uncertain outcome until it is reconciled;
// retrying resubmits the same versioned request, which the server answers idempotently.
export function useHoldingRemoval({
  snapshot,
  onUpdate,
  flash,
  setWorking,
  setError,
  onRemoved,
}: {
  snapshot: Snapshot;
  onUpdate: (snapshot: Snapshot) => void;
  flash: (message: string) => void;
  setWorking: (working: boolean) => void;
  setError: (message: string) => void;
  onRemoved: (id: string) => void;
}) {
  const [uncertain, setUncertain] = useState<RemovalRequest | null>(null);
  const deleting = useRef(false);
  const submit = async (request: RemovalRequest) => {
    if (deleting.current) return;
    deleting.current = true;
    setWorking(true);
    setError("");
    try {
      onUpdate(await deleteHolding(request.id, request.expectedRevision));
      setUncertain(null);
      onRemoved(request.id);
      flash("资产已删除");
    } catch (e) {
      const pending = isPendingFinancialRequest(e);
      setUncertain(pending ? request : null);
      setError(pending ? String(e) : `删除失败：${String(e)}`);
    } finally {
      deleting.current = false;
      setWorking(false);
    }
  };
  const remove = async (item: Holding, blocked: boolean) => {
    if (blocked || uncertain || deleting.current) return;
    const count = (snapshot.holdingTrades ?? []).filter(
      (t) => t.holdingId === item.id,
    ).length;
    const expectedRevision = snapshot.holdingRevisions?.[item.id] ?? "";
    if (
      !window.confirm(
        `确认删除“${item.name}”？\n` +
          (count
            ? `将同时删除 ${count} 笔交易明细，并按这些交易对当前存款的净影响退回买入支出、扣回卖出和分红收入。已核对余额覆盖的历史交易不调整当前存款，会标记待核对。\n`
            : "") +
          "此操作用于移除误录持仓，不代表实际卖出。更正历史和此前日期的资产记录会保留。",
      )
    )
      return;
    if (count && !expectedRevision) {
      setError("持仓版本缺失，请刷新后重试");
      return;
    }
    await submit({ id: item.id, expectedRevision });
  };
  return {
    uncertain: Boolean(uncertain),
    remove,
    reconcile: () => (uncertain ? submit(uncertain) : Promise.resolve()),
  };
}
