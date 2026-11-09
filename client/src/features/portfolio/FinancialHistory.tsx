import { useEffect, useState } from "react";
import {
  getFinancialHistory,
  previewTradeRepair,
  repairTrades,
  confirmCorporateAction,
  declareCorporateAction,
} from "../../api/portfolio";
import type {
  Holding,
  HoldingTrade,
  HoldingTradeSummary,
  Snapshot,
} from "../../types";

export function FinancialHistory({
  holding,
  snapshot,
  onUpdate,
  declaring,
  onDeclareDone,
}: {
  holding: Holding;
  snapshot: Snapshot;
  onUpdate: (snapshot: Snapshot) => void;
  declaring: boolean;
  onDeclareDone: () => void;
}) {
  const [history, setHistory] = useState<Awaited<
    ReturnType<typeof getFinancialHistory>
  > | null>(null);
  const [candidate, setCandidate] = useState("");
  const [reason, setReason] = useState("");
  const [preview, setPreview] = useState<HoldingTradeSummary | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [effectiveOn, setEffectiveOn] = useState("");
  const [numerator, setNumerator] = useState("");
  const [denominator, setDenominator] = useState("");
  const [requestId, setRequestId] = useState(() => crypto.randomUUID());
  useEffect(() => {
    let active = true;
    getFinancialHistory(holding.id)
      .then((data) => {
        if (active) {
          setHistory(data);
          if (data.quarantine) {
            try {
              const raw = JSON.parse(data.quarantine.payload) as {
                id: string;
                payload: string;
              }[];
              setCandidate(
                JSON.stringify(
                  raw.map((row) => {
                    const original = [...data.records]
                      .reverse()
                      .map((r) => JSON.parse(r.after) as HoldingTrade | null)
                      .find((t) => t?.id === row.id);
                    return original ?? JSON.parse(row.payload);
                  }),
                  null,
                  2,
                ),
              );
            } catch {
              setCandidate("[]");
            }
          }
        }
      })
      .catch((e) => {
        if (active) setError(String(e));
      });
    return () => {
      active = false;
    };
  }, [holding.id, snapshot.updatedAt]);
  const request = () => ({
    expectedRevision: snapshot.holdingRevisions?.[holding.id] ?? "",
    trades: JSON.parse(candidate) as HoldingTrade[],
    reason,
  });
  const run = async (apply: boolean) => {
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      if (apply) {
        onUpdate(await repairTrades(holding.id, request()));
        setPreview(null);
      } else setPreview(await previewTradeRepair(holding.id, request()));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const actions = (snapshot.corporateActions ?? []).filter(
    (a) => a.holdingId === holding.id,
  );
  const pending = actions.filter((a) => !a.applied && !a.dismissed);
  const applied = actions.filter((a) => a.applied);
  const needsReview = snapshot.valuationStatus.issues.some(
    (i) =>
      i.holdingIds.includes(holding.id) &&
      ["quarantinedTrades", "pendingCorporateAction"].includes(i.code),
  );
  if (
    !needsReview &&
    !declaring &&
    !error &&
    !pending.length &&
    !applied.length &&
    !history?.quarantine
  )
    return null;
  return (
    <section
      className="investment-help financial-history"
      aria-label="数据核对"
    >
      {error && <p role="alert">{error}</p>}
      {pending.map((action) => (
        <section key={action.id}>
          <p>
            拆合股比例：{action.split.effectiveOn}，每{" "}
            {action.split.denominator} 股变为 {action.split.numerator} 股。
          </p>
          {action.sourceUrl && (
            <a
              href={action.sourceUrl}
              target="_blank"
              rel="noopener noreferrer"
            >
              核对行情来源
            </a>
          )}
          <label>
            核对依据
            <input value={reason} onChange={(e) => setReason(e.target.value)} />
          </label>
          <button
            disabled={busy || !reason.trim()}
            onClick={async () => {
              setBusy(true);
              setError("");
              try {
                onUpdate(
                  await confirmCorporateAction(
                    action.id,
                    snapshot.holdingRevisions?.[holding.id] ?? "",
                    reason,
                  ),
                );
              } catch (e) {
                setError(String(e));
              } finally {
                setBusy(false);
              }
            }}
          >
            确认比例并重建持仓
          </button>
          <button
            disabled={busy || !reason.trim()}
            onClick={async () => {
              setBusy(true);
              setError("");
              try {
                onUpdate(
                  await confirmCorporateAction(
                    action.id,
                    snapshot.holdingRevisions?.[holding.id] ?? "",
                    reason,
                    "reject",
                  ),
                );
              } catch (e) {
                setError(String(e));
              } finally {
                setBusy(false);
              }
            }}
          >
            来源有误，否决此记录
          </button>
          <p>
            总成本与现金保持不变，原始成交不改写；确认后请重新刷新行情。零碎股与现金替代分配需另行核对。
          </p>
        </section>
      ))}
      {applied.map((action) => (
        <section key={action.id}>
          <p>
            已确认公司行动：{action.split.effectiveOn} ·{" "}
            {action.split.denominator} 股变为 {action.split.numerator} 股
          </p>
          <label>
            撤回依据
            <input value={reason} onChange={(e) => setReason(e.target.value)} />
          </label>
          <button
            disabled={busy || !reason.trim()}
            onClick={async () => {
              setBusy(true);
              setError("");
              try {
                onUpdate(
                  await confirmCorporateAction(
                    action.id,
                    snapshot.holdingRevisions?.[holding.id] ?? "",
                    reason,
                    "revert",
                  ),
                );
              } catch (e) {
                setError(String(e));
              } finally {
                setBusy(false);
              }
            }}
          >
            撤回此调整并重建
          </button>
        </section>
      ))}
      {declaring && (
        <form aria-label="登记拆合股" onSubmit={(e) => e.preventDefault()}>
          <p>
            适用于未上市证券或行情缺少公司行动。原始成交保留，补充后仍需单独核对确认。零碎股现金分配须按实际单据处理。
          </p>
          <fieldset disabled={busy}>
            <label>
              生效日期
              <input
                type="date"
                value={effectiveOn}
                onChange={(e) => setEffectiveOn(e.target.value)}
              />
            </label>
            <label>
              原股数
              <input
                inputMode="decimal"
                value={denominator}
                onChange={(e) => setDenominator(e.target.value)}
              />
            </label>
            <label>
              新股数
              <input
                inputMode="decimal"
                value={numerator}
                onChange={(e) => setNumerator(e.target.value)}
              />
            </label>
            <label>
              券商通知与核对依据
              <input
                value={reason}
                onChange={(e) => setReason(e.target.value)}
              />
            </label>
            <button
              disabled={
                !effectiveOn || !numerator || !denominator || !reason.trim()
              }
              onClick={async () => {
                setBusy(true);
                setError("");
                try {
                  onUpdate(
                    await declareCorporateAction(holding.id, {
                      requestId,
                      expectedRevision:
                        snapshot.holdingRevisions?.[holding.id] ?? "",
                      split: { effectiveOn, numerator, denominator },
                      reason,
                    }),
                  );
                  setRequestId(crypto.randomUUID());
                  onDeclareDone();
                } catch (e) {
                  setError(String(e));
                } finally {
                  setBusy(false);
                }
              }}
            >
              登记待核对公司行动
            </button>
            <button type="button" onClick={onDeclareDone}>
              取消
            </button>
          </fieldset>
        </form>
      )}
      {history?.quarantine && (
        <section>
          <p role="alert">
            {history.quarantine.reason}
            。当前余额和原始记录均已保留。修复不会重放历史现金流，完成后请核对存款。
          </p>
          <details>
            <summary>查看隔离原文</summary>
            <pre>{history.quarantine.payload}</pre>
          </details>
          <label>
            修复候选交易 JSON
            <textarea
              rows={12}
              value={candidate}
              onChange={(e) => {
                setCandidate(e.target.value);
                setPreview(null);
              }}
            />
          </label>
          <label>
            核对依据与修复原因
            <input
              value={reason}
              onChange={(e) => {
                setReason(e.target.value);
                setPreview(null);
              }}
            />
          </label>
          <button
            disabled={busy || !reason.trim()}
            onClick={() => void run(false)}
          >
            预览修复结果
          </button>
          {preview && (
            <div>
              <p>
                恢复数量 {preview.quantity} · 成本 {preview.costBasis} ·
                已结算盈亏 {preview.settledProfit} {holding.currency}
                。存款保持当前余额。
              </p>
              <button disabled={busy} onClick={() => void run(true)}>
                确认修复并保留原文
              </button>
            </div>
          )}
        </section>
      )}
    </section>
  );
}
