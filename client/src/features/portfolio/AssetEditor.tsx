import { PrivateNumberInput } from "../../components/AmountPrivacy";
import { Button, Input } from "antd";
import { useRef, useState } from "react";
import {
  saveHolding,
  updateHolding,
  updateHoldingAmount,
} from "../../api/portfolio";
import type { Holding, Snapshot } from "../../types/user";
import { localDateValue } from "../../lib/dates";
import {
  HoldingFields,
  hasHoldingDetails,
  holdingExactValues,
  validHoldingDetails,
} from "./HoldingFields";

const currencies = ["CNY", "USD", "HKD", "EUR", "JPY", "GBP"];
const assetClasses = ["股票", "基金", "债券", "黄金", "其他"];

function initialAmount(snapshot: Snapshot, holding: Holding) {
  if (holding.assetClass === "现金" && !holding.id) return "";
  return (
    snapshot.cashBalances?.find((b) => b.holdingId === holding.id)?.amount ??
    String(holding.marketValue)
  );
}

function initialCashDate(snapshot: Snapshot, holding: Holding) {
  if (!holding.id) return holding.valuationDate || localDateValue(new Date());
  return (
    snapshot.cashBalances?.find((b) => b.holdingId === holding.id)
      ?.confirmedOn ?? holding.valuationDate
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
  const initialDate =
    initial.assetClass === "现金"
      ? initialCashDate(snapshot, initial)
      : initial.valuationDate;
  const [editing, setEditing] = useState<Holding>(() =>
    initial.assetClass === "现金"
      ? { ...initial, valuationDate: initialDate }
      : initial,
  );
  const [assetAmount, setAssetAmount] = useState(() =>
    initialAmount(snapshot, initial),
  );
  const [assetCost, setAssetCost] = useState(
    initial.costBasis > 0 ? String(initial.costBasis) : "",
  );
  const cashRevision = useRef(snapshot.holdingRevisions?.[initial.id] ?? "");
  const cash = editing.assetClass === "现金";
  const cashMetadataChanged =
    cash &&
    (editing.name.trim() !== initial.name.trim() ||
      editing.currency !== initial.currency ||
      editing.valuationDate !== initialDate);
  const tradeManaged = Boolean(
    snapshot.holdingTrades?.some((t) => t.holdingId === editing.id),
  );
  const manualAssetAmount =
    cash || Boolean(editing.id && !hasHoldingDetails(editing));
  const today = localDateValue(new Date());
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
    (cash
      ? !editing.valuationDate || editing.valuationDate <= today
      : editing.valuationDate && editing.valuationDate <= today),
  );

  const save = () => {
    if (busy || !valid) return;
    void run(async () => {
      const { id, ...values } = editing;
      if (id && cash && !cashMetadataChanged) {
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
      onSaved(cash ? "存款账户已保存" : "持仓已保存");
    });
  };

  return (
    <form
      className="portfolio-form asset-editor"
      onSubmit={(e) => {
        e.preventDefault();
        save();
      }}
    >
      <fieldset className="holding-fields" disabled={busy}>
        <div className="form-grid">
          {cash && (
            <>
              <label>
                <span>账户名称</span>
                <Input
                  autoFocus
                  required
                  maxLength={200}
                  value={editing.name}
                  placeholder="例如：微信零钱、招商银行、公积金账户"
                  onChange={(e) =>
                    setEditing({ ...editing, name: e.target.value })
                  }
                />
              </label>
              <label>
                <span>币种</span>
                <select
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
                  {[...new Set([...currencies, editing.currency])].map((c) => (
                    <option key={c}>{c}</option>
                  ))}
                </select>
              </label>
            </>
          )}
          {!cash && (
            <>
              <label>
                <span>名称</span>
                <Input
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
                <span>证券代码（自动价格，选填）</span>
                <Input
                  value={editing.symbol}
                  placeholder="AAPL / 0700.HK / 600519.SS / 000001.SZ"
                  onChange={(e) =>
                    setEditing({ ...editing, symbol: e.target.value })
                  }
                />
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
              <PrivateNumberInput
                data-money-input="true"
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
                      ? String(editing.quantity * editing.currentPrice)
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
          {cash && (
            <label>
              <span>核对日期</span>
              <Input
                type="date"
                max={today}
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
          )}
          {!cash && (
            <div className="asset-cost-field">
              <label>
                <span>持仓总成本（选填）</span>
                <div className="input-affix">
                  <i>{editing.currency}</i>
                  <PrivateNumberInput
                    data-money-input="true"
                    type="number"
                    min={0}
                    max={1e15}
                    inputMode="decimal"
                    step="any"
                    placeholder="未填写"
                    readOnly={tradeManaged || editing.costPrice != null}
                    value={
                      editing.costPrice != null
                        ? holdingExactValues(editing).costBasis
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
                <Input
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
        <div className="form-actions">
          <Button type="primary" htmlType="submit" disabled={busy || !valid}>
            {busy ? "保存中…" : "保存这笔资产"}
          </Button>
        </div>
      </fieldset>
    </form>
  );
}
