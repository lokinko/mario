import { useId, useRef, useState } from "react";
import {
  saveHolding,
  updateHolding,
  updateHoldingAmount,
} from "../../api/portfolio";
import type { Holding, Snapshot } from "../../types";
import { localDateValue } from "../../lib/dates";
import { formatNumber } from "../../lib/format";
import {
  HoldingFields,
  hasHoldingDetails,
  holdingExactValues,
  validHoldingDetails,
} from "./HoldingFields";

const currencies = ["CNY", "USD", "HKD", "EUR", "JPY", "GBP"];
const assetClasses = ["股票", "基金", "债券", "黄金", "其他"];

function initialAmount(snapshot: Snapshot, holding: Holding) {
  if (!holding.id && holding.assetClass !== "现金") return "";
  return (
    snapshot.cashBalances?.find((b) => b.holdingId === holding.id)?.amount ??
    String(holding.marketValue)
  );
}

export function AssetEditor({
  snapshot,
  initial,
  busy,
  run,
  cashAttemptId,
  onUpdate,
  onSaved,
}: {
  snapshot: Snapshot;
  initial: Holding;
  busy: boolean;
  run: (task: () => Promise<void>) => Promise<void>;
  /** Stable per (holding, amount, revision) so a retried save replays the same request. */
  cashAttemptId: (key: string) => string;
  onUpdate: (snapshot: Snapshot) => void;
  onSaved: (message: string) => void;
}) {
  const [editing, setEditing] = useState(initial);
  const [assetAmount, setAssetAmount] = useState(() =>
    initialAmount(snapshot, initial),
  );
  const [assetCost, setAssetCost] = useState(
    initial.costBasis > 0 ? String(initial.costBasis) : "",
  );
  const cashRevision = useRef(snapshot.holdingRevisions?.[initial.id] ?? "");
  const costHelpId = useId();
  const codeHelpId = useId();
  const cash = editing.assetClass === "现金";
  const tradeManaged = Boolean(
    snapshot.holdingTrades?.some((t) => t.holdingId === editing.id),
  );
  const manualAssetAmount =
    cash || Boolean(editing.id && !hasHoldingDetails(editing));
  const valid = Boolean(
    editing.name.trim() &&
    (manualAssetAmount ? assetAmount.trim() : hasHoldingDetails(editing)) &&
    validHoldingDetails(editing) &&
    Number.isFinite(editing.marketValue) &&
    editing.marketValue >= 0 &&
    editing.marketValue <= 1e15 &&
    Number.isFinite(editing.costBasis) &&
    editing.costBasis >= 0 &&
    editing.costBasis <= 1e15 &&
    (cash ||
      (editing.valuationDate &&
        editing.valuationDate <= localDateValue(new Date()))),
  );

  const save = () => {
    if (busy || !valid) return;
    void run(async () => {
      const { id, ...values } = editing;
      if (id && cash) {
        const key = JSON.stringify([id, assetAmount, cashRevision.current]);
        onUpdate(
          await updateHoldingAmount(
            id,
            assetAmount,
            cashRevision.current,
            cashAttemptId(key),
          ),
        );
      } else {
        const input = {
          ...values,
          name: values.name.trim(),
          ...(cash
            ? { cashAmount: assetAmount }
            : {
                exactValues: {
                  ...holdingExactValues(editing),
                  ...(manualAssetAmount
                    ? { marketValue: assetAmount || "0" }
                    : {}),
                  ...(!tradeManaged && editing.costPrice == null
                    ? { costBasis: assetCost || "0" }
                    : {}),
                },
              }),
        };
        onUpdate(
          id ? await updateHolding(id, input) : await saveHolding(input),
        );
      }
      onSaved(cash ? "已保存你的实际余额" : "持仓已保存");
    });
  };

  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        save();
      }}
    >
      <fieldset className="holding-fields" disabled={busy}>
        <div className="form-grid">
          {cash ? (
            <p>
              {editing.currency} 存款 · 填写当前余额，之后的交易会自动结算。
            </p>
          ) : (
            <>
              <label>
                <span>名称</span>
                <input
                  autoFocus
                  required
                  value={editing.name}
                  placeholder="例如：工资卡、某只基金"
                  onChange={(e) =>
                    setEditing({ ...editing, name: e.target.value })
                  }
                />
              </label>
              <label>
                <span id={`${codeHelpId}-label`}>
                  证券代码（自动价格，选填）
                </span>
                <input
                  aria-labelledby={`${codeHelpId}-label`}
                  aria-describedby={tradeManaged ? codeHelpId : undefined}
                  value={editing.symbol}
                  placeholder="AAPL / 0700.HK / 600519.SS / 000001.SZ"
                  onChange={(e) =>
                    setEditing({ ...editing, symbol: e.target.value })
                  }
                />
                {tradeManaged && (
                  <small id={codeHelpId}>
                    可补填或纠正同一证券的代码；如需记录另一只证券，请新建持仓。
                  </small>
                )}
              </label>
              <HoldingFields
                tradeManaged={tradeManaged}
                holding={editing}
                onChange={(next) => {
                  setEditing(next);
                  setAssetAmount(String(next.marketValue));
                  setAssetCost(
                    next.costBasis === 0 ? "" : String(next.costBasis),
                  );
                }}
              />
            </>
          )}
          <label>
            <span>{cash ? "当前金额" : "当前市值"}</span>
            <div className="input-affix">
              <i>{editing.currency}</i>
              <input
                type="number"
                required={manualAssetAmount}
                readOnly={!manualAssetAmount}
                placeholder={
                  manualAssetAmount
                    ? "手动输入"
                    : "根据持仓数和当前价格自动计算"
                }
                min={0}
                max={1e15}
                inputMode="decimal"
                step="any"
                value={
                  manualAssetAmount
                    ? assetAmount
                    : editing.quantity != null && editing.currentPrice != null
                      ? formatNumber(editing.quantity * editing.currentPrice)
                      : ""
                }
                onChange={(e) => {
                  setAssetAmount(e.target.value);
                  setEditing({
                    ...editing,
                    marketValue: Number(e.target.value),
                  });
                }}
              />
            </div>
          </label>
          {!cash && (
            <div className="asset-cost-field">
              <label>
                <span>持仓总成本（选填）</span>
                <div className="input-affix">
                  <i>{editing.currency}</i>
                  <input
                    type="number"
                    min={0}
                    max={1e15}
                    inputMode="decimal"
                    step="any"
                    placeholder="未填写"
                    aria-describedby={costHelpId}
                    readOnly={tradeManaged || editing.costPrice != null}
                    value={
                      editing.costPrice != null
                        ? formatNumber(editing.costBasis)
                        : assetCost
                    }
                    onChange={(e) => {
                      setAssetCost(e.target.value);
                      setEditing({
                        ...editing,
                        costBasis: Number(e.target.value),
                      });
                    }}
                  />
                </div>
              </label>
              <small id={costHelpId}>
                {tradeManaged
                  ? "数量与成本由交易明细计算，请通过录入或编辑交易调整。当前价格可在下方更新。"
                  : "成本价为当前持仓每单位的平均买入成本（可含买入费用）；填入成本价和数量后自动计算总成本。旧记录也可直接填写总成本。"}
              </small>
            </div>
          )}
        </div>
        {!cash && (
          <details className="holding-details">
            <summary>
              币种、日期等（{editing.valuationDate} / {editing.currency}）
            </summary>
            <div className="form-grid">
              <label>
                <span>币种</span>
                <select
                  disabled={tradeManaged}
                  value={editing.currency}
                  onChange={(e) =>
                    setEditing({
                      ...editing,
                      currency: e.target.value,
                      fxRateToBase: null,
                      fxRateSource: "",
                      fxRateObservedOn: "",
                    })
                  }
                >
                  {currencies.map((c) => (
                    <option key={c}>{c}</option>
                  ))}
                </select>
              </label>
              <label>
                <span>余额日期</span>
                <input
                  required
                  type="date"
                  max={localDateValue(new Date())}
                  value={editing.valuationDate}
                  onChange={(e) =>
                    setEditing({
                      ...editing,
                      valuationDate: e.target.value,
                      fxRateToBase: null,
                      fxRateSource: "",
                      fxRateObservedOn: "",
                    })
                  }
                />
              </label>
              <label>
                <span>类别</span>
                <select
                  disabled={tradeManaged}
                  value={editing.assetClass}
                  onChange={(e) =>
                    setEditing({
                      ...editing,
                      assetClass: e.target.value as Holding["assetClass"],
                    })
                  }
                >
                  {assetClasses.map((c) => (
                    <option key={c}>{c}</option>
                  ))}
                </select>
              </label>
            </div>
          </details>
        )}
        {editing.currency !== snapshot.profile.baseCurrency && (
          <p className="section-intro">
            汇率自动读取；如需调整，可在右侧“汇率”浮窗的“管理汇率”中修改。
          </p>
        )}
        <div className="form-actions">
          <button className="primary" type="submit" disabled={!valid}>
            {busy ? "保存中…" : "保存这笔资产"}
          </button>
        </div>
      </fieldset>
    </form>
  );
}
