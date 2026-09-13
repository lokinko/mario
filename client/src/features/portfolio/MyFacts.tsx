import { AssetAmountRow } from "./AssetAmountRow";
import { AssetHistory } from "./AssetHistory";
import { useState } from "react";
import { ArrowRight, Plus, Save, X } from "lucide-react";
import { saveHolding, updateHolding, saveProfile } from "../../api";
import type { Holding, Snapshot } from "../../types";
import { supportingNav, type View } from "../../app/navigation";
import { PageHeader } from "../../components/PageHeader";
import { NumberField } from "../../components/NumberField";
import { formatMoney } from "../../lib/format";
import { localDateValue } from "../../lib/dates";
import { emptyHolding } from "./forms";

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

  const saveIncome = async () => {
    setBusy(true);
    setError("");
    try {
      onUpdate(await saveProfile(profile));
      flash("收支已保存，可以继续聊了");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const saveAsset = async () => {
    if (
      !editing ||
      !assetAmount.trim() ||
      !Number.isFinite(Number(assetAmount)) ||
      Number(assetAmount) < 0
    )
      return;
    setBusy(true);
    setError("");
    try {
      const { id, ...values } = editing;
      const input = { ...values, name: values.name.trim() };
      onUpdate(id ? await updateHolding(id, input) : await saveHolding(input));
      setEditing(null);
      flash("已保存你的实际余额");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const add = () => {
    setAssetAmount("");
    setEditing({
      ...emptyHolding(currency),
      id: "",
      name: section === "savings" ? "银行存款" : "",
      assetClass: section === "savings" ? "现金" : "其他",
    });
    setError("");
  };
  return (
    <div className="page narrow facts-page">
      <PageHeader
        title="我的情况"
        description="填你确定的，暂时不清楚的可以先留空。"
      />
      <AssetHistory snapshot={snapshot} navigate={navigate} />
      <p className="facts-intro">
        你记下实际的收支和资产，mario
        在问答时查资料、整理分析，有需要再向你了解。
      </p>
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
            disabled={busy}
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
          {error}。输入已保留，请重试。
        </div>
      )}
      {section === "income" ? (
        <section className="panel form-panel">
          <h2>每个月大概收支多少？</h2>
          <p className="section-intro">
            金额单位：{currency}。大致金额就可以，以后随时修改。
          </p>
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
              <button className="primary" onClick={() => void saveIncome()}>
                <Save size={16} />
                {busy ? "保存中…" : "保存收支"}
              </button>
            </div>
          </fieldset>
        </section>
      ) : (
        <section className="panel form-panel">
          <div className="panel-title">
            <h2>
              {section === "savings"
                ? "现在有多少存款？"
                : "已经持有哪些投资？"}
            </h2>
            <button
              className="secondary"
              disabled={busy || Boolean(editing)}
              onClick={add}
            >
              <Plus size={16} />
              {section === "savings" ? "添加存款" : "添加持仓"}
            </button>
          </div>
          <p className="section-intro">
            {section === "savings"
              ? "按账户记录余额即可。这里的存款也计入你的资产，不用在持仓中重复填写。"
              : "填名称和当前金额即可。没有投资也没关系，可以直接开始问答。"}
          </p>
          <div className="facts-assets">
            {(section === "savings" ? cash : investments).map((item) => (
              <AssetAmountRow
                key={item.id}
                holding={item}
                revision={snapshot.holdingRevisions?.[item.id]}
                onUpdate={onUpdate}
                onEdit={() => {
                  setAssetAmount(String(item.marketValue));
                  setEditing({ ...item });
                  setError("");
                }}
              />
            ))}
            {(section === "savings" ? cash : investments).length === 0 && (
              <p className="empty">还没有记录，想好了再填也可以。</p>
            )}
          </div>
        </section>
      )}
      {editing && (
        <section className="panel form-panel" aria-label="记录资产">
          <div className="panel-title">
            <h2>
              {editing.id
                ? "更新实际余额"
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
                <label>
                  <span>当前金额</span>
                  <div className="input-affix">
                    <i>{editing.currency}</i>
                    <input
                      type="number"
                      inputMode="decimal"
                      step="any"
                      value={assetAmount}
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
                      <span>类别（不确定可选其他）</span>
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
                <NumberField
                  label={`折算汇率（1 ${editing.currency} 对应 ${currency}）`}
                  value={editing.fxRateToBase ?? 0}
                  onChange={(v) =>
                    setEditing({
                      ...editing,
                      fxRateToBase: Number(v) || null,
                      fxRateSource: "",
                      fxRateObservedOn: "",
                    })
                  }
                />
              )}
              <div className="form-actions">
                <button
                  className="primary"
                  type="submit"
                  disabled={
                    !editing.name.trim() ||
                    !assetAmount.trim() ||
                    !Number.isFinite(editing.marketValue) ||
                    editing.marketValue < 0 ||
                    editing.marketValue > 1e15 ||
                    (editing.fxRateToBase != null &&
                      (!Number.isFinite(editing.fxRateToBase) ||
                        editing.fxRateToBase <= 0))
                  }
                >
                  {busy ? "保存中…" : "保存这笔资产"}
                </button>
              </div>
            </fieldset>
          </form>
        </section>
      )}
      <div className="facts-next">
        <p>先填一项也可以，不用完成整份档案。</p>
        <button className="primary" onClick={() => navigate("advisor")}>
          去聊聊 <ArrowRight size={16} />
        </button>
      </div>
      <details className="facts-advanced">
        <summary>更多资料与记录</summary>
        <p>需要管理目标、核对流水或复盘时，从这里打开。</p>
        <div className="archive-links">
          {supportingNav.map((item) => (
            <button
              className="secondary"
              key={item.id}
              onClick={() => navigate(item.id)}
            >
              {item.label}
            </button>
          ))}
        </div>
      </details>
    </div>
  );
}
