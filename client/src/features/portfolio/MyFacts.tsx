import { holdingExactValues } from "./HoldingFields";
import { Drawer } from "../../components/Drawer";
import { AssetAmountRow } from "./AssetAmountRow";
import { AssetHistory } from "./AssetHistory";
import { HoldingTrades } from "./HoldingTrades";
import { InvestmentOverview } from "./InvestmentOverview";
import { isClosedHolding, sortInvestmentHoldings } from "./holdingMetrics";
import { useId, useRef, useState } from "react";
import { Plus, RefreshCw, Save, Search } from "lucide-react";
import {
  deleteHolding,
  saveHolding,
  updateHolding,
  updateHoldingAmount,
  saveProfile,
  refreshSecurityPrices,
} from "../../api/portfolio";
import type { FinancialProfile, Holding, Snapshot } from "../../types";
import { PageHeader } from "../../components/PageHeader";
import { NumberField } from "../../components/NumberField";
import { localDateValue } from "../../lib/dates";
import { formatMoney, formatNumber } from "../../lib/format";
import { emptyHolding } from "./forms";
import {
  HoldingFields,
  hasHoldingDetails,
  validHoldingDetails,
} from "./HoldingFields";

export function MyFacts({
  snapshot,
  onUpdate,
  flash,
}: {
  snapshot: Snapshot;
  onUpdate: (snapshot: Snapshot) => void;
  flash: (message: string) => void;
}) {
  const [profile, setProfile] = useState(snapshot.profile);
  const [editing, setEditing] = useState<Holding | null>(null);
  const [addingTrade, setAddingTrade] = useState(false);
  const [editingTrade, setEditingTrade] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const exactHolding = (holding: Holding): Holding => {
    const source = snapshot.accounting?.holdings[holding.id];
    return source
      ? {
          ...holding,
          exactValues: {
            marketValue: source.marketValue,
            costBasis: source.costBasis,
            quantity: source.quantity,
            costPrice: source.costPrice,
            currentPrice: source.currentPrice,
          },
        }
      : holding;
  };
  const selectedRaw = snapshot.holdings.find((item) => item.id === selectedId);
  const selected = selectedRaw ? exactHolding(selectedRaw) : undefined;
  const [showClosed, setShowClosed] = useState(false);
  const [search, setSearch] = useState("");
  const [sort, setSort] = useState<"value-desc" | "value-asc" | "name">(
    "value-desc",
  );
  const tradeManaged = Boolean(
    editing && snapshot.holdingTrades?.some((t) => t.holdingId === editing.id),
  );
  const [assetAmount, setAssetAmount] = useState("");
  const cashRevision = useRef("");
  const cashAttempt = useRef<{ key: string; id: string } | null>(null);
  const [assetCost, setAssetCost] = useState("");
  const costHelpId = useId();
  const codeHelpId = useId();
  const currencyHelpId = useId();
  const [working, setBusy] = useState(false);
  const [uncertainDelete, setUncertainDelete] = useState<{
    id: string;
    expectedRevision: string;
  } | null>(null);
  const deleting = useRef(false);
  const busy = working || Boolean(uncertainDelete);
  const [error, setError] = useState("");
  const [section, setSection] = useState<"income" | "savings" | "holdings">(
    "holdings",
  );
  const cash = snapshot.holdings.filter((item) => item.assetClass === "现金");
  const cashCurrencies = ["CNY", "HKD", "USD"];
  const cashRows = cashCurrencies.map(
    (currency) =>
      cash.find((item) => item.currency === currency) ?? {
        ...emptyHolding(currency),
        id: "",
        name: `${currency} 存款`,
        assetClass: "现金" as const,
      },
  );
  const investments = sortInvestmentHoldings(
    snapshot.holdings.filter((item) => item.assetClass !== "现金"),
    snapshot,
    sort,
  );
  const currency = snapshot.profile.baseCurrency;
  const closedInvestments = investments.filter((item) =>
    isClosedHolding(item, Boolean(snapshot.holdingTradeSummaries?.[item.id])),
  );
  const visibleInvestments = investments.filter((item) => {
    const closed = isClosedHolding(
      item,
      Boolean(snapshot.holdingTradeSummaries?.[item.id]),
    );
    return (
      (showClosed || !closed) &&
      `${item.name} ${item.symbol} ${item.assetClass} ${item.currency}`
        .toLocaleLowerCase()
        .includes(search.trim().toLocaleLowerCase())
    );
  });
  const currencyGroups = Array.from(
    new Set(visibleInvestments.map((item) => item.currency)),
  )
    .sort((a, b) =>
      a === currency ? -1 : b === currency ? 1 : a.localeCompare(b),
    )
    .map((code) => ({
      currency: code,
      holdings: visibleInvestments.filter((item) => item.currency === code),
    }));
  const validIncome =
    [
      profile.monthlyIncome,
      profile.monthlyExpense,
      profile.liabilities,
      profile.emergencyFund,
      profile.investableAssets,
    ].every((value) => Number.isFinite(value) && value >= 0 && value <= 1e15) &&
    Number.isInteger(profile.horizonYears) &&
    profile.horizonYears >= 0 &&
    profile.horizonYears <= 80 &&
    Number.isFinite(profile.maxDrawdownPct) &&
    profile.maxDrawdownPct >= 0 &&
    profile.maxDrawdownPct <= 100;
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
    (editing.assetClass === "现金" ||
      (editing.valuationDate &&
        editing.valuationDate <= localDateValue(new Date()))),
  );

  const setProfileAmount = (
    key:
      | "monthlyIncome"
      | "monthlyExpense"
      | "emergencyFund"
      | "liabilities"
      | "investableAssets",
    value: string,
  ) => {
    const exact = profile.exactAmounts ?? {
      monthlyIncome: String(profile.monthlyIncome),
      monthlyExpense: String(profile.monthlyExpense),
      emergencyFund: String(profile.emergencyFund),
      liabilities: String(profile.liabilities),
      investableAssets: String(profile.investableAssets),
    };
    setProfile({
      ...profile,
      [key]: Number(value),
      exactAmounts: { ...exact, [key]: value || "0" },
    });
  };
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
      const input = {
        ...values,
        name: values.name.trim(),
        ...(editing.assetClass === "现金" ? { cashAmount: assetAmount } : {}),
        ...(editing.assetClass !== "现金"
          ? {
              exactValues: {
                ...holdingExactValues(editing),
                ...(manualAssetAmount
                  ? { marketValue: assetAmount || "0" }
                  : {}),
                ...(!tradeManaged && editing.costPrice == null
                  ? { costBasis: assetCost || "0" }
                  : {}),
              },
            }
          : {}),
      };
      if (id && editing.assetClass === "现金") {
        const key = JSON.stringify([id, assetAmount, cashRevision.current]);
        if (cashAttempt.current?.key !== key)
          cashAttempt.current = { key, id: crypto.randomUUID() };
        onUpdate(
          await updateHoldingAmount(
            id,
            assetAmount,
            cashRevision.current,
            cashAttempt.current.id,
          ),
        );
      } else
        onUpdate(
          id ? await updateHolding(id, input) : await saveHolding(input),
        );
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
      name: "",
      assetClass: "其他",
    });
    setError("");
  };
  const submitRemoval = async (request: {
    id: string;
    expectedRevision: string;
  }) => {
    if (deleting.current) return;
    deleting.current = true;
    setBusy(true);
    setError("");
    try {
      onUpdate(await deleteHolding(request.id, request.expectedRevision));
      setUncertainDelete(null);
      if (editing?.id === request.id) setEditing(null);
      if (selectedId === request.id) setSelectedId(null);
      flash("资产已删除");
    } catch (e) {
      const uncertain = String(e).includes("结果待确认");
      setUncertainDelete(uncertain ? request : null);
      setError(uncertain ? String(e) : `删除失败：${String(e)}`);
    } finally {
      deleting.current = false;
      setBusy(false);
    }
  };
  const removeAsset = async (item: Holding) => {
    if (busy || editingTrade || deleting.current) return;
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
    await submitRemoval({ id: item.id, expectedRevision });
  };
  const errorNotice = error && (
    <div className="error-box" role="alert">
      {error}
      {uncertainDelete && (
        <button
          type="button"
          className="secondary"
          disabled={working}
          onClick={() => void submitRemoval(uncertainDelete)}
        >
          核对删除结果
        </button>
      )}
    </div>
  );
  const renderAssetRow = (item: Holding) => {
    const tradeSummary = snapshot.holdingTradeSummaries?.[item.id];
    return (
      <div key={item.id || item.currency} className="facts-asset-entry">
        <AssetAmountRow
          holding={exactHolding(item)}
          exact={snapshot.accounting?.holdings[item.id]}
          realizedPnl={tradeSummary?.settledProfit}
          onOpen={() => {
            if (item.assetClass === "现金") {
              cashRevision.current = snapshot.holdingRevisions?.[item.id] ?? "";
              setAssetAmount(
                snapshot.cashBalances?.find((b) => b.holdingId === item.id)
                  ?.amount ?? String(item.marketValue),
              );
              setEditing(exactHolding(item));
              setError("");
            } else setSelectedId(item.id);
          }}
          disabled={busy}
          canDelete={item.assetClass !== "现金"}
          onDelete={() => void removeAsset(item)}
          onEdit={() => {
            cashRevision.current = snapshot.holdingRevisions?.[item.id] ?? "";
            setAssetAmount(
              snapshot.cashBalances?.find((b) => b.holdingId === item.id)
                ?.amount ?? String(item.marketValue),
            );
            setAssetCost(item.costBasis > 0 ? String(item.costBasis) : "");
            setEditing(exactHolding(item));
            setError("");
          }}
        />
      </div>
    );
  };
  return (
    <div className="page narrow facts-page">
      <PageHeader
        title="我的情况"
        action={
          section === "holdings" ? (
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
              {section === "holdings" && (
                <button
                  className="secondary"
                  disabled={busy || Boolean(editing) || addingTrade}
                  onClick={add}
                >
                  <Plus size={16} />
                  添加持仓
                </button>
              )}
            </div>
          ) : undefined
        }
      />
      {addingTrade && (
        <Drawer title="记录买入" onClose={() => setAddingTrade(false)}>
          <HoldingTrades
            snapshot={snapshot}
            onUpdate={onUpdate}
            flash={flash}
            onClose={() => setAddingTrade(false)}
          />
        </Drawer>
      )}
      <div className="section-switcher" role="group" aria-label="我的情况分类">
        {(
          [
            ["holdings", "投资持仓"],
            ["savings", "存款与现金"],
            ["income", "收入与支出"],
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
      {!editing && !selected && errorNotice}
      {section === "holdings" &&
        Boolean(snapshot.priceRefreshErrors?.length) && (
          <p className="warning-text" role="status">
            部分价格更新失败，已保留原值：
            {snapshot.priceRefreshErrors?.join("；")}
          </p>
        )}
      {snapshot.valuationStatus.decisionReady === false &&
        snapshot.valuationStatus.warnings.length > 0 && (
          <p className="warning-text">
            行动建议已暂停：{snapshot.valuationStatus.warnings.join("；")}
          </p>
        )}
      {section !== "income" && (
        <InvestmentOverview
          holdings={section === "holdings" ? investments : cash}
          accounting={snapshot.accounting}
          baseCurrency={currency}
          cash={section === "savings"}
        />
      )}
      {snapshot.cashBalances
        ?.filter((b) => b.needsReconciliation)
        .map((b) => (
          <p className="error-box" role="alert" key={b.currency}>
            {b.currency}{" "}
            存款待核对：基准之前的交易发生更正，当前余额已保留。请按实际余额重新确认。
          </p>
        ))}
      {section === "savings" && (
        <p className="section-intro">
          按币种核对实际余额，保存后建立新的核对基准。新增成交选择结算当前存款，历史补录不重放已包含的现金流；基准之前的交易更正会提示重新核对余额。
        </p>
      )}
      {section === "savings" &&
        cash.some((item) => !cashCurrencies.includes(item.currency)) && (
          <p className="warning-text">
            旧数据中还有其他币种存款，已保留并计入总资产：
            {cash
              .filter((item) => !cashCurrencies.includes(item.currency))
              .map(
                (item) => `${item.currency} ${formatNumber(item.marketValue)}`,
              )
              .join("；")}
            。请先核对并处理这些余额。
          </p>
        )}
      {section === "income" ? (
        <section className="panel form-panel" aria-label="收支与风险设置">
          <h2>收入与支出</h2>
          {!validIncome && (
            <p className="warning-text" role="alert">
              请核对金额和投资偏好：金额不能为负，投资期限为 0—80 年，回撤为
              0—100%。
            </p>
          )}
          <form
            onSubmit={(event) => {
              event.preventDefault();
              void saveIncome();
            }}
          >
            <fieldset className="holding-fields" disabled={busy}>
              <div className="form-grid">
                <NumberField
                  label="月收入"
                  value={
                    profile.exactAmounts?.monthlyIncome ?? profile.monthlyIncome
                  }
                  onChange={(v) => setProfileAmount("monthlyIncome", v)}
                  prefix={profile.baseCurrency}
                />
                <NumberField
                  label="月支出"
                  value={
                    profile.exactAmounts?.monthlyExpense ??
                    profile.monthlyExpense
                  }
                  onChange={(v) => setProfileAmount("monthlyExpense", v)}
                  prefix={profile.baseCurrency}
                />
              </div>
              <details className="holding-details">
                <summary>有负债或预留备用金？（选填）</summary>
                <div className="form-grid">
                  <NumberField
                    label="尚未还清的负债"
                    value={
                      profile.exactAmounts?.liabilities ?? profile.liabilities
                    }
                    onChange={(v) => setProfileAmount("liabilities", v)}
                    prefix={profile.baseCurrency}
                  />
                  <NumberField
                    label="存款中预留的备用金"
                    value={
                      profile.exactAmounts?.emergencyFund ??
                      profile.emergencyFund
                    }
                    onChange={(v) => setProfileAmount("emergencyFund", v)}
                    prefix={profile.baseCurrency}
                  />
                </div>
                <p className="section-intro">
                  备用金是存款中留作急用的部分，不会再次计入资产。
                </p>
              </details>
              <details className="holding-details">
                <summary>投资偏好与记账币种</summary>
                <div className="form-grid">
                  <label>
                    <span>基准币种</span>
                    <select
                      aria-label="基准币种"
                      aria-describedby={currencyHelpId}
                      value={profile.baseCurrency}
                      onChange={(event) =>
                        setProfile({
                          ...profile,
                          baseCurrency: event.target.value,
                        })
                      }
                    >
                      {["CNY", "USD", "HKD", "EUR", "JPY", "GBP"].map(
                        (value) => (
                          <option key={value}>{value}</option>
                        ),
                      )}
                    </select>
                    <small id={currencyHelpId}>
                      汇总和目标使用此币种；切换不会自动换算已填金额。
                    </small>
                  </label>
                  <NumberField
                    label="可投资资产"
                    value={
                      profile.exactAmounts?.investableAssets ??
                      profile.investableAssets
                    }
                    prefix={profile.baseCurrency}
                    onChange={(value) =>
                      setProfileAmount("investableAssets", value)
                    }
                  />
                  <NumberField
                    label="投资期限"
                    value={profile.horizonYears}
                    suffix="年"
                    onChange={(value) =>
                      setProfile({ ...profile, horizonYears: Number(value) })
                    }
                  />
                  <NumberField
                    label="可承受最大回撤"
                    value={profile.maxDrawdownPct}
                    suffix="%"
                    onChange={(value) =>
                      setProfile({ ...profile, maxDrawdownPct: Number(value) })
                    }
                  />
                  <label>
                    <span>风险倾向</span>
                    <select
                      value={profile.riskLevel}
                      onChange={(event) =>
                        setProfile({
                          ...profile,
                          riskLevel: event.target
                            .value as FinancialProfile["riskLevel"],
                        })
                      }
                    >
                      {["保守", "稳健", "均衡", "进取"].map((value) => (
                        <option key={value}>{value}</option>
                      ))}
                    </select>
                  </label>
                </div>
              </details>
              <div className="form-actions">
                <button
                  className="primary"
                  type="submit"
                  disabled={!validIncome}
                >
                  <Save size={16} />
                  {busy ? "保存中…" : "保存收支"}
                </button>
              </div>
            </fieldset>
          </form>
        </section>
      ) : (
        <section
          className={`panel form-panel${section === "holdings" ? " investment-panel" : ""}`}
        >
          {section === "holdings" && investments.length > 0 && (
            <p className="sr-only" id="holdings-sort-help">
              {sort === "name"
                ? "按币种分组，组内按名称排列 · 已清仓默认收起"
                : `按币种分组，组内按折合 ${currency} 市值${sort === "value-asc" ? "从低到高" : "从高到低"}排列 · 已清仓默认收起，展开后按最近交易排列`}
              {sort !== "name" &&
                investments.some(
                  (item) =>
                    !isClosedHolding(
                      item,
                      Boolean(snapshot.holdingTradeSummaries?.[item.id]),
                    ) &&
                    item.currency !== currency &&
                    !(item.fxRateToBase != null && item.fxRateToBase > 0),
                ) &&
                " · 缺少汇率的资产排在未清仓资产末尾"}
            </p>
          )}
          {section === "holdings" && investments.length > 0 && (
            <div className="holdings-toolbar" data-presentation="true">
              <div className="position-filters">
                <span>按币种分组</span>
                {closedInvestments.length > 0 && (
                  <button
                    aria-expanded={showClosed}
                    onClick={() => setShowClosed((value) => !value)}
                  >
                    {showClosed ? "收起已清仓" : "展开已清仓"}
                    <span>{closedInvestments.length}</span>
                  </button>
                )}
              </div>
              <div className="holdings-tools">
                <label className="holdings-search">
                  <Search size={16} aria-hidden="true" />
                  <input
                    type="search"
                    aria-label="搜索持仓"
                    placeholder="搜索名称、代码或币种"
                    value={search}
                    onChange={(event) => setSearch(event.target.value)}
                  />
                </label>
                <select
                  aria-label="持仓排序"
                  aria-describedby="holdings-sort-help"
                  value={sort}
                  onChange={(event) =>
                    setSort(event.target.value as typeof sort)
                  }
                >
                  <option value="value-desc">市值从高到低</option>
                  <option value="value-asc">市值从低到高</option>
                  <option value="name">名称排序</option>
                </select>
              </div>
            </div>
          )}
          <div
            className={`facts-assets${section === "holdings" ? " investment-assets" : ""}`}
            role="table"
            aria-label={section === "holdings" ? "持仓列表" : "现金列表"}
            aria-colcount={section === "holdings" ? 7 : 3}
          >
            {section === "holdings" && investments.length > 0 && (
              <div role="row" className="holdings-list-header">
                {[
                  "资产 / 币种",
                  "持仓数",
                  "现价 / 成本价",
                  "当前市值",
                  "浮动盈亏",
                  "已结算收益",
                  "操作",
                ].map((label) => (
                  <span role="columnheader" key={label}>
                    {label}
                  </span>
                ))}
              </div>
            )}
            {section === "savings" && (
              <div role="row" className="holdings-list-header cash-list-header">
                {["币种", "余额", "操作"].map((label) => (
                  <span role="columnheader" key={label}>
                    {label}
                  </span>
                ))}
              </div>
            )}
            {section === "savings" ? (
              <div role="rowgroup">{cashRows.map(renderAssetRow)}</div>
            ) : (
              currencyGroups.map((group) => (
                <div
                  role="rowgroup"
                  aria-label={group.currency + " 持仓"}
                  key={group.currency}
                >
                  <div role="row" className="holdings-currency-header">
                    <div role="cell" aria-colspan={7}>
                      <strong>{group.currency}</strong>
                      <span>{group.holdings.length} 项持仓</span>
                    </div>
                  </div>
                  {group.holdings.map(renderAssetRow)}
                </div>
              ))
            )}
            {section === "holdings" &&
              investments.length > 0 &&
              visibleInvestments.length === 0 && (
                <div className="empty-state" role="row">
                  <div role="cell" aria-colspan={7}>
                    <p>
                      {search.trim()
                        ? "没有找到匹配的持仓。"
                        : "暂无未清仓持仓，可点击上方展开已清仓持仓。"}
                    </p>
                    {search.trim() && (
                      <button
                        className="text-button"
                        onClick={() => setSearch("")}
                      >
                        清除筛选
                      </button>
                    )}
                  </div>
                </div>
              )}
            {section === "holdings" && investments.length === 0 && (
              <div role="row">
                <div role="cell" aria-colspan={7}>
                  <p className="empty">暂无资产，点击上方添加。</p>
                </div>
              </div>
            )}
          </div>
          {section === "holdings" && (
            <details className="investment-help">
              <summary>持仓与盈亏说明</summary>
              <p className="section-intro">
                用“记录买入”从最早一笔真实成交开始，按账户分别录入买入、卖出和现金分红，数量、成本和已结算盈亏自动计算。首次交易会按成交记录替换手工数量和成本，不自动补期初余额。成交与当前行情分别管理；填写证券代码后可刷新当前价格，无代码的资产请手动核对价格。浮动盈亏反映剩余持仓，已结算盈亏包含卖出盈亏与净现金分红。新增结算扣加同币种存款；历史补录不重放已包含的现金流，核对基准之前的更正会提示重新核对余额。
              </p>
            </details>
          )}
        </section>
      )}

      {(editing || selected) && (
        <Drawer
          title={
            editing
              ? editing.id
                ? "编辑 " + editing.name
                : editing.assetClass === "现金"
                  ? "添加存款"
                  : "添加持仓"
              : selected!.name
          }
          resetKey={editing ? "edit" : "detail"}
          busy={busy}
          onClose={() => {
            setEditing(null);
            setSelectedId(null);
            setError("");
          }}
        >
          {editing ? (
            <>
              {errorNotice}
              <form
                onSubmit={(e) => {
                  e.preventDefault();
                  void saveAsset();
                }}
              >
                <fieldset className="holding-fields" disabled={busy}>
                  <div className="form-grid">
                    {editing.assetClass !== "现金" && (
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
                    )}
                    {editing.assetClass === "现金" && (
                      <p>
                        {editing.currency} 存款 ·
                        填写当前余额，之后的交易会自动结算。
                      </p>
                    )}
                    {editing.assetClass !== "现金" && (
                      <label>
                        <span id={`${codeHelpId}-label`}>
                          证券代码（自动价格，选填）
                        </span>
                        <input
                          aria-labelledby={`${codeHelpId}-label`}
                          aria-describedby={
                            tradeManaged ? codeHelpId : undefined
                          }
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
                        {editing.assetClass === "现金"
                          ? "当前金额"
                          : "当前市值"}
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
                              readOnly={
                                tradeManaged || editing.costPrice != null
                              }
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
                  {editing.assetClass !== "现金" && (
                    <details className="holding-details">
                      <summary>
                        币种、日期等（{editing.valuationDate} /{" "}
                        {editing.currency}）
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
                            {["CNY", "USD", "HKD", "EUR", "JPY", "GBP"].map(
                              (c) => (
                                <option key={c}>{c}</option>
                              ),
                            )}
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
                        {
                          <label>
                            <span>类别</span>
                            <select
                              disabled={tradeManaged}
                              value={editing.assetClass}
                              onChange={(e) =>
                                setEditing({
                                  ...editing,
                                  assetClass: e.target
                                    .value as Holding["assetClass"],
                                })
                              }
                            >
                              {["股票", "基金", "债券", "黄金", "其他"].map(
                                (c) => (
                                  <option key={c}>{c}</option>
                                ),
                              )}
                            </select>
                          </label>
                        }
                      </div>
                    </details>
                  )}
                  {editing.currency !== currency && (
                    <p className="section-intro">
                      汇率自动读取；如需调整，可在右侧“汇率”浮窗的“管理汇率”中修改。
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
            </>
          ) : selected ? (
            <>
              {errorNotice}
              <div className="detail-actions">
                <button
                  className="secondary"
                  disabled={busy || editingTrade}
                  title={editingTrade ? "请先保存或取消当前交易" : undefined}
                  onClick={() => {
                    setAssetAmount(String(selected.marketValue));
                    setAssetCost(
                      selected.costBasis > 0 ? String(selected.costBasis) : "",
                    );
                    setEditing({ ...selected });
                  }}
                >
                  编辑资产
                </button>
                <button
                  type="button"
                  className="secondary danger-text"
                  disabled={busy || editingTrade}
                  title={editingTrade ? "请先保存或取消当前交易" : undefined}
                  onClick={() => void removeAsset(selected)}
                >
                  删除持仓
                </button>
              </div>
              <dl className="detail-grid">
                <div>
                  <dt>当前市值</dt>
                  <dd>
                    {formatMoney(
                      selected.exactValues?.marketValue ?? selected.marketValue,
                      selected.currency,
                    )}
                  </dd>
                </div>
                <div>
                  <dt>持仓总成本</dt>
                  <dd>
                    {selected.costBasis > 0 ||
                    isClosedHolding(
                      selected,
                      Boolean(snapshot.holdingTradeSummaries?.[selected.id]),
                    )
                      ? formatMoney(
                          selected.exactValues?.costBasis ?? selected.costBasis,
                          selected.currency,
                        )
                      : "—"}
                  </dd>
                </div>
                <div>
                  <dt>已结算盈亏</dt>
                  <dd>
                    {snapshot.holdingTradeSummaries?.[selected.id]
                      ? formatMoney(
                          Number(
                            snapshot.holdingTradeSummaries[selected.id]
                              .settledProfit,
                          ),
                          selected.currency,
                        )
                      : "未录入交易"}
                  </dd>
                </div>
                <div>
                  <dt>估值日期</dt>
                  <dd>{selected.valuationDate || "待补"}</dd>
                </div>
                <div>
                  <dt>证券代码 / 币种</dt>
                  <dd>
                    {selected.symbol || "—"} / {selected.currency}
                  </dd>
                </div>
                <div>
                  <dt>行情来源</dt>
                  <dd>
                    {snapshot.holdingValuations?.find(
                      (v) => v.holdingId === selected.id,
                    )?.providerName || "手动记录"}
                  </dd>
                </div>
              </dl>
              {snapshot.holdingValuations
                ?.filter((v) => v.holdingId === selected.id)
                .map((v) => (
                  <p className="section-intro" key={v.holdingId}>
                    收盘日 {v.observedOn} · {v.disclaimer}
                  </p>
                ))}
              {selected.assetClass !== "现金" && (
                <>
                  <h3>交易明细</h3>
                  {snapshot.clearedTradeHoldings?.includes(selected.id) && (
                    <p className="warning-text">交易明细需重新录入</p>
                  )}
                  <fieldset disabled={busy} className="holding-trades-controls">
                    <HoldingTrades
                      key={selected.id}
                      snapshot={snapshot}
                      holding={selected}
                      onEditingChange={setEditingTrade}
                      onUpdate={onUpdate}
                      flash={flash}
                    />
                  </fieldset>
                </>
              )}
            </>
          ) : null}
        </Drawer>
      )}
      <AssetHistory snapshot={snapshot} />
    </div>
  );
}
