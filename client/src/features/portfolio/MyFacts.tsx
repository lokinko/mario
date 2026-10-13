import { ExchangeRates } from "./ExchangeRates";
import { AssetAmountRow } from "./AssetAmountRow";
import { AssetHistory } from "./AssetHistory";
import { HoldingTrades } from "./HoldingTrades";
import { InvestmentOverview } from "./InvestmentOverview";
import { isClosedHolding, sortInvestmentHoldings } from "./holdingMetrics";
import { useId, useState } from "react";
import { ArrowRight, Plus, RefreshCw, Save, X } from "lucide-react";
import {
  deleteHolding,
  saveHolding,
  updateHolding,
  saveProfile,
  refreshSecurityPrices,
} from "../../api/portfolio";
import type { Holding, Snapshot } from "../../types";
import { type View } from "../../app/navigation";
import { PageHeader } from "../../components/PageHeader";
import { NumberField } from "../../components/NumberField";
import { localDateValue } from "../../lib/dates";
import { formatNumber } from "../../lib/format";
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
  const [addingTrade, setAddingTrade] = useState(false);
  const [positionFilter, setPositionFilter] = useState<
    "all" | "open" | "closed"
  >("all");
  const tradeManaged = Boolean(
    editing && snapshot.holdingTrades?.some((t) => t.holdingId === editing.id),
  );
  const [assetAmount, setAssetAmount] = useState("");
  const [assetCost, setAssetCost] = useState("");
  const costHelpId = useId();
  const codeHelpId = useId();
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
  const investments = sortInvestmentHoldings(
    snapshot.holdings.filter((item) => item.assetClass !== "现金"),
    snapshot,
  );
  const currency = snapshot.profile.baseCurrency;
  const closedInvestments = investments.filter((item) =>
    isClosedHolding(item, Boolean(snapshot.holdingTradeSummaries?.[item.id])),
  );
  const effectivePositionFilter = closedInvestments.length
    ? positionFilter
    : "all";
  const visibleInvestments = investments.filter((item) => {
    const closed = isClosedHolding(
      item,
      Boolean(snapshot.holdingTradeSummaries?.[item.id]),
    );
    return (
      effectivePositionFilter === "all" ||
      (effectivePositionFilter === "closed" ? closed : !closed)
    );
  });
  const validIncome = [
    profile.monthlyIncome,
    profile.monthlyExpense,
    profile.liabilities,
    profile.emergencyFund,
  ].every((value) => Number.isFinite(value) && value >= 0 && value <= 1e15);
  const manualAssetAmount = Boolean(
    editing &&
    (editing.assetClass === "现金" ||
      (editing.id && !hasHoldingDetails(editing))),
  );
  const validAsset = Boolean(
    editing &&
    editing.name.trim() &&
    (manualAssetAmount ? assetAmount.trim() : hasHoldingDetails(editing)) &&
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
    if (!window.confirm(`确认删除“${item.name}”？`)) return;
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
      <ExchangeRates
        baseCurrency={currency}
        usedCurrencies={[
          ...snapshot.holdings.map((holding) => holding.currency),
          ...(snapshot.holdingTrades ?? []).map((trade) => trade.currency),
        ]}
        onUpdate={onUpdate}
      />
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
            disabled={busy || Boolean(editing) || addingTrade}
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
      {section === "holdings" &&
        Boolean(snapshot.priceRefreshErrors?.length) && (
          <p className="warning-text" role="status">
            部分价格更新失败，已保留原值：
            {snapshot.priceRefreshErrors?.join("；")}
          </p>
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
        <section
          className={`panel form-panel${section === "holdings" ? " investment-panel" : ""}`}
        >
          <div className="panel-title">
            <div className="investment-panel-heading">
              <h2>{section === "savings" ? "存款与现金" : "投资持仓"}</h2>
              {section === "holdings" && (
                <p>
                  {investments.length} 项资产 · {closedInvestments.length}{" "}
                  项已清仓
                </p>
              )}
            </div>
            <div className="trade-toolbar">
              {section === "holdings" && investments.length > 0 && (
                <button
                  className="secondary"
                  disabled={busy || Boolean(editing)}
                  onClick={async () => {
                    setBusy(true);
                    setError("");
                    try {
                      const result = await refreshSecurityPrices();
                      onUpdate(result.snapshot);
                      if (result.errors.length)
                        setError(result.errors.join("；"));
                      else
                        flash(`价格已检查，更新 ${result.updatedCount} 项持仓`);
                    } catch (e) {
                      setError(`价格更新失败：${String(e)}`);
                    } finally {
                      setBusy(false);
                    }
                  }}
                >
                  <RefreshCw size={15} />
                  {busy ? "更新中…" : "更新自动价格"}
                </button>
              )}
              {section === "holdings" && (
                <button
                  className="primary"
                  disabled={busy || Boolean(editing) || addingTrade}
                  onClick={() => setAddingTrade(true)}
                >
                  <Plus size={16} />
                  记录买入
                </button>
              )}
              <button
                className="secondary"
                disabled={busy || Boolean(editing) || addingTrade}
                onClick={add}
              >
                <Plus size={16} />
                {section === "savings" ? "添加存款" : "添加持仓"}
              </button>
            </div>
          </div>
          {section === "holdings" && investments.length > 0 && (
            <InvestmentOverview
              holdings={investments}
              summaries={snapshot.holdingTradeSummaries}
            />
          )}
          {section === "holdings" && investments.length > 0 && (
            <p className="investment-sort-note">
              按折合 {currency} 市值从高到低排列 · 已清仓按最近交易排列
              {investments.some(
                (item) =>
                  !isClosedHolding(
                    item,
                    Boolean(snapshot.holdingTradeSummaries?.[item.id]),
                  ) &&
                  item.currency !== currency &&
                  !(item.fxRateToBase != null && item.fxRateToBase > 0),
              ) && " · 缺少汇率的资产排在未清仓资产末尾"}
            </p>
          )}
          {section === "holdings" && closedInvestments.length > 0 && (
            <div
              className="position-filters"
              role="group"
              aria-label="持仓状态筛选"
            >
              {(
                [
                  ["all", "全部", investments.length],
                  [
                    "open",
                    "未清仓",
                    investments.length - closedInvestments.length,
                  ],
                  ["closed", "已清仓", closedInvestments.length],
                ] as const
              ).map(([id, label, count]) => (
                <button
                  key={id}
                  aria-pressed={effectivePositionFilter === id}
                  onClick={() => setPositionFilter(id)}
                >
                  {label}
                  <span>{count}</span>
                </button>
              ))}
            </div>
          )}
          <div
            className={`facts-assets${section === "holdings" ? " investment-assets" : ""}`}
          >
            {(section === "savings" ? cash : visibleInvestments).map((item) => {
              const trades = (snapshot.holdingTrades ?? []).filter(
                (t) => t.holdingId === item.id,
              );
              const tradeSummary = snapshot.holdingTradeSummaries?.[item.id];
              return (
                <div key={item.id} className="facts-asset-entry">
                  <AssetAmountRow
                    holding={item}
                    realizedPnl={
                      tradeSummary
                        ? Number(tradeSummary.settledProfit)
                        : undefined
                    }
                    revision={snapshot.holdingRevisions?.[item.id]}
                    valuation={snapshot.holdingValuations?.find(
                      (v) => v.holdingId === item.id,
                    )}
                    onUpdate={onUpdate}
                    disabled={busy}
                    canDelete={!trades.length}
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
                  {item.assetClass !== "现金" && (
                    <details className="holding-trade-details">
                      <summary>
                        交易明细
                        {snapshot.clearedTradeHoldings?.includes(item.id)
                          ? " · 需重新录入"
                          : trades.length
                            ? ` · ${trades.length} 笔`
                            : " · 记买入 / 卖出"}
                      </summary>
                      <HoldingTrades
                        snapshot={snapshot}
                        holding={item}
                        onUpdate={onUpdate}
                        flash={flash}
                      />
                    </details>
                  )}
                </div>
              );
            })}
            {section === "holdings" &&
              investments.length > 0 &&
              visibleInvestments.length === 0 && (
                <p className="empty-state">当前筛选下没有持仓。</p>
              )}
            {(section === "savings" ? cash : investments).length === 0 && (
              <p className="empty">暂无资产，点击上方添加。</p>
            )}
          </div>
          {section === "holdings" && (
            <details className="investment-help">
              <summary>持仓与盈亏说明</summary>
              <p className="section-intro">
                用“记录买入”从最早一笔真实成交开始，按账户分别录入买入、卖出和现金分红，数量、成本和已结算盈亏自动计算。首次交易会按成交记录替换手工数量和成本，不自动补期初余额。成交与当前行情分别管理；填写证券代码后自动更新当前价格，无代码的资产请手动核对价格。浮动盈亏反映剩余持仓，已结算盈亏包含卖出盈亏与净现金分红。现金余额需按真实账户余额另行核对。
              </p>
            </details>
          )}
        </section>
      )}
      {addingTrade && (
        <section className="panel form-panel">
          <HoldingTrades
            snapshot={snapshot}
            onUpdate={onUpdate}
            flash={flash}
            onClose={() => setAddingTrade(false)}
          />
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
                )}
                {editing.assetClass !== "现金" && (
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
                )}
                <label>
                  <span>
                    {editing.assetClass === "现金" ? "当前金额" : "当前市值"}
                  </span>
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
                          : editing.quantity != null &&
                              editing.currentPrice != null
                            ? formatNumber(
                                editing.quantity * editing.currentPrice,
                              )
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
                        disabled={tradeManaged}
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
                  汇率自动读取；如需调整，可在顶部“汇率设置”统一修改。
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
      <div className="facts-next">
        <button className="primary" onClick={() => navigate("advisor")}>
          去聊聊 <ArrowRight size={16} />
        </button>
      </div>
    </div>
  );
}
