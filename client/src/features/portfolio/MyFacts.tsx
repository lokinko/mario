import { ExchangeRates } from "./ExchangeRates";
import { AssetAmountRow } from "./AssetAmountRow";
import { AssetHistory } from "./AssetHistory";
import { useId, useState } from "react";
import { ArrowRight, Plus, Save, X } from "lucide-react";
import {
  deleteHolding,
  saveHolding,
  updateHolding,
  saveProfile,
} from "../../api";
import type { Holding, Snapshot } from "../../types";
import { type View } from "../../app/navigation";
import { PageHeader } from "../../components/PageHeader";
import { NumberField } from "../../components/NumberField";
import { localDateValue } from "../../lib/dates";
import { emptyHolding } from "./forms";
import {
  HoldingFields,
  hasHoldingDetails,
  validHoldingDetails,
} from "./HoldingFields";

export function MyFacts({
  snapshot,
  onUpdate,
  navigate,
  flash,
}: {
  snapshot: Snapshot;
  onUpdate: (snapshot: Snapshot) => void;
  navigate: (view: View) => void;
  flash: (message: string) => void;
}) {
  const [profile, setProfile] = useState(snapshot.profile);
  const [editing, setEditing] = useState<Holding | null>(null);
  const [assetAmount, setAssetAmount] = useState("");
  const [assetCost, setAssetCost] = useState("");
  const costHelpId = useId();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [section, setSection] = useState<"income" | "savings" | "holdings">(
    snapshot.holdings.some((h) => h.assetClass !== "现金")
      ? "holdings"
      : snapshot.holdings.length
        ? "savings"
        : "income",
  );
  const cash = snapshot.holdings.filter((item) => item.assetClass === "现金");
  const investments = snapshot.holdings.filter(
    (item) => item.assetClass !== "现金",
  );
  const currency = snapshot.profile.baseCurrency;
  const validIncome = [
    profile.monthlyIncome,
    profile.monthlyExpense,
    profile.liabilities,
    profile.emergencyFund,
  ].every((value) => Number.isFinite(value) && value >= 0 && value <= 1e15);
  const validAsset = Boolean(
    editing &&
    editing.name.trim() &&
    (assetAmount.trim() || hasHoldingDetails(editing)) &&
    validHoldingDetails(editing) &&
    Number.isFinite(editing.marketValue) &&
    editing.marketValue >= 0 &&
    editing.marketValue <= 1e15 &&
    Number.isFinite(editing.costBasis) &&
    editing.costBasis >= 0 &&
    editing.costBasis <= 1e15 &&
    editing.valuationDate &&
    editing.valuationDate <= localDateValue(new Date()),
  );

  const saveIncome = async () => {
    if (busy || !validIncome) return;
    setBusy(true);
    setError("");
    try {
      onUpdate(await saveProfile(profile));
      flash("收支已保存");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const saveAsset = async () => {
    if (!editing || busy || !validAsset) return;
    setBusy(true);
    setError("");
    try {
      const { id, ...values } = editing;
      const input = { ...values, name: values.name.trim() };
      onUpdate(id ? await updateHolding(id, input) : await saveHolding(input));
      setEditing(null);
      flash(
        editing.assetClass === "现金" ? "已保存你的实际余额" : "持仓已保存",
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const add = () => {
    setAssetAmount("");
    setAssetCost("");
    setEditing({
      ...emptyHolding(currency),
      id: "",
      name: section === "savings" ? "银行存款" : "",
      assetClass: section === "savings" ? "现金" : "其他",
    });
    setError("");
  };
  const removeAsset = async (item: Holding) => {
    if (busy) return;
    if (!window.confirm(`确认删除“${item.name}”？相关决策日志不会被删除。`))
      return;
    setBusy(true);
    setError("");
    try {
      onUpdate(await deleteHolding(item.id));
      if (editing?.id === item.id) setEditing(null);
      flash("资产已删除");
    } catch (e) {
      setError(`删除失败：${String(e)}`);
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="page narrow facts-page">
      <PageHeader title="我的情况" />
      <AssetHistory snapshot={snapshot} />
      <div className="section-switcher" role="group" aria-label="我的情况分类">
        {(
          [
            ["income", "收入与支出"],
            ["savings", "存款与现金"],
            ["holdings", "投资持仓"],
          ] as const
        ).map(([id, label]) => (
          <button
            key={id}
            disabled={busy || Boolean(editing)}
            aria-pressed={section === id}
            onClick={() => {
              setSection(id);
              setError("");
            }}
          >
            {label}
          </button>
        ))}
      </div>
      {error && (
        <div className="error-box" role="alert">
          {error}。请重试。
        </div>
      )}
      {section === "income" ? (
        <section className="panel form-panel">
          <h2>收入与支出</h2>
          {!validIncome && (
            <p className="warning-text" role="alert">
              请核对金额，输入有效的非负数。
            </p>
          )}
          <fieldset className="holding-fields" disabled={busy}>
            <div className="form-grid">
              <NumberField
                label="月收入"
                value={profile.monthlyIncome}
                onChange={(v) =>
                  setProfile({ ...profile, monthlyIncome: Number(v) })
                }
                prefix={currency}
              />
              <NumberField
                label="月支出"
                value={profile.monthlyExpense}
                onChange={(v) =>
                  setProfile({ ...profile, monthlyExpense: Number(v) })
                }
                prefix={currency}
              />
            </div>
            <details className="holding-details">
              <summary>有负债或预留备用金？（选填）</summary>
              <div className="form-grid">
                <NumberField
                  label="尚未还清的负债"
                  value={profile.liabilities}
                  onChange={(v) =>
                    setProfile({ ...profile, liabilities: Number(v) })
                  }
                  prefix={currency}
                />
                <NumberField
                  label="存款中预留的备用金"
                  value={profile.emergencyFund}
                  onChange={(v) =>
                    setProfile({ ...profile, emergencyFund: Number(v) })
                  }
                  prefix={currency}
                />
              </div>
              <p className="section-intro">
                备用金是存款中留作急用的部分，不会再次计入资产。
              </p>
            </details>
            <div className="form-actions">
              <button
                className="primary"
                disabled={!validIncome}
                onClick={() => void saveIncome()}
              >
                <Save size={16} />
                {busy ? "保存中…" : "保存收支"}
              </button>
            </div>
          </fieldset>
        </section>
      ) : (
        <section className="panel form-panel">
          <div className="panel-title">
            <h2>{section === "savings" ? "存款与现金" : "投资持仓"}</h2>
            <button
              className="secondary"
              disabled={busy || Boolean(editing)}
              onClick={add}
            >
              <Plus size={16} />
              {section === "savings" ? "添加存款" : "添加持仓"}
            </button>
          </div>
          <div className="facts-assets">
            {section === "holdings" && investments.length > 0 && (
              <div className="investment-amount-head" aria-hidden="true">
                <span>资产</span>
                <span>成本价</span>
                <span>持仓数</span>
                <span>当前价格</span>
                <span>当前市值</span>
                <span>浮动盈亏</span>
              </div>
            )}
            {(section === "savings" ? cash : investments).map((item) => (
              <AssetAmountRow
                key={item.id}
                holding={item}
                revision={snapshot.holdingRevisions?.[item.id]}
                onUpdate={onUpdate}
                disabled={busy}
                onDelete={() => void removeAsset(item)}
                onEdit={() => {
                  setAssetAmount(String(item.marketValue));
                  setAssetCost(
                    item.costBasis > 0 ? String(item.costBasis) : "",
                  );
                  setEditing({ ...item });
                  setError("");
                }}
              />
            ))}
            {(section === "savings" ? cash : investments).length === 0 && (
              <p className="empty">暂无资产，点击上方添加。</p>
            )}
          </div>
          {section === "holdings" && (
            <p className="section-intro">
              成本价、持仓数和当前价格均手动输入；持仓数与当前价格需一起填写，成本价可稍后补充。市值
              = 持仓数 × 当前价格， 总成本 = 持仓数 ×
              成本价；价格以持仓币种计价。
              浮动盈亏不含已实现盈亏、分红及卖出税费。旧记录可保留总金额，补充数量和价格后自动计算。
            </p>
          )}
        </section>
      )}
      {editing && (
        <section className="panel form-panel" aria-label="记录资产">
          <div className="panel-title">
            <h2>
              {editing.id
                ? editing.assetClass === "现金"
                  ? "更新实际余额"
                  : "更新持仓"
                : editing.assetClass === "现金"
                  ? "记一笔存款"
                  : "记一笔持仓"}
            </h2>
            <button
              className="text-button"
              disabled={busy}
              aria-label="取消记录资产"
              onClick={() => setEditing(null)}
            >
              <X size={18} />
            </button>
          </div>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void saveAsset();
            }}
          >
            <fieldset className="holding-fields" disabled={busy}>
              <div className="form-grid">
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
                {editing.assetClass !== "现金" && (
                  <HoldingFields
                    holding={editing}
                    onChange={(next) => {
                      setEditing(next);
                      setAssetAmount(String(next.marketValue));
                      setAssetCost(
                        next.costBasis === 0 ? "" : String(next.costBasis),
                      );
                    }}
                  />
                )}
                <label>
                  <span>
                    {editing.assetClass === "现金" ? "当前金额" : "当前市值"}
                  </span>
                  <div className="input-affix">
                    <i>{editing.currency}</i>
                    <input
                      type="number"
                      required={!hasHoldingDetails(editing)}
                      readOnly={hasHoldingDetails(editing)}
                      min={0}
                      max={1e15}
                      inputMode="decimal"
                      step="any"
                      value={
                        hasHoldingDetails(editing)
                          ? editing.marketValue
                          : assetAmount
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
                {editing.assetClass !== "现金" && (
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
                          readOnly={editing.costPrice != null}
                          value={
                            editing.costPrice != null
                              ? editing.costBasis
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
                      成本价为当前持仓每单位的平均买入成本（可含买入费用）；填入成本价和数量后自动计算总成本。旧记录也可直接填写总成本。
                    </small>
                  </div>
                )}
              </div>
              <details className="holding-details">
                <summary>
                  币种、日期等（{editing.valuationDate} / {editing.currency}）
                </summary>
                <div className="form-grid">
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
                      {["CNY", "USD", "HKD", "EUR", "JPY", "GBP"].map((c) => (
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
                  {editing.assetClass !== "现金" && (
                    <label>
                      <span>类别</span>
                      <select
                        value={editing.assetClass}
                        onChange={(e) =>
                          setEditing({
                            ...editing,
                            assetClass: e.target.value as Holding["assetClass"],
                          })
                        }
                      >
                        {["股票", "基金", "债券", "黄金", "其他"].map((c) => (
                          <option key={c}>{c}</option>
                        ))}
                      </select>
                    </label>
                  )}
                </div>
              </details>
              {editing.currency !== currency && (
                <p className="section-intro">
                  汇率自动读取；如需调整，可在下方“汇率设置”统一修改。
                </p>
              )}
              <div className="form-actions">
                <button
                  className="primary"
                  type="submit"
                  disabled={!validAsset}
                >
                  {busy ? "保存中…" : "保存这笔资产"}
                </button>
              </div>
            </fieldset>
          </form>
        </section>
      )}
      <ExchangeRates baseCurrency={currency} onUpdate={onUpdate} />
      <div className="facts-next">
        <button className="primary" onClick={() => navigate("advisor")}>
          去聊聊 <ArrowRight size={16} />
        </button>
      </div>
    </div>
  );
}
