import { Drawer } from "../../components/Drawer";
import { AssetAmountRow } from "./AssetAmountRow";
import { AssetEditor } from "./AssetEditor";
import { AssetHistory } from "./AssetHistory";
import { HoldingDetail } from "./HoldingDetail";
import { HoldingTrades } from "./HoldingTrades";
import { InvestmentOverview } from "./InvestmentOverview";
import { ProfileForm } from "./ProfileForm";
import { isClosedHolding, sortInvestmentHoldings } from "./holdingMetrics";
import { useRef, useState } from "react";
import { Plus, RefreshCw, Search } from "lucide-react";
import { deleteHolding, refreshSecurityPrices } from "../../api/portfolio";
import type { Holding, Snapshot } from "../../types";
import { PageHeader } from "../../components/PageHeader";
import { formatNumber } from "../../lib/format";
import { emptyHolding } from "./forms";

const cashCurrencies = ["CNY", "HKD", "USD"];

function withExactValues(snapshot: Snapshot, holding: Holding): Holding {
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
}

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
  const selectedRaw = snapshot.holdings.find((item) => item.id === selectedId);
  const selected = selectedRaw
    ? withExactValues(snapshot, selectedRaw)
    : undefined;
  const [showClosed, setShowClosed] = useState(false);
  const [search, setSearch] = useState("");
  const [sort, setSort] = useState<"value-desc" | "value-asc" | "name">(
    "value-desc",
  );
  const cashAttempt = useRef<{ key: string; id: string } | null>(null);
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

  const run = async (
    task: () => Promise<void>,
    failure: (error: unknown) => string = String,
  ) => {
    setBusy(true);
    setError("");
    try {
      await task();
    } catch (e) {
      setError(failure(e));
    } finally {
      setBusy(false);
    }
  };
  const cashAttemptId = (key: string) => {
    if (cashAttempt.current?.key !== key)
      cashAttempt.current = { key, id: crypto.randomUUID() };
    return cashAttempt.current.id;
  };
  const openEditor = (holding: Holding) => {
    setEditing(withExactValues(snapshot, holding));
    setError("");
  };
  const refreshPrices = () =>
    run(
      async () => {
        const result = await refreshSecurityPrices();
        onUpdate(result.snapshot);
        if (result.errors.length) setError(result.errors.join("；"));
        else flash(`价格已检查，更新 ${result.updatedCount} 项持仓`);
      },
      (e) => `价格更新失败：${String(e)}`,
    );
  const add = () =>
    openEditor({
      ...emptyHolding(currency),
      id: "",
      name: "",
      assetClass: "其他",
    });
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
          holding={withExactValues(snapshot, item)}
          exact={snapshot.accounting?.holdings[item.id]}
          realizedPnl={tradeSummary?.settledProfit}
          onOpen={() => {
            if (item.assetClass === "现金") openEditor(item);
            else setSelectedId(item.id);
          }}
          disabled={busy}
          canDelete={item.assetClass !== "现金"}
          onDelete={() => void removeAsset(item)}
          onEdit={() => openEditor(item)}
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
                  onClick={refreshPrices}
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
        <ProfileForm
          profile={profile}
          onChange={setProfile}
          busy={busy}
          run={run}
          onUpdate={onUpdate}
          flash={flash}
        />
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
          {errorNotice}
          {editing ? (
            <AssetEditor
              snapshot={snapshot}
              initial={editing}
              busy={busy}
              run={run}
              cashAttemptId={cashAttemptId}
              onUpdate={onUpdate}
              onSaved={(message) => {
                setEditing(null);
                flash(message);
              }}
            />
          ) : selected ? (
            <HoldingDetail
              snapshot={snapshot}
              holding={selected}
              busy={busy}
              editingTrade={editingTrade}
              onEditingTradeChange={setEditingTrade}
              onEdit={() => setEditing({ ...selected })}
              onDelete={() => void removeAsset(selected)}
              onUpdate={onUpdate}
              flash={flash}
            />
          ) : null}
        </Drawer>
      )}
      <AssetHistory snapshot={snapshot} />
    </div>
  );
}
